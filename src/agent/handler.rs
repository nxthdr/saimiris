use anyhow::Result;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Headers;
use rdkafka::Message;
use std::sync::mpsc::channel;
use tokio::task;
use tracing::{debug, info, trace};

use crate::agent::consumer::init_consumer;
use crate::agent::producer::produce;
use crate::agent::receiver::ReceiveLoop;
use crate::agent::sender::SendLoop;
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::AppConfig;
use crate::probe::decode_probe;
use crate::utils::test_id;

pub async fn handle(config: &AppConfig) -> Result<()> {
    trace!("Agent handler");
    trace!("{:?}", config);

    // Test input ID
    if !test_id(Some(config.agent.id.clone()), None, None) {
        return Err(anyhow::anyhow!("Invalid agent ID"));
    }

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
                info!("Kafka error: {}", e);
            }
            Ok(m) => {
                let probes = match m.payload_view::<str>() {
                    None => "",
                    Some(Ok(s)) => s,
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!(
                            "Kafka error: Error while deserializing message payload: {:?}",
                            e
                        ))
                    }
                };

                debug!("Received message");
                if let Some(headers) = m.headers() {
                    for header in headers.iter() {
                        debug!("Header {:#?}: {:?}", header.key, header.value);
                    }
                }

                // Filter out the messages that are not intended for the agent
                // by looking at the agent ID in the headers
                let mut is_intended = false;
                if let Some(headers) = m.headers() {
                    for header in headers.iter() {
                        if header.key == &config.agent.id {
                            is_intended = true;
                            break;
                        }
                    }
                }
                if !is_intended {
                    info!("Target not intended for this agent");

                    // TODO: handle errors
                    consumer.commit_message(&m, CommitMode::Async).unwrap();
                    continue;
                }

                // Decode probes
                let probes_to_send = probes
                    .split('\n')
                    .map(|probe| decode_probe(probe))
                    .collect::<Result<Vec<_>>>()?;

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
