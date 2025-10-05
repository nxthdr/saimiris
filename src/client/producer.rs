use caracat::models::Probe;
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json;
use std::time::Duration;
use tracing::{error, info};

use crate::auth::KafkaAuth;
use crate::config::AppConfig;
use crate::probe::serialize_probe;

#[derive(Debug, Clone)]
pub struct MeasurementInfo {
    pub name: String,
    pub src_ip: Option<String>,
    // Measurement tracking fields
    pub measurement_id: Option<String>,
}

pub fn create_messages(probes: Vec<Probe>, message_max_bytes: usize) -> Vec<Vec<u8>> {
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
    config: &AppConfig,
    auth: KafkaAuth,
    agents: Vec<MeasurementInfo>,
    probes: Vec<Probe>,
) {
    let producer: &FutureProducer = match auth {
        KafkaAuth::PlainText => &ClientConfig::new()
            .set("bootstrap.servers", config.kafka.brokers.clone())
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error"),
        KafkaAuth::SasalPlainText(scram_auth) => &ClientConfig::new()
            .set("bootstrap.servers", config.kafka.brokers.clone())
            .set("message.timeout.ms", "5000")
            .set("sasl.username", scram_auth.username)
            .set("sasl.password", scram_auth.password)
            .set("sasl.mechanisms", scram_auth.mechanism)
            .set("security.protocol", "SASL_PLAINTEXT")
            .create()
            .expect("Producer creation error"),
    };

    let topic = config.kafka.in_topics.split(',').collect::<Vec<&str>>()[0];

    // Construct headers
    let mut headers = OwnedHeaders::new();

    // Add agent-specific headers
    for agent in &agents {
        // Serialize all agent info into a single header value
        let agent_info_json = serde_json::json!({
            "src_ip": agent.src_ip,
        });
        let agent_info_str = agent_info_json.to_string();

        headers = headers.insert(Header {
            key: &agent.name,
            value: Some(&agent_info_str),
        });
    }

    // Add measurement tracking headers if provided
    // Take measurement info from the first agent (assuming all agents share the same measurement)
    if let Some(first_agent) = agents.first() {
        if let Some(ref measurement_id) = first_agent.measurement_id {
            headers = headers.insert(Header {
                key: "measurement_id",
                value: Some(measurement_id),
            });
        }
    }

    // Place probes into Kafka messages
    let probes_len = probes.len();
    let messages = create_messages(probes, config.kafka.message_max_bytes);

    info!(
        "topic={},messages={},probes={}",
        topic,
        messages.len(),
        probes_len,
    );

    // Send to Kafka
    for (message_index, message) in messages.iter().enumerate() {
        let is_last_message = message_index == messages.len() - 1;

        // Clone headers and add end_of_measurement for this specific message
        let mut message_headers = headers.clone();
        message_headers = message_headers.insert(Header {
            key: "end_of_measurement",
            value: Some(&is_last_message.to_string()),
        });

        let delivery_status = producer
            .send(
                FutureRecord::to(topic)
                    .payload(message)
                    .key(&format!(""))
                    .headers(message_headers),
                Duration::from_secs(0),
            )
            .await;

        match delivery_status {
            Ok(delivery) => {
                info!(
                    "successfully sent message to partition {} at offset {}",
                    delivery.partition, delivery.offset
                );
            }
            Err((error, _)) => {
                error!("failed to send message: {}", error);
            }
        }
    }
}
