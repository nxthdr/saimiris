use caracat::models::Reply;
use caracat::receiver::Receiver;
use hyperloglog::HyperLogLog;
use log::{error, trace};
use std::fmt::Display;
use std::fmt::Formatter;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use crate::config::CaracatConfig;

#[allow(dead_code)]
pub struct ReceiveLoop {
    handle: JoinHandle<()>,
    tx: Sender<Reply>,
    stopped: Arc<Mutex<bool>>,
    statistics: Arc<Mutex<ReceiveStatistics>>,
}

impl ReceiveLoop {
    pub fn new(tx: Sender<Reply>, config: CaracatConfig) -> Self {
        // By default if a thread panic, the other threads are not affected and the error
        // is only surfaced when joining the thread. However since this is a long-lived thread,
        // we're not calling join until the end of the process. Since this loop is critical to
        // the process, we don't want it to crash silently. We currently rely on
        // `utilities::exit_process_on_panic` but we might find a better way in the future.
        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();
        let statistics = Arc::new(Mutex::new(ReceiveStatistics::default()));
        let statistics_thr = statistics.clone();

        let tx_thr = tx.clone();

        let handle = thread::spawn(move || {
            let mut receiver = Receiver::new_batch(&config.interface).unwrap();

            loop {
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
                        if !config.integrity_check
                            || (config.integrity_check && reply.is_valid(config.instance_id))
                        {
                            statistics
                                .icmp_messages_incl_dest
                                .insert(&reply.reply_src_addr);
                            if reply.is_time_exceeded() {
                                statistics
                                    .icmp_messages_excl_dest
                                    .insert(&reply.reply_src_addr);
                            }

                            tx_thr.send(reply).unwrap();
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
                    trace!("Stopping receive loop");
                    break;
                }
            }
        });

        ReceiveLoop {
            handle,
            tx,
            stopped,
            statistics,
        }
    }

    #[allow(dead_code)]
    pub fn stop(self) {
        *self.stopped.lock().unwrap() = true;
        self.handle.join().unwrap();
    }

    #[allow(dead_code)]
    pub fn statistics(&self) -> &Arc<Mutex<ReceiveStatistics>> {
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
