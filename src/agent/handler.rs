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
use crate::agent::producer;
use crate::agent::receiver::ReceiveLoop;
use crate::agent::sender::SendLoop;
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::AppConfig;
use crate::probe::deserialize_probes;

fn determine_target_sender(
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

    // Get the current Tokio runtime handle
    let current_tokio_handle = TokioHandle::current();

    if config.caracat.is_empty() {
        anyhow::bail!(
            "No Caracat configurations found. At least one Caracat instance must be configured."
        );
    }

    // -- Configure Caracat senders and receivers --
    // Create a channel for replies from Caracat receivers to Kafka producer
    let (tx_async_reply_to_producer, rx_async_reply_for_producer): (
        Sender<Reply>,
        Receiver<Reply>,
    ) = channel(100000);

    let mut probe_senders_map: HashMap<String, Sender<Vec<Probe>>> = HashMap::new();
    let mut default_probe_sender_channel: Option<Sender<Vec<Probe>>> = None;
    let mut _feedback_receivers_for_send_loops: Vec<Receiver<bool>> = Vec::new();

    for caracat_cfg in &config.caracat {
        debug!(
            "Initializing Caracat instance for interface: {}, src_ipv4: {:?}, src_ipv6: {:?}",
            caracat_cfg.interface, caracat_cfg.src_ipv4_addr, caracat_cfg.src_ipv6_addr
        );

        let (tx_probe_to_sender, rx_probes_for_sender): (Sender<Vec<Probe>>, Receiver<Vec<Probe>>) =
            channel(100000);
        let (tx_feedback_from_sender, rx_feedback_for_this_sender): (Sender<bool>, Receiver<bool>) =
            channel(1);

        if default_probe_sender_channel.is_none() {
            default_probe_sender_channel = Some(tx_probe_to_sender.clone());
            debug!(
                "Set default sender to the one for interface: {}",
                caracat_cfg.interface
            );
        }

        let sender_ip_key: Option<String> = caracat_cfg
            .src_ipv4_addr
            .map(|ip| ip.to_string())
            .or_else(|| caracat_cfg.src_ipv6_addr.map(|ip| ip.to_string()));

        if let Some(ip_key) = sender_ip_key {
            if probe_senders_map.contains_key(&ip_key) {
                warn!("Duplicate Caracat configuration for source IP: {}. Only the first one will be targetable by this IP via header.", ip_key);
            } else {
                probe_senders_map.insert(ip_key.clone(), tx_probe_to_sender.clone());
                debug!(
                    "Caracat sender registered for IP-specific targeting: {}",
                    ip_key
                );
            }
        } else {
            debug!("Caracat configuration for interface {} has no source IP. It can be the default sender if it's the first config.", caracat_cfg.interface);
        }

        SendLoop::new(
            rx_probes_for_sender,
            tx_feedback_from_sender,
            config.agent.id.clone(),
            caracat_cfg.clone(),
            current_tokio_handle.clone(),
        );
        debug!(
            "Caracat sender instance started for interface {}",
            caracat_cfg.interface
        );

        ReceiveLoop::new(
            tx_async_reply_to_producer.clone(),
            config.agent.id.clone(),
            caracat_cfg.clone(),
            current_tokio_handle.clone(),
        );
        debug!(
            "Caracat receiver started for interface {}",
            caracat_cfg.interface
        );
        _feedback_receivers_for_send_loops.push(rx_feedback_for_this_sender);
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
                rx_async_reply_for_producer,
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
    // This loop will continuously receive messages from Kafka, deserialize probes, and send them to the appropriate Caracat sender.
    // In parallel, the Caracat receivers will send the replies to Kafka producer.
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
            for header in headers.iter() {
                if header.key == config.agent.id {
                    is_intended_for_this_agent = true;
                    sender_ip_from_header = match header.value {
                        Some(value_bytes) => String::from_utf8(value_bytes.to_vec()).ok(),
                        None => None,
                    };
                }
            }
        }

        if !is_intended_for_this_agent {
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
                debug!("Probes not sent as no target sender could be determined.");
            }
        }

        if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
            error!("Failed to commit processed message: {}", e);
        }
    }
}
