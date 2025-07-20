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
use tracing::warn;
use tracing::{debug, error, info, trace};

use crate::config::CaracatConfig;

// Type to represent probes with their source IP and measurement tracking info
#[derive(Debug)]
pub struct ProbesWithSource {
    pub probes: Vec<Probe>,
    pub source_ip: String,
    pub measurement_info: Option<crate::agent::gateway::MeasurementInfo>,
}

pub struct SendLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl SendLoop {
    pub fn new(
        mut rx: tokio::sync::mpsc::Receiver<ProbesWithSource>,
        config: CaracatConfig,
        app_config: &crate::config::AppConfig,
        runtime_handle: TokioHandle,
    ) -> Self {
        // Extract needed values from app_config
        let agent_id = app_config.agent.id.clone();
        let gateway_url = app_config.gateway.as_ref().and_then(|g| g.url.clone());
        let agent_key = app_config
            .gateway
            .as_ref()
            .and_then(|g| g.agent_key.clone());

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
            // Track probes sent per measurement
            let mut probes_sent_in_measurement: HashMap<String, u32> = HashMap::new();

            // Extra logging for debugging SendLoop lifecycle
            info!("SendLoop for interface {} is running.", config.interface);

            loop {
                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping SendLoop for interface: {}", config.interface);
                    break;
                }

                trace!(
                    "SendLoop waiting for probes on interface: {}",
                    config.interface
                );
                let probes_with_source = match thread_runtime_handle.block_on(rx.recv()) {
                    Some(p) => {
                        trace!(
                            "SendLoop successfully received probes from channel for interface: {}",
                            config.interface
                        );
                        p
                    }
                    None => {
                        info!(
                            "Probe channel closed for SendLoop on interface {}. Exiting loop.",
                            config.interface
                        );
                        break;
                    }
                };

                let source_ip = probes_with_source.source_ip.clone();
                let measurement_info = probes_with_source.measurement_info.clone();
                let probes = probes_with_source.probes;

                trace!("SendLoop received {} probes for interface {}, source_ip: {}, measurement_id: {:?}",
                       probes.len(), config.interface, source_ip, measurement_info.as_ref().map(|m| &m.measurement_id));

                counter!("saimiris_sender_read_total", metrics_labels.clone())
                    .increment(probes.len().try_into().unwrap_or(0));

                // Determine if we should use a specific source IP or default behavior
                let use_default_source = source_ip.is_empty();
                let sender_key = if use_default_source {
                    "default".to_string()
                } else {
                    source_ip.clone()
                };

                trace!(
                    "SendLoop determining sender key: use_default_source={}, sender_key={}",
                    use_default_source,
                    sender_key
                );

                // Get or create CaracatSender for this sender key
                trace!(
                    "SendLoop looking for existing sender for key: {}",
                    sender_key
                );
                let caracat_sender = match caracat_senders.get_mut(&sender_key) {
                    Some(sender) => {
                        trace!("SendLoop found existing sender for key: {}", sender_key);
                        sender
                    }
                    None => {
                        trace!("SendLoop creating new sender for key: {}", sender_key);
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

                        trace!("SendLoop attempting to create CaracatSender with src_ipv4: {:?}, src_ipv6: {:?}", src_ipv4, src_ipv6);

                        // Create the sender with a timeout to prevent hanging
                        let interface_name = config.interface.clone();
                        let instance_id = config.instance_id;
                        let dry_run = config.dry_run;

                        let caracat_sender_result = thread_runtime_handle.block_on(async {
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                tokio::task::spawn_blocking(move || {
                                    CaracatSender::new(
                                        &interface_name,
                                        src_ipv4,
                                        src_ipv6,
                                        instance_id,
                                        dry_run,
                                    )
                                }),
                            )
                            .await
                            {
                                Ok(Ok(join_result)) => join_result,
                                Ok(Err(e)) => Err(anyhow::anyhow!(
                                    "CaracatSender::new() task panicked: {:?}",
                                    e
                                )),
                                Err(_) => Err(anyhow::anyhow!(
                                    "CaracatSender::new() timed out after 5 seconds"
                                )),
                            }
                        });

                        match caracat_sender_result {
                            Ok(sender) => {
                                trace!(
                                    "SendLoop successfully created CaracatSender for key: {}",
                                    sender_key
                                );
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
                                trace!("SendLoop failed to create CaracatSender for key: {}, error: {}", sender_key, e);
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

                // Report measurement status if we have measurement info
                if let Some(ref measurement_info) = measurement_info {
                    *probes_sent_in_measurement
                        .entry(measurement_info.measurement_id.clone())
                        .or_insert(0) += sent_count_batch as u32;

                    // Report status to gateway if configured
                    if let (Some(ref gateway_url), Some(ref agent_key)) = (&gateway_url, &agent_key)
                    {
                        let total_sent = *probes_sent_in_measurement
                            .get(&measurement_info.measurement_id)
                            .unwrap_or(&0);

                        // Use runtime handle to run async code in this thread
                        match thread_runtime_handle.block_on(
                            crate::agent::gateway::report_measurement_status(
                                gateway_url.as_str(),
                                &agent_id,
                                agent_key.as_str(),
                                &measurement_info.measurement_id,
                                total_sent,
                                measurement_info.end_of_measurement,
                            ),
                        ) {
                            Ok(_) => tracing::debug!(
                                "Reported measurement status for {}: {} probes sent, completed: {}",
                                measurement_info.measurement_id,
                                total_sent,
                                measurement_info.end_of_measurement
                            ),
                            Err(e) => tracing::warn!("Failed to report measurement status: {}", e),
                        }

                        // Clean up tracking for completed measurements
                        if measurement_info.end_of_measurement {
                            probes_sent_in_measurement.remove(&measurement_info.measurement_id);
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
