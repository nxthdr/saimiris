use anyhow::Result;
use caracat::models::Reply;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Headers;
use rdkafka::Message;
use std::collections::HashMap;
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::spawn;
use tracing::{debug, error, info, trace, warn};

use crate::agent::consumer::init_consumer;
use crate::agent::gateway::spawn_healthcheck_loop;
use crate::agent::producer;
use crate::agent::receiver::ReceiveLoop;
use crate::agent::sender::{ProbesWithSource, SendLoop};
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::{AppConfig, CaracatConfig};
use crate::probe::deserialize_probes;

pub fn determine_target_sender(
    probe_senders_map: &HashMap<String, Sender<ProbesWithSource>>,
    caracat_configs: &[CaracatConfig],
    sender_ip_from_header: Option<&String>,
) -> Result<(Option<Sender<ProbesWithSource>>, bool)> {
    // First, try to find a config with prefixes that matches the source IP (if provided)
    if let Some(ip_addr_str) = sender_ip_from_header {
        for caracat_cfg in caracat_configs {
            let has_prefix =
                caracat_cfg.src_ipv4_prefix.is_some() || caracat_cfg.src_ipv6_prefix.is_some();

            if has_prefix {
                let validation_result = crate::config::validate_ip_against_prefixes(
                    ip_addr_str,
                    &caracat_cfg.src_ipv4_prefix,
                    &caracat_cfg.src_ipv6_prefix,
                );

                if validation_result.is_ok() {
                    // Find the corresponding sender for this instance
                    let instance_key = format!("instance_{}", caracat_cfg.instance_id);
                    if let Some(sender) = probe_senders_map.get(&instance_key) {
                        debug!(
                            "Source IP {} matches prefix configuration for instance {}, using corresponding sender",
                            ip_addr_str, caracat_cfg.instance_id
                        );
                        return Ok((Some(sender.clone()), true)); // true = use source IP from header
                    }
                }
            }
        }
    }

    // If no prefix-based match found, look for a default config (no prefixes)
    for caracat_cfg in caracat_configs {
        let has_prefix =
            caracat_cfg.src_ipv4_prefix.is_some() || caracat_cfg.src_ipv6_prefix.is_some();

        if !has_prefix {
            // No prefixes configured, use default behavior
            let instance_key = format!("instance_{}", caracat_cfg.instance_id);
            if let Some(sender) = probe_senders_map.get(&instance_key) {
                debug!(
                    "Using default sender for instance {} (no prefixes configured)",
                    caracat_cfg.instance_id
                );
                return Ok((Some(sender.clone()), false)); // false = don't use source IP from header
            }
        }
    }

    // If we get here, either:
    // 1. Source IP was provided but doesn't match any configured prefix, OR
    // 2. No source IP was provided and no default config exists
    if let Some(ip_addr_str) = sender_ip_from_header {
        Err(anyhow::anyhow!(
            "Source IP address {} is not within any configured prefix for this agent",
            ip_addr_str
        ))
    } else {
        Err(anyhow::anyhow!(
            "No source IP address provided and no default configuration (without prefixes) available"
        ))
    }
}

pub async fn handle(config: &AppConfig) -> Result<()> {
    trace!("Agent handler");
    info!("Agent ID: {}", config.agent.id);

    // --- Gateway registration and health reporting ---
    if let Some(gateway) = &config.gateway {
        if let (Some(gateway_url), Some(agent_key), Some(agent_secret)) =
            (&gateway.url, &gateway.agent_key, &gateway.agent_secret)
        {
            spawn_healthcheck_loop(
                gateway_url.clone(),
                config.agent.id.clone(),
                agent_key.clone(),
                agent_secret.clone(),
                config.caracat.clone(),
            );
        }
    }

    let current_tokio_handle = TokioHandle::current();

    if config.caracat.is_empty() {
        anyhow::bail!(
            "No Caracat configurations found. At least one Caracat instance must be configured."
        );
    }

    info!(
        "Found {} Caracat configurations. Initializing SendLoops and ReceiveLoops.",
        config.caracat.len()
    );

    // Channel for all replies from all ReceiveLoops to the single Kafka producer
    let (tx_async_reply_to_producer, rx_async_reply_for_producer): (
        Sender<Reply>,
        Receiver<Reply>,
    ) = channel(100000);

    let mut probe_senders_map: HashMap<String, Sender<ProbesWithSource>> = HashMap::new();
    let mut default_probe_sender_channel: Option<Sender<ProbesWithSource>> = None;

    // --- Setup SendLoops (one per CaracatConfig) ---
    for caracat_cfg in &config.caracat {
        debug!(
                "Initializing SendLoop for Caracat instance: interface: {}, src_ipv4_prefix: {:?}, src_ipv6_prefix: {:?}, instance_id: {}",
                caracat_cfg.interface, caracat_cfg.src_ipv4_prefix, caracat_cfg.src_ipv6_prefix, caracat_cfg.instance_id
            );

        let (tx_probe_to_sender, rx_probes_for_sender): (
            Sender<ProbesWithSource>,
            Receiver<ProbesWithSource>,
        ) = channel(100); // Probes for this specific SendLoop

        if default_probe_sender_channel.is_none() {
            default_probe_sender_channel = Some(tx_probe_to_sender.clone());
            debug!(
                "Set default sender channel to the one for interface: {} (Instance ID: {})",
                caracat_cfg.interface, caracat_cfg.instance_id
            );
        }

        // Register this caracat instance with its prefix configuration
        // We no longer map by specific IP but by instance ID
        let has_prefix =
            caracat_cfg.src_ipv4_prefix.is_some() || caracat_cfg.src_ipv6_prefix.is_some();

        let instance_key = format!("instance_{}", caracat_cfg.instance_id);
        if probe_senders_map.contains_key(&instance_key) {
            warn!("Duplicate Caracat configuration for instance ID: {}. Only the first one will be used.", caracat_cfg.instance_id);
        } else {
            probe_senders_map.insert(instance_key.clone(), tx_probe_to_sender.clone());
            if has_prefix {
                debug!(
                    "Caracat sender registered for instance ID: {} with prefixes IPv4: {:?}, IPv6: {:?}",
                    caracat_cfg.instance_id, caracat_cfg.src_ipv4_prefix, caracat_cfg.src_ipv6_prefix
                );
            } else {
                debug!(
                    "Caracat sender registered for instance ID: {} without prefixes (will use default source IP behavior)",
                    caracat_cfg.instance_id
                );
            }
        }

        let _send_loop = SendLoop::new(
            rx_probes_for_sender,
            caracat_cfg.clone(),
            config,
            current_tokio_handle.clone(),
        );
        debug!(
            "Caracat SendLoop instance started for interface {} (Instance ID: {})",
            caracat_cfg.interface, caracat_cfg.instance_id
        );
    }

    // --- Setup ReceiveLoops (one per unique physical interface) ---
    let mut unique_interfaces: HashMap<String, Vec<CaracatConfig>> = HashMap::new();
    for caracat_cfg in &config.caracat {
        unique_interfaces
            .entry(caracat_cfg.interface.clone())
            .or_default()
            .push(caracat_cfg.clone());
    }

    info!(
        "Found {} unique physical interfaces for Caracat configurations.",
        unique_interfaces.len()
    );

    for (interface_name, configs_for_interface) in unique_interfaces {
        if configs_for_interface.is_empty() {
            continue;
        }
        // All configs_for_interface share the same interface_name.
        // We need to pass all relevant instance IDs for this physical interface.
        let instance_ids_for_interface: Vec<u16> = configs_for_interface
            .iter()
            .map(|cfg| cfg.instance_id)
            .collect();

        // The ReceiveLoop will use the first config for basic settings like integrity_check,
        // but it needs all instance_ids for demultiplexing.
        // Or, you might define a "shared" config for the receiver if some params differ.
        // For simplicity, let's assume the first config's integrity_check flag is representative.
        let representative_cfg = configs_for_interface[0].clone(); // Used for general receiver settings

        info!(
            "Initializing ReceiveLoop for physical interface: {} (Associated Instance IDs: {:?})",
            interface_name, instance_ids_for_interface
        );

        let _receive_loop = ReceiveLoop::new(
            tx_async_reply_to_producer.clone(), // All receivers send to the same producer channel
            config.agent.id.clone(),
            representative_cfg,         // Use the first config for basic settings
            instance_ids_for_interface, // Pass all valid instance IDs for this interface
            current_tokio_handle.clone(),
        );
        debug!(
            "Caracat ReceiveLoop started for physical interface {}",
            interface_name
        );
    }

    // -- Configure Kafka producer and consumer --
    let kafka_auth = match config.kafka.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: config.kafka.auth_sasl_username.clone(),
            password: config.kafka.auth_sasl_password.clone(),
            mechanism: config.kafka.auth_sasl_mechanism.clone(),
        }),
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid Kafka producer authentication protocol"
            ))
        }
    };

    if config.kafka.out_enable {
        info!("Kafka producer enabled. Spawning async producer task.");
        let producer_config = config.clone();
        let producer_auth_clone = kafka_auth.clone();
        spawn(async move {
            producer::produce(
                &producer_config,
                producer_auth_clone,
                rx_async_reply_for_producer, // Single receiver for all replies
            )
            .await
        });
        debug!("Async Kafka producer task spawned.");
    } else {
        info!("Kafka producer disabled. Caracat replies will be ignored.");
        drop(rx_async_reply_for_producer);
        drop(tx_async_reply_to_producer);
    }

    let consumer: StreamConsumer<rdkafka::consumer::DefaultConsumerContext> =
        init_consumer(config, kafka_auth).await;
    info!(
        "Kafka consumer initialized. Listening for probes on topics: {}",
        config.kafka.in_topics
    );

    // -- Start the main loop --
    loop {
        let message = match consumer.recv().await {
            Ok(m) => m,
            Err(e) => {
                error!("Kafka consumer error: {}. Retrying in 5s...", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let payload_bytes = match message.payload() {
            Some(bytes) => bytes,
            None => {
                warn!("Received message with empty payload. Ignored.");
                if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
                    error!("Failed to commit empty message: {}", e);
                }
                continue;
            }
        };
        debug!(
            "Kafka message received, payload size: {}",
            payload_bytes.len()
        );

        let mut is_intended_for_this_agent = false;
        let mut sender_ip_from_header: Option<String> = None;
        let mut measurement_info: Option<crate::agent::gateway::MeasurementInfo> = None;

        if let Some(headers) = message.headers() {
            debug!("Message has {} headers", headers.count());
            for header in headers.iter() {
                debug!(
                    "Header: key='{}', value_len={}",
                    header.key,
                    header.value.map(|v| v.len()).unwrap_or(0)
                );
                if header.key == config.agent.id {
                    debug!("Found header for agent ID: {}", config.agent.id);
                    is_intended_for_this_agent = true;
                    if let Some(value_bytes) = header.value {
                        // Parse the JSON header value to extract measurement info
                        if let Ok(header_str) = String::from_utf8(value_bytes.to_vec()) {
                            if let Ok(agent_info) =
                                serde_json::from_str::<serde_json::Value>(&header_str)
                            {
                                // Extract src_ip from the JSON
                                sender_ip_from_header = agent_info
                                    .get("src_ip")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                debug!("Extracted src_ip: {:?}", sender_ip_from_header);

                                // Extract measurement tracking information
                                if let (Some(measurement_id), Some(end_of_measurement)) = (
                                    agent_info.get("measurement_id").and_then(|v| v.as_str()),
                                    agent_info
                                        .get("end_of_measurement")
                                        .and_then(|v| v.as_bool()),
                                ) {
                                    measurement_info =
                                        Some(crate::agent::gateway::MeasurementInfo {
                                            measurement_id: measurement_id.to_string(),
                                            end_of_measurement,
                                        });
                                    debug!(
                                        "Extracted measurement info: measurement_id={}, end_of_measurement={}",
                                        measurement_id, end_of_measurement
                                    );
                                }
                            }
                        }
                    }
                }
            }
        } else {
            debug!("Message has no headers");
        }

        if !is_intended_for_this_agent && !config.caracat.is_empty() {
            debug!(
                "Message not intended for this agent (ID: {}). Ignored.",
                config.agent.id
            );
            if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
                warn!("Failed to commit ignored message (not intended): {}", e);
            }
            continue;
        }

        info!("Message intended for this agent. Processing probes.");

        let probes_to_send = match deserialize_probes(payload_bytes.to_vec()) {
            Ok(probes) if probes.is_empty() => {
                debug!("No probes to send after deserialization (empty list). Ignored.");
                if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
                    warn!("Failed to commit ignored message (empty probes): {}", e);
                }
                continue;
            }
            Ok(probes) => {
                trace!("{} probes deserialized successfully.", probes.len());
                probes
            }
            Err(e) => {
                error!(
                    "Failed to deserialize probes from Kafka message: {:?}. Message ignored.",
                    e
                );
                if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
                    warn!(
                        "Failed to commit ignored message (deserialization error): {}",
                        e
                    );
                }
                continue;
            }
        };

        let target_sender_result = determine_target_sender(
            &probe_senders_map,
            &config.caracat,
            sender_ip_from_header.as_ref(),
        );

        match target_sender_result {
            Ok((Some(sender_channel), use_source_ip_flag)) => {
                debug!(
                    "Distributing {} probes to selected Caracat sender.",
                    probes_to_send.len()
                );

                let probes_count = probes_to_send.len();
                // Create ProbesWithSource, use source IP from header only if use_source_ip_flag is true
                let probes_with_source = if use_source_ip_flag {
                    ProbesWithSource {
                        probes: probes_to_send,
                        source_ip: sender_ip_from_header.unwrap().clone(),
                        measurement_info: measurement_info.clone(),
                    }
                } else {
                    // Use empty string to indicate no specific source IP (default behavior)
                    ProbesWithSource {
                        probes: probes_to_send,
                        source_ip: String::new(),
                        measurement_info: measurement_info.clone(),
                    }
                };

                trace!("Attempting to send {} probes to selected sender instance via async channel", probes_count);
                match sender_channel.try_send(probes_with_source) {
                    Ok(()) => {
                        trace!("Probes successfully queued for the selected sender instance via async send.");
                    }
                    Err(send_err) => {
                        error!("Failed to send probes to selected Caracat sender (async channel error): {}. SendLoop may have exited.", send_err);
                    }
                }
            }
            Ok((None, _)) => {
                error!("No suitable sender found for the provided source IP");
            }
            Err(e) => {
                error!(
                    "Failed to validate source IP against configured prefixes: {}",
                    e
                );
                if !probes_to_send.is_empty() {
                    warn!(
                        "Probes not sent due to validation error (source IP: {:?}): {}",
                        sender_ip_from_header, e
                    );
                }
            }
        }

        if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
            error!("Failed to commit processed message: {}", e);
        }
    }
}
