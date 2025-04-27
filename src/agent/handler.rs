use anyhow::Result;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Headers;
use rdkafka::Message;
use std::sync::mpsc::channel;
use tokio::task;
use tracing::{debug, error, info, trace, warn};

use crate::agent::consumer::init_consumer;
use crate::agent::producer::produce;
use crate::agent::receiver::ReceiveLoop;
use crate::agent::sender::SendLoop;
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::AppConfig;
use crate::probe::deserialize_probes;

pub async fn handle(config: &AppConfig) -> Result<()> {
    trace!("Agent handler");
    trace!("{:?}", config);

    info!("Agent ID: {}", config.agent.id);

    // Configure Kafka authentication
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

    // MPSC channel to communicate between Kafka consumer task and caracat sender task
    let (tx_sender_probe, rx_sender_probe) = channel();
    let (tx_sender_feedback, rx_sender_feedback) = channel();

    // MPSC channel to communicate between caracat receiver task and Kafka producer task
    let (tx_reply, rx_reply) = channel();

    debug!("Starting caracat sender");
    SendLoop::new(rx_sender_probe, tx_sender_feedback, config.caracat.clone());

    debug!("Starting caracat receiver");
    ReceiveLoop::new(tx_reply, config.caracat.clone());

    // Start the Kafka producer task if enabled
    let config_task = config.clone();
    let kafka_auth_task = kafka_auth.clone();
    task::spawn(async move {
        produce(&config_task, kafka_auth_task, rx_reply).await;
    });

    let consumer = init_consumer(config, kafka_auth.clone()).await;
    let mut first_loop = true;
    loop {
        match consumer.recv().await {
            Err(e) => {
                error!("Kafka consumer error: {}. Attempting to continue...", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Ok(m) => {
                let payload_bytes = match m.payload() {
                    None => {
                        warn!("Received message with empty payload. Skipping.");
                        if let Err(e) = consumer.commit_message(&m, CommitMode::Async) {
                            error!("Failed to commit empty message: {}", e);
                        }
                        continue;
                    }
                    Some(bytes) => bytes.to_vec(),
                };

                debug!(
                    "Received message with payload size: {}",
                    payload_bytes.len()
                );
                if let Some(headers) = m.headers() {
                    for header in headers.iter() {
                        trace!(
                            "Header {}: {:?}",
                            header.key,
                            String::from_utf8_lossy(header.value.unwrap_or_default())
                        );
                    }
                }

                // Filter out the messages that are not intended for the agent
                // by looking at the agent ID in the headers
                let mut is_intended = false;
                if let Some(headers) = m.headers() {
                    for header in headers.iter() {
                        if header.key == config.agent.id {
                            debug!(
                                "Message intended for this agent (header key match: {})",
                                config.agent.id
                            );
                            is_intended = true;
                            break;
                        }
                    }
                }

                if !is_intended {
                    debug!(
                        "Message not intended for this agent (ID: {}). Skipping.",
                        config.agent.id
                    );
                    if let Err(e) = consumer.commit_message(&m, CommitMode::Async) {
                        warn!("Failed to commit skipped message: {}", e);
                    }
                    continue;
                }

                // Deserialize probes
                let probes_to_send = match deserialize_probes(payload_bytes) {
                    Ok(probe) => {
                        trace!("Successfully deserialized probe: {:?}", probe);
                        probe
                    }
                    Err(e) => {
                        error!("Failed to deserialize probe from Kafka message: {:?}. Skipping message.", e);
                        if let Err(e) = consumer.commit_message(&m, CommitMode::Async) {
                            warn!("Failed to commit skipped message: {}", e);
                        }
                        continue;
                    }
                };

                // Send to the channel
                tx_sender_probe.send(probes_to_send).unwrap();

                if first_loop {
                    first_loop = false;
                } else {
                    // Wait for the sender to finish the measurement
                    loop {
                        let sender_feedback = match rx_sender_feedback.try_recv() {
                            Ok(feedback) => feedback,
                            Err(_) => false,
                        };

                        if sender_feedback {
                            break;
                        }
                    }
                }

                // Commit the consumed message
                // TODO: handle errors
                consumer.commit_message(&m, CommitMode::Async).unwrap();
            }
        };
    }
}
