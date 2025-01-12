//! High-level interface for capturing replies.
use anyhow::Result;
use hyperloglog::HyperLogLog;

use std::fmt::Display;
use std::fmt::Formatter;
use std::io::{stdout, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::thread::JoinHandle;

use log::{error, info, trace};

use crate::handler::Config;
use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::receiver::Receiver;
use caracat::sender::Sender;

/// Send probes from an iterator.
pub fn probe<T: Iterator<Item = Probe>>(
    config: Config,
    probes: T,
    csv_output: Option<String>,
) -> Result<(SendStatistics, ReceiveStatistics)> {
    info!("{:?}", config);

    let rate_limiter = RateLimiter::new(
        config.probing_rate,
        config.batch_size,
        config.rate_limiting_method,
    );

    let receiver = ReceiveLoop::new(
        config.interface.clone(),
        config.instance_id,
        config.integrity_check,
        csv_output,
    );

    let mut prober = SendLoop::new(
        config.batch_size,
        config.instance_id,
        config.min_ttl,
        config.max_ttl,
        config.max_probes,
        config.packets,
        rate_limiter,
        Sender::new(
            &config.interface,
            None,
            None,
            config.instance_id,
            config.dry_run,
        )?,
    );

    prober.probe(probes)?;
    info!("Waiting {:?} for replies...", config.receiver_wait_time);
    sleep(config.receiver_wait_time);

    // TODO: Cleaner way?
    let final_prober_statistics = *prober.statistics().lock().unwrap();
    let final_receiver_statistics = receiver.statistics().lock().unwrap().clone();

    receiver.stop();

    Ok((final_prober_statistics, final_receiver_statistics))
}

// The pcap crate doesn't support `pcap_loop` and `pcap_breakloop`,
// so we implement our own looping mechanism.
pub struct ReceiveLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
    statistics: Arc<Mutex<ReceiveStatistics>>,
}

impl ReceiveLoop {
    pub fn new(
        interface: String,
        instance_id: u16,
        integrity_check: bool,
        output_csv: Option<String>,
    ) -> Self {
        // By default if a thread panic, the other threads are not affected and the error
        // is only surfaced when joining the thread. However since this is a long-lived thread,
        // we're not calling join until the end of the process. Since this loop is critical to
        // the process, we don't want it to crash silently. We currently rely on
        // `utilities::exit_process_on_panic` but we might find a better way in the future.
        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();
        let statistics = Arc::new(Mutex::new(ReceiveStatistics::default()));
        let statistics_thr = statistics.clone();

        let handle = thread::spawn(move || {
            let mut receiver = Receiver::new_batch(&interface).unwrap();

            let wtr: Box<dyn Write> = match output_csv {
                Some(output_csv) => {
                    let file = std::fs::File::create(output_csv).unwrap();
                    Box::new(std::io::BufWriter::new(file))
                }
                None => Box::new(stdout().lock()),
            };

            let mut csv_writer = csv::WriterBuilder::new()
                .has_headers(false) // TODO: Set to true, but how to serialize MPLS labels?
                .from_writer(wtr);

            loop {
                // TODO: Cleanup this loop & statistics handling
                let result = receiver.next_reply();
                let pcap_statistics = receiver.statistics().unwrap();
                let mut statistics = statistics_thr.lock().unwrap();
                statistics.pcap_received = pcap_statistics.received;
                statistics.pcap_dropped = pcap_statistics.dropped;
                statistics.pcap_if_dropped = pcap_statistics.if_dropped;
                match result {
                    Ok(reply) => {
                        trace!("{:?}", reply);
                        statistics.received += 1;
                        if integrity_check && reply.is_valid(instance_id) {
                            statistics
                                .icmp_messages_incl_dest
                                .insert(&reply.reply_src_addr);
                            if reply.is_time_exceeded() {
                                statistics
                                    .icmp_messages_excl_dest
                                    .insert(&reply.reply_src_addr);
                            }
                            csv_writer.serialize(reply).unwrap();
                            // TODO: Write round column.
                            // TODO: Compare output with caracal (capture timestamp resolution?)
                        } else {
                            trace!("invalid_reply_reason=caracat_checksum");
                            statistics.received_invalid += 1;
                        }
                    }
                    Err(error) => {
                        // TODO: Cleanup this by returning a proper error type,
                        // e.g. ReceiverError::CaptureError(...)
                        match error.downcast_ref::<pcap::Error>() {
                            Some(error) => match error {
                                pcap::Error::TimeoutExpired => {}
                                _ => error!("{:?}", error),
                            },
                            None => {
                                statistics.received += 1;
                                error!("{:?}", error)
                            }
                        }
                    }
                }

                if *stopped_thr.lock().unwrap() {
                    break;
                }
            }
            csv_writer.flush().unwrap();
        });
        ReceiveLoop {
            handle,
            stopped,
            statistics,
        }
    }

    pub fn stop(self) {
        *self.stopped.lock().unwrap() = true;
        self.handle.join().unwrap();
    }

    pub fn statistics(&self) -> &Arc<Mutex<ReceiveStatistics>> {
        &self.statistics
    }
}

pub struct SendLoop {
    batch_size: u64,
    instance_id: u16,
    min_ttl: Option<u8>,
    max_ttl: Option<u8>,
    max_probes: Option<u64>,
    packets: u64,
    rate_limiter: RateLimiter,
    sender: Sender,
    statistics: Arc<Mutex<SendStatistics>>,
}

impl SendLoop {
    pub fn new(
        batch_size: u64,
        instance_id: u16,
        min_ttl: Option<u8>,
        max_ttl: Option<u8>,
        max_probes: Option<u64>,
        packets: u64,
        rate_limiter: RateLimiter,
        sender: Sender,
    ) -> Self {
        let statistics = Arc::new(Mutex::new(SendStatistics::default()));
        SendLoop {
            batch_size,
            instance_id,
            min_ttl,
            max_ttl,
            max_probes,
            packets,
            rate_limiter,
            sender,
            statistics,
        }
    }

    pub fn probe<T: Iterator<Item = Probe>>(&mut self, probes: T) -> Result<()> {
        for probe in probes {
            let mut statistics = self.statistics.lock().unwrap();
            statistics.read += 1;

            if let Some(ttl) = self.min_ttl {
                if probe.ttl < ttl {
                    trace!("{:?} filter=ttl_too_low", probe);
                    statistics.filtered_low_ttl += 1;
                    continue;
                }
            }

            if let Some(ttl) = self.max_ttl {
                if probe.ttl > ttl {
                    trace!("{:?} filter=ttl_too_high", probe);
                    statistics.filtered_high_ttl += 1;
                    continue;
                }
            }

            for i in 0..self.packets {
                trace!(
                    "{:?} id={} packet={}",
                    probe,
                    probe.checksum(self.instance_id),
                    i + 1
                );
                match self.sender.send(&probe) {
                    Ok(_) => statistics.sent += 1,
                    Err(error) => {
                        statistics.failed += 1;
                        error!("{}", error);
                    }
                }
                // Rate limit every `batch_size` packets sent.
                if (statistics.sent + statistics.failed) % self.batch_size == 0 {
                    self.rate_limiter.wait();
                }
            }

            if let Some(max_probes) = self.max_probes {
                if statistics.sent >= max_probes {
                    info!("max_probes reached, exiting...");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn statistics(&self) -> &Arc<Mutex<SendStatistics>> {
        &self.statistics
    }
}


#[derive(Clone, Debug)]
pub struct ReceiveStatistics {
    /// Number of packets received.
    pub pcap_received: u32,
    /// Number of packets dropped because there was no room in the operating system's buffer when
    /// they arrived, because packets weren't being read fast enough.
    pub pcap_dropped: u32,
    /// Number of packets dropped by the network interface or its driver.
    pub pcap_if_dropped: u32,
    pub received: u64,
    pub received_invalid: u64,
    pub icmp_messages_incl_dest: HyperLogLog,
    pub icmp_messages_excl_dest: HyperLogLog,
}

impl Default for ReceiveStatistics {
    fn default() -> Self {
        Self {
            pcap_received: 0,
            pcap_dropped: 0,
            pcap_if_dropped: 0,
            received: 0,
            received_invalid: 0,
            icmp_messages_incl_dest: HyperLogLog::new(0.001),
            icmp_messages_excl_dest: HyperLogLog::new(0.001),
        }
    }
}

impl Display for ReceiveStatistics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "pcap_received={}", self.pcap_received)?;
        write!(f, " pcap_dropped={}", self.pcap_dropped)?;
        write!(f, " pcap_interface_dropped={}", self.pcap_if_dropped)?;
        write!(f, " packets_received={}", self.received)?;
        write!(f, " packets_received_invalid={}", self.received_invalid,)?;
        write!(
            f,
            " icmp_distinct_incl_dest={}",
            self.icmp_messages_incl_dest.len().trunc(),
        )?;
        write!(
            f,
            " icmp_distinct_excl_dest={}",
            self.icmp_messages_excl_dest.len().trunc(),
        )
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct SendStatistics {
    pub read: u64,
    pub sent: u64,
    pub failed: u64,
    pub filtered_low_ttl: u64,
    pub filtered_high_ttl: u64,
}

impl Display for SendStatistics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "probes_read={} packets_sent={} packets_failed={} filtered_low_ttl={} filtered_high_ttl={}",
               self.read, self.sent, self.failed, self.filtered_low_ttl, self.filtered_high_ttl)
    }
}
