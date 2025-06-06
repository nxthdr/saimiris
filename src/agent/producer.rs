use caracat::models::Reply;
use metrics::counter;
use rdkafka::config::ClientConfig;
use rdkafka::message::OwnedHeaders;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, error, warn};

use crate::auth::KafkaAuth;
use crate::config::AppConfig;
use crate::reply::serialize_reply;

pub async fn produce(config: &AppConfig, auth: KafkaAuth, mut rx: Receiver<Reply>) {
    if config.kafka.out_enable == false {
        warn!("Kafka producer is disabled");
        loop {
            rx.recv()
                .await
                .expect("Failed to receive message from Kafka producer channel");
        }
    }

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

    let mut additional_message = None;
    loop {
        let start_time = std::time::Instant::now();
        let mut final_message = Vec::new();
        let mut n_messages = 0;

        // Send the additional reply first
        if let Some(message) = additional_message {
            let message = serialize_reply(config.agent.id.clone(), &message);
            final_message.extend_from_slice(&message);
            n_messages += 1;
            additional_message = None;
        }

        loop {
            if std::time::Instant::now().duration_since(start_time)
                > std::time::Duration::from_millis(config.kafka.out_batch_wait_time)
            {
                break;
            }

            let message = rx.try_recv();
            if message.is_err() {
                tokio::time::sleep(Duration::from_millis(config.kafka.out_batch_wait_interval))
                    .await;
                continue;
            }

            let message = message.unwrap();
            let message_bin = serialize_reply(config.agent.id.clone(), &message);

            // Max message size is 1048576 bytes (including headers)
            if final_message.len() + message_bin.len() > config.kafka.message_max_bytes {
                additional_message = Some(message);
                break;
            }

            final_message.extend_from_slice(&message_bin);
            n_messages += 1;
        }

        if final_message.is_empty() {
            continue;
        }

        debug!("Sending {} replies to Kafka", n_messages);
        let delivery_status = producer
            .send(
                FutureRecord::to(config.kafka.out_topic.as_str())
                    .payload(&final_message)
                    .key(&format!("")) // TODO
                    .headers(OwnedHeaders::new()), // TODO
                Duration::from_secs(0),
            )
            .await;

        let metric_name = "saimiris_kafka_messages_total";
        match delivery_status {
            Ok((partition, offset)) => {
                counter!(metric_name, "agent" => config.agent.id.clone(), "status" => "success")
                    .increment(1);
                debug!(
                    "successfully sent message to partition {} at offset {}",
                    partition, offset
                );
            }
            Err((error, _)) => {
                counter!(metric_name, "agent" => config.agent.id.clone(), "status" => "failure")
                    .increment(1);
                error!("failed to send message: {}", error);
            }
        }
    }
}
