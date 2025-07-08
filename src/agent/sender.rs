use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::rate_limiter::RateLimitingMethod;
use caracat::sender::Sender as CaracatSender;
use metrics::counter;
use metrics::Label;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::Receiver as TokioReceiver;
use tracing::warn;
use tracing::{debug, error, info, trace};

use crate::config::CaracatConfig;

// Type to represent probes with their source IP
#[derive(Debug)]
pub struct ProbesWithSource {
    pub probes: Vec<Probe>,
    pub source_ip: String,
}

pub struct SendLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl SendLoop {
    pub fn new(
        mut rx: TokioReceiver<ProbesWithSource>,
        agent_id: String,
        config: CaracatConfig,
        runtime_handle: TokioHandle,
    ) -> Self {
        let method = match config.rate_limiting_method.to_lowercase().as_str() {
            "auto" => RateLimitingMethod::Auto,
            "active" => RateLimitingMethod::Active,
            "sleep" => RateLimitingMethod::Sleep,
            "none" => RateLimitingMethod::None,
            other => {
                warn!(
                    "Unknown rate_limiting_method '{}', defaulting to 'auto'",
                    other
                );
                RateLimitingMethod::Auto
            }
        };
        let mut rate_limiter = RateLimiter::new(config.probing_rate, config.batch_size, method);

        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();
        let interface_name = config.interface.clone();

        let metrics_labels = vec![Label::new("agent", agent_id.to_string())];

        // Clone the handle to move into the thread
        let thread_runtime_handle = runtime_handle.clone();

        let handle = thread::spawn(move || {
            debug!("SendLoop thread started for interface: {}", interface_name);

            // Cache of CaracatSender instances per source IP
            let mut caracat_senders: HashMap<String, CaracatSender> = HashMap::new();

            // Extra logging for debugging SendLoop lifecycle
            info!("SendLoop for interface {} is running.", config.interface);

            loop {
                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping SendLoop for interface: {}", config.interface);
                    break;
                }

                let probes_with_source = match thread_runtime_handle.block_on(rx.recv()) {
                    Some(p) => p,
                    None => {
                        error!(
                            "Probe channel closed for SendLoop on interface {}. Exiting loop.",
                            config.interface
                        );
                        break;
                    }
                };

                let source_ip = probes_with_source.source_ip.clone();
                let probes = probes_with_source.probes;

                counter!("saimiris_sender_read_total", metrics_labels.clone())
                    .increment(probes.len().try_into().unwrap_or(0));

                // Determine if we should use a specific source IP or default behavior
                let use_default_source = source_ip.is_empty();
                let sender_key = if use_default_source {
                    "default".to_string()
                } else {
                    source_ip.clone()
                };

                // Get or create CaracatSender for this sender key
                let caracat_sender = match caracat_senders.get_mut(&sender_key) {
                    Some(sender) => sender,
                    None => {
                        let (src_ipv4, src_ipv6) = if use_default_source {
                            // Use default behavior - let CaracatSender choose source IPs
                            (None, None)
                        } else {
                            // Parse the source IP to determine if it's IPv4 or IPv6
                            let parsed_ip: IpAddr = match source_ip.parse() {
                                Ok(ip) => ip,
                                Err(e) => {
                                    error!(
                                        "Invalid source IP address '{}': {}. Skipping probes.",
                                        source_ip, e
                                    );
                                    continue;
                                }
                            };

                            match parsed_ip {
                                IpAddr::V4(ipv4) => (Some(ipv4), None),
                                IpAddr::V6(ipv6) => (None, Some(ipv6)),
                            }
                        };

                        let caracat_sender_result = CaracatSender::new(
                            &config.interface,
                            src_ipv4,
                            src_ipv6,
                            config.instance_id,
                            config.dry_run,
                        );

                        match caracat_sender_result {
                            Ok(sender) => {
                                if use_default_source {
                                    debug!(
                                        "Created new CaracatSender with default source IP behavior on interface {}",
                                        config.interface
                                    );
                                } else {
                                    debug!(
                                        "Created new CaracatSender for source IP {} on interface {}",
                                        source_ip, config.interface
                                    );
                                }
                                caracat_senders.insert(sender_key.clone(), sender);
                                caracat_senders.get_mut(&sender_key).unwrap()
                            }
                            Err(e) => {
                                if use_default_source {
                                    error!(
                                        "Failed to create Caracat sender with default source IP behavior on interface {}: {}. Skipping probes.",
                                        config.interface, e
                                    );
                                } else {
                                    error!(
                                        "Failed to create Caracat sender for source IP {} on interface {}: {}. Skipping probes.",
                                        source_ip, config.interface, e
                                    );
                                }
                                continue;
                            }
                        }
                    }
                };

                let mut sent_count_batch = 0;

                for probe in probes {
                    if *stopped_thr.lock().unwrap() {
                        trace!(
                            "Stopping SendLoop mid-batch for interface: {}",
                            config.interface
                        );
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
