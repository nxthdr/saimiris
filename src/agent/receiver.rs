use caracat::models::Reply;
use caracat::receiver::Receiver;
use metrics::counter;
use metrics::Label;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::Sender as TokioSender;
use tracing::{debug, error, info, trace};

use crate::config::CaracatConfig;

pub struct ReceiveLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl ReceiveLoop {
    fn is_valid_for_any_instance(reply: &Reply, valid_instance_ids: &[u16]) -> bool {
        valid_instance_ids
            .iter()
            .any(|&instance_id| reply.is_valid(instance_id))
    }

    pub fn new(
        tx: TokioSender<Reply>,
        agent_id: String,
        config: CaracatConfig,
        valid_instance_ids: Vec<u16>,
        runtime_handle: TokioHandle,
    ) -> Self {
        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();

        let metrics_labels = vec![Label::new("agent", agent_id.to_string())];
        let interface_name = config.interface.clone();

        let thread_runtime_handle = runtime_handle.clone();

        let handle = thread::spawn(move || {
            debug!(
                "ReceiveLoop thread started for interface: {}",
                interface_name
            );
            let mut receiver = match Receiver::new_batch(&config.interface) {
                Ok(r) => r,
                Err(e) => {
                    error!(
                        "Failed to create Caracat receiver for interface {}: {}. ReceiveLoop thread exiting.",
                        config.interface, e
                    );
                    if let Ok(mut s) = stopped_thr.lock() {
                        *s = true;
                    }
                    return;
                }
            };

            loop {
                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping receive loop for interface: {}", config.interface);
                    break;
                }

                // The `next_reply()` might block, which is fine for a std::thread.
                let result = receiver.next_reply();
                match result {
                    Ok(reply) => {
                        counter!("saimiris_receiver_received_total", metrics_labels.clone())
                            .increment(1);
                        if !config.integrity_check
                            || (config.integrity_check
                                && Self::is_valid_for_any_instance(&reply, &valid_instance_ids))
                        {
                            // Send to the Tokio MPSC channel. This is an async operation,
                            // so we need to block on it from this synchronous thread.
                            match thread_runtime_handle.block_on(tx.send(reply)) {
                                Ok(_) => {
                                    trace!(
                                        "Reply sent from ReceiveLoop for interface: {}",
                                        config.interface
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to send reply from ReceiveLoop for interface {}: {}. Receiver (Kafka producer) might have shut down. Stopping loop.",
                                        config.interface, e
                                    );
                                    break;
                                }
                            }
                        } else {
                            counter!(
                                "saimiris_receiver_received_invalid_total",
                                metrics_labels.clone()
                            )
                            .increment(1);
                        }
                    }
                    Err(error) => {
                        if *stopped_thr.lock().unwrap() {
                            trace!(
                                "Stopping receive loop for interface {} during error handling.",
                                config.interface
                            );
                            break;
                        }

                        counter!(
                            "saimiris_receiver_received_error_total",
                            metrics_labels.clone()
                        )
                        .increment(1);
                        match error.downcast_ref::<pcap::Error>() {
                            Some(pcap_error) => match pcap_error {
                                pcap::Error::TimeoutExpired => {
                                    // This is expected if pcap has a read timeout.
                                    // Continue the loop unless stopped.
                                }
                                _ => error!(
                                    "pcap error in ReceiveLoop for interface {}: {:?}",
                                    config.interface, pcap_error
                                ),
                            },
                            None => {
                                error!(
                                    "Unknown error in ReceiveLoop for interface {}: {:?}",
                                    config.interface, error
                                );
                            }
                        }
                    }
                }
            }
            debug!(
                "ReceiveLoop thread finished for interface: {}",
                interface_name
            );
        });

        ReceiveLoop { handle, stopped }
    }

    #[allow(dead_code)]
    pub fn stop(self) {
        info!("Requesting stop for ReceiveLoop.");
        if let Ok(mut stopped_lock) = self.stopped.lock() {
            *stopped_lock = true;
        } else {
            error!("Failed to acquire lock to stop ReceiveLoop.");
        }
        match self.handle.join() {
            Ok(_) => info!("ReceiveLoop successfully joined."),
            Err(e) => error!("Error joining ReceiveLoop thread: {:?}", e),
        }
    }
}
