use caracat::models::Probe;
use log::info;
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

use crate::auth::KafkaAuth;
use crate::config::AppConfig;
use crate::probe::encode_probe;

pub async fn produce(config: &AppConfig, auth: KafkaAuth, agents: Vec<&str>, probes: Vec<Probe>) {
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
    for agent in agents {
        headers = headers.insert(Header {
            key: agent,
            value: Some(agent),
        });
    }

    // Place probes into Kafka messages
    let mut messages = Vec::new();
    let mut current_message = String::new();
    for (i, probe) in probes.iter().enumerate() {
        // Format probe
        let probe_str = encode_probe(probe);
        // Max message size is 1048576 bytes
        if current_message.len() + probe_str.len() + 1 > 1048576 {
            messages.push(current_message);
            current_message = String::new();
        }
        current_message.push_str(&probe_str);
        if i < probes.len() - 1 {
            current_message.push_str("\n");
        }
    }
    if !current_message.is_empty() {
        messages.push(current_message);
    }

    info!(
        "topic={},messages={},probes={}",
        topic,
        messages.len(),
        probes.len(),
    );

    // Send to Kafka
    for message in messages {
        let delivery_status = producer
            .send(
                FutureRecord::to(topic)
                    .payload(&format!("{}", message))
                    .key(&format!("")) // TODO Client ID
                    .headers(headers.clone()),
                Duration::from_secs(0),
            )
            .await;

        info!("{:?}", delivery_status);
    }
}
