use log::info;
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

use crate::auth::KafkaAuth;
use crate::config::AppConfig;

pub async fn produce(config: &AppConfig, auth: KafkaAuth, target: &str) {
    let producer: &FutureProducer = match auth {
        KafkaAuth::PlainText => &ClientConfig::new()
            .set("bootstrap.servers", config.brokers.clone())
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error"),
        KafkaAuth::SasalPlainText(scram_auth) => &ClientConfig::new()
            .set("bootstrap.servers", config.brokers.clone())
            .set("message.timeout.ms", "5000")
            .set("sasl.username", scram_auth.username)
            .set("sasl.password", scram_auth.password)
            .set("sasl.mechanisms", scram_auth.mechanism)
            .set("security.protocol", "SASL_PLAINTEXT")
            .create()
            .expect("Producer creation error"),
    };

    let topic = config.in_topics.split(',').collect::<Vec<&str>>()[0];
    info!("Producing to topic: {}", topic);

    let delivery_status = producer
        .send(
            FutureRecord::to(topic)
                .payload(target)
                .key(&format!("Key")) // TODO
                .headers(OwnedHeaders::new().insert(Header {
                    // TODO
                    key: "header_key",
                    value: Some("header_value"),
                })),
            Duration::from_secs(0),
        )
        .await;

    info!("{:?}", delivery_status);
}
