use caracat::models::Probe;
use log::{debug, info};
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

use crate::auth::KafkaAuth;
use crate::config::AppConfig;
use crate::probe::encode_probe;

fn create_messages(probes: Vec<Probe>, message_max_bytes: usize) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current_message = String::new();
    for probe in probes {
        // Format probe
        let probe_str = encode_probe(&probe);
        // Max message size is 1048576 bytes
        if current_message.len() + probe_str.len() + 1 > message_max_bytes {
            // Remove the last newline character
            current_message.pop();
            messages.push(current_message);
            current_message = String::new();
        }

        current_message.push_str(&probe_str);
        current_message.push_str("\n");
    }
    if !current_message.is_empty() {
        // Remove the last newline character
        current_message.pop();
        messages.push(current_message);
    }
    messages
}

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
    let probes_len = probes.len();
    let messages = create_messages(probes, config.kafka.message_max_bytes);

    info!(
        "topic={},messages={},probes={}",
        topic,
        messages.len(),
        probes_len,
    );

    // Send to Kafka
    for message in messages {
        debug!("{}", message);
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
