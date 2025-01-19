use log::info;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{Consumer, DefaultConsumerContext};

use crate::auth::KafkaAuth;
use crate::config::AppConfig;

pub async fn init_consumer(config: &AppConfig, auth: KafkaAuth) -> StreamConsumer {
    let context = DefaultConsumerContext;
    info!("Brokers: {}", config.brokers);
    info!("Group ID: {}", config.in_group_id);
    let consumer: StreamConsumer<DefaultConsumerContext> = match auth {
        KafkaAuth::PlainText => ClientConfig::new()
            .set("bootstrap.servers", config.brokers.clone())
            .set("group.id", config.in_group_id.clone())
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create_with_context(context.clone())
            .expect("Consumer creation error"),
        KafkaAuth::SasalPlainText(scram_auth) => ClientConfig::new()
            .set("bootstrap.servers", config.brokers.clone())
            .set("group.id", config.in_group_id.clone())
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("sasl.username", scram_auth.username)
            .set("sasl.password", scram_auth.password)
            .set("sasl.mechanisms", scram_auth.mechanism)
            .set("security.protocol", "SASL_PLAINTEXT")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create_with_context(context)
            .expect("Consumer creation error"),
    };

    let topics: Vec<&str> = config.in_topics.split(',').collect();
    info!("Subscribing to topics: {:?}", topics);
    consumer
        .subscribe(&topics)
        .expect("Cannot subscribe to specified topics");

    consumer
}
