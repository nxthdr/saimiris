use anyhow::Result;
use caracat::models::{Probe, Reply};
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
use crate::agent::sender::SendLoop;
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::{AppConfig, CaracatConfig};
use crate::probe::deserialize_probes;

pub fn determine_target_sender(
    probe_senders_map: &HashMap<String, Sender<Vec<Probe>>>,
    sender_ip_from_header: Option<&String>,
    default_sender_channel: Option<&Sender<Vec<Probe>>>,
) -> Option<Sender<Vec<Probe>>> {
    if let Some(ip_addr_str) = sender_ip_from_header {
        if let Some(sender) = probe_senders_map.get(ip_addr_str) {
            debug!("Found target sender for IP: {}", ip_addr_str);
            return Some(sender.clone());
        } else {
            warn!(
                "No Caracat sender configured for IP address: {}. Probes will not be sent.",
                ip_addr_str
            );
            return None;
        }
    } else {
        if let Some(default_sender) = default_sender_channel {
            debug!("No specific sender IP address found in headers. Using default sender (first configured Caracat instance).");
            return Some(default_sender.clone());
        } else {
            warn!("No specific sender IP address found in headers, and no default sender (first CaracatConfig) is available. Probes will not be sent.");
            return None;
        }
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

    let mut probe_senders_map: HashMap<String, Sender<Vec<Probe>>> = HashMap::new();
    let mut default_probe_sender_channel: Option<Sender<Vec<Probe>>> = None;

    // --- Setup SendLoops (one per CaracatConfig) ---
    for caracat_cfg in &config.caracat {
        debug!(
            "Initializing SendLoop for Caracat instance: interface: {}, src_ipv4: {:?}, src_ipv6: {:?}, instance_id: {}",
            caracat_cfg.interface, caracat_cfg.src_ipv4_addr, caracat_cfg.src_ipv6_addr, caracat_cfg.instance_id
        );

        let (tx_probe_to_sender, rx_probes_for_sender): (Sender<Vec<Probe>>, Receiver<Vec<Probe>>) =
            channel(100000); // Probes for this specific SendLoop

        if default_probe_sender_channel.is_none() {
            default_probe_sender_channel = Some(tx_probe_to_sender.clone());
            debug!(
                "Set default sender channel to the one for interface: {} (Instance ID: {})",
                caracat_cfg.interface, caracat_cfg.instance_id
            );
        }

        let sender_ip_key: Option<String> = caracat_cfg
            .src_ipv4_addr
            .map(|ip| ip.to_string())
            .or_else(|| caracat_cfg.src_ipv6_addr.map(|ip| ip.to_string()));

        if let Some(ip_key) = sender_ip_key {
            if probe_senders_map.contains_key(&ip_key) {
                warn!("Duplicate Caracat configuration for source IP: {}. Only the first one will be targetable by this IP via header. (Associated with instance_id: {})", ip_key, caracat_cfg.instance_id);
            } else {
                probe_senders_map.insert(ip_key.clone(), tx_probe_to_sender.clone());
                debug!(
                    "Caracat sender registered for IP-specific targeting: {} (Instance ID: {})",
                    ip_key, caracat_cfg.instance_id
                );
            }
        } else {
            debug!("Caracat configuration for interface {} (Instance ID: {}) has no source IP. It can be the default sender if it's the first config.", caracat_cfg.interface, caracat_cfg.instance_id);
        }

        let _send_loop = SendLoop::new(
            rx_probes_for_sender,
            config.agent.id.clone(),
            caracat_cfg.clone(),
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
            representative_cfg, // Use the first config for basic settings
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
                        // Parse the JSON header value to extract src_ip
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

        let target_sender_channel = determine_target_sender(
            &probe_senders_map,
            sender_ip_from_header.as_ref(),
            default_probe_sender_channel.as_ref(),
        );

        if let Some(sender_channel) = target_sender_channel {
            debug!(
                "Distributing {} probes to selected Caracat sender.",
                probes_to_send.len()
            );

            let send_task_join_handle =
                spawn(async move { sender_channel.send(probes_to_send).await });

            match send_task_join_handle.await {
                Ok(Ok(())) => {
                    trace!("Probes successfully queued for the selected sender instance via async send.");
                }
                Ok(Err(send_err)) => {
                    error!("Failed to send probes to selected Caracat sender (async channel error): {}. SendLoop may have exited.", send_err);
                }
                Err(join_err) => {
                    error!(
                        "Async task for sending probes to selected sender panicked: {}",
                        join_err
                    );
                }
            }
        } else {
            if !probes_to_send.is_empty() {
                warn!(
                    "Probes not sent as no target Caracat sender could be determined (X-Sender-IP: {:?}).",
                    sender_ip_from_header
                );
            }
        }

        if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
            error!("Failed to commit processed message: {}", e);
        }
    }
}
