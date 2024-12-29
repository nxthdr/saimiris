//! High-level interface for capturing replies.
use anyhow::Result;

use std::io::{stdout, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::thread::JoinHandle;

use log::{error, info, trace};

use caracat::high_level::{Config, ReceiveStatistics, SendLoop, SendStatistics};
use caracat::logger::StatisticsLogger;
use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::receiver::Receiver;
use caracat::sender::Sender;
use caracat::utilities::prefix_filter_from_file;

/// Send probes from an iterator.
pub fn probe<T: Iterator<Item = Probe>>(
    config: Config,
    probes: T,
    csv_output: Option<String>,
) -> Result<(SendStatistics, ReceiveStatistics)> {
    info!("{}", config);

    let allowed_prefixes = match config.allowed_prefixes_file {
        None => None,
        Some(path) => Some(prefix_filter_from_file(&path)?),
    };

    let blocked_prefixes = match config.blocked_prefixes_file {
        None => None,
        Some(path) => Some(prefix_filter_from_file(&path)?),
    };

    let rate_limiter = RateLimiter::new(
        config.probing_rate,
        config.batch_size,
        config.rate_limiting_method,
    );
    let rate_statistics = rate_limiter.statistics().clone();

    let receiver = ReceiveLoop::new(
        config.interface.clone(),
        config.instance_id,
        config.extra_string,
        config.integrity_check,
        csv_output,
    );
    let receiver_statistics = receiver.statistics().clone();

    let mut prober = SendLoop::new(
        config.batch_size,
        config.instance_id,
        config.min_ttl,
        config.max_ttl,
        config.max_probes,
        config.packets,
        allowed_prefixes,
        blocked_prefixes,
        rate_limiter,
        Sender::new(&config.interface, config.instance_id, config.dry_run)?,
    );
    let prober_statistics = prober.statistics().clone();

    let logger = StatisticsLogger::new(prober_statistics, rate_statistics, receiver_statistics);

    prober.probe(probes)?;
    info!("Waiting {:?} for replies...", config.receiver_wait_time);
    sleep(config.receiver_wait_time);

    // TODO: Cleaner way?
    let final_prober_statistics = *prober.statistics().lock().unwrap();
    let final_receiver_statistics = receiver.statistics().lock().unwrap().clone();

    receiver.stop();
    logger.stop();

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
        extra_string: Option<String>,
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
                    Ok(mut reply) => {
                        trace!("{}", reply);
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
                            reply.extra = extra_string.clone();
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
