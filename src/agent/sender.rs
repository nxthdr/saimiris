use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::sender::Sender as CaracatSender;
use metrics::counter;
use metrics::Label;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender};
use tracing::{debug, error, info, trace, warn};

use crate::config::CaracatConfig;

#[allow(dead_code)]
pub struct SendLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl SendLoop {
    pub fn new(
        mut rx: TokioReceiver<Vec<Probe>>,
        feedback: TokioSender<bool>,
        agent_id: String,
        config: CaracatConfig,
        runtime_handle: TokioHandle,
    ) -> Self {
        let mut rate_limiter = RateLimiter::new(
            config.probing_rate,
            config.batch_size,
            config.rate_limiting_method,
        );

        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();
        let interface_name = config.interface.clone();

        let metrics_labels = vec![Label::new("agent", agent_id.to_string())];

        let caracat_sender_result = CaracatSender::new(
            &config.interface,
            config.src_ipv4_addr,
            config.src_ipv6_addr,
            config.instance_id,
            config.dry_run,
        );

        // Clone the handle to move into the thread
        let thread_runtime_handle = runtime_handle.clone();

        let handle = thread::spawn(move || {
            debug!("SendLoop thread started for interface: {}", interface_name);

            let mut caracat_sender = match caracat_sender_result {
                Ok(s) => s,
                Err(e) => {
                    error!(
                        "Failed to create Caracat sender for interface {}: {}. SendLoop thread exiting.",
                        config.interface, e
                    );
                    if let Ok(mut s_lock) = stopped_thr.lock() {
                        *s_lock = true;
                    }
                    // Consider sending feedback if the channel is still valid, though it might also fail.
                    // thread_runtime_handle.block_on(feedback.send(false)).ok(); // Example: signal failure
                    return;
                }
            };

            loop {
                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping SendLoop for interface: {}", config.interface);
                    break;
                }

                // Use the passed runtime handle
                let probes = match thread_runtime_handle.block_on(rx.recv()) {
                    Some(p) => p,
                    None => {
                        info!(
                            "Probe channel closed for SendLoop on interface {}. Exiting loop.",
                            config.interface
                        );
                        break;
                    }
                };

                counter!("saimiris_sender_read_total", metrics_labels.clone())
                    .increment(probes.len().try_into().unwrap_or(0));

                let mut sent_count_batch = 0;

                for probe in probes {
                    if *stopped_thr.lock().unwrap() {
                        trace!(
                            "Stopping SendLoop mid-batch for interface: {}",
                            config.interface
                        );
                        // Use the passed runtime handle
                        if thread_runtime_handle
                            .block_on(feedback.send(true)) // Or false to indicate partial completion/stop
                            .is_err()
                        {
                            warn!("Failed to send feedback from SendLoop on interface {} during mid-batch stop (receiver dropped).", config.interface);
                        }
                        return;
                    }

                    trace!("{:?}", probe);

                    if let Some(ttl) = config.min_ttl {
                        if probe.ttl < ttl {
                            trace!("{:?} filter=ttl_too_low", probe);
                            counter!("saimiris_sender_filtered_total", "agent" => agent_id.clone(), "filter" => "ttl_too_low")
                                .increment(1);
                            continue;
                        }
                    }

                    if let Some(ttl) = config.max_ttl {
                        if probe.ttl > ttl {
                            trace!("{:?} filter=ttl_too_high", probe);
                            counter!("saimiris_sender_filtered_total", "agent" => agent_id.clone(), "filter" => "ttl_too_high")
                                .increment(1);
                            continue;
                        }
                    }

                    for i in 0..config.packets {
                        trace!(
                            "{:?} id={} packet={}",
                            probe,
                            probe.checksum(config.instance_id),
                            i + 1
                        );
                        match caracat_sender.send(&probe) {
                            Ok(_) => {
                                sent_count_batch += 1;
                                counter!("saimiris_sender_sent_total", metrics_labels.clone())
                                    .increment(1);
                            }
                            Err(error) => {
                                error!(
                                    "Error sending probe on interface {}: {}",
                                    config.interface, error
                                );
                                counter!("saimiris_sender_failed_total", metrics_labels.clone())
                                    .increment(1);
                            }
                        }
                        if (sent_count_batch) % config.batch_size == 0 && sent_count_batch > 0 {
                            rate_limiter.wait();
                        }
                    }
                }

                // Use the passed runtime handle
                if thread_runtime_handle.block_on(feedback.send(true)).is_err() {
                    warn!("Failed to send feedback from SendLoop on interface {} (receiver dropped). Loop may exit if this is unexpected.", config.interface);
                }
            }
            debug!("SendLoop thread finished for interface: {}", interface_name);
        });

        SendLoop { handle, stopped }
    }

    #[allow(dead_code)]
    pub fn stop(self) {
        info!("Requesting stop for SendLoop.");
        if let Ok(mut stopped_lock) = self.stopped.lock() {
            *stopped_lock = true;
        } else {
            error!("Failed to acquire lock to stop SendLoop.");
        }
        // Consider adding a timeout to join if the thread might get stuck
        match self.handle.join() {
            Ok(_) => info!("SendLoop successfully joined."),
            Err(e) => error!("Error joining SendLoop thread: {:?}", e),
        }
    }
}
