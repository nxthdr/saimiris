use anyhow::Result;
use caracat::models::Probe;
use rdkafka::config::ClientConfig as KafkaClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;
use tracing::{error, info};

use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::{AppConfig, ClientConfig};
use crate::probe::serialize_probe;

fn create_messages(probes: Vec<Probe>, message_max_bytes: usize) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let mut current_message = Vec::new();
    for probe in probes {
        // Serialize the probe
        let message_bin = serialize_probe(&probe);

        // Max message size is 1048576 bytes (including headers)
        if current_message.len() + message_bin.len() > message_max_bytes {
            messages.push(current_message);
            current_message = Vec::new();
        }

        current_message.extend_from_slice(&message_bin);
    }
    if !current_message.is_empty() {
        messages.push(current_message);
    }

    messages
}

pub async fn produce(
    app_config: &AppConfig,
    client_config: &ClientConfig,
    probes: Vec<Probe>,
) -> Result<()> {
    // Configure Kafka authentication
    let auth = match app_config.kafka.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: app_config.kafka.auth_sasl_username.clone(),
            password: app_config.kafka.auth_sasl_password.clone(),
            mechanism: app_config.kafka.auth_sasl_mechanism.clone(),
        }),
        _ => {
            anyhow::bail!("Invalid Kafka producer authentication protocol")
        }
    };

    let producer: &FutureProducer = match auth {
        KafkaAuth::PlainText => &KafkaClientConfig::new()
            .set("bootstrap.servers", app_config.kafka.brokers.clone())
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error"),
        KafkaAuth::SasalPlainText(scram_auth) => &KafkaClientConfig::new()
            .set("bootstrap.servers", app_config.kafka.brokers.clone())
            .set("message.timeout.ms", "5000")
            .set("sasl.username", scram_auth.username)
            .set("sasl.password", scram_auth.password)
            .set("sasl.mechanisms", scram_auth.mechanism)
            .set("security.protocol", "SASL_PLAINTEXT")
            .create()
            .expect("Producer creation error"),
    };

    let topic = app_config.kafka.in_topics.split(',').collect::<Vec<&str>>()[0];

    // Construct headers
    let mut headers = OwnedHeaders::new();
    for agent in client_config.agents.clone() {
        headers = headers.insert(Header {
            key: &agent,
            value: Some(client_config.rate.unwrap_or(0).to_string().as_bytes()),
        });
    }

    // Place probes into Kafka messages
    let probes_len = probes.len();
    let messages = create_messages(probes, app_config.kafka.message_max_bytes);

    info!(
        "topic={},messages={},probes={}",
        topic,
        messages.len(),
        probes_len,
    );

    // Send to Kafka
    for message in messages {
        let delivery_status = producer
            .send(
                FutureRecord::to(topic)
                    .payload(&message)
                    .key(&format!(""))
                    .headers(headers.clone()),
                Duration::from_secs(0),
            )
            .await;

        match delivery_status {
            Ok((partition, offset)) => {
                info!(
                    "successfully sent message to partition {} at offset {}",
                    partition, offset
                );
            }
            Err((error, _)) => {
                error!("failed to send message: {}", error);
            }
        }
    }

    Ok(())
}
