use caracat::models::{MPLSLabel, Reply};
use log::{debug, info, warn};
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::auth::KafkaAuth;
use crate::config::AppConfig;

fn encode_mpls_labels(mpls_labels: &Vec<MPLSLabel>) -> String {
    String::from("[")
        + &mpls_labels
            .iter()
            .map(|label| {
                format!(
                    "({}, {}, {}, {})",
                    label.label, label.experimental, label.bottom_of_stack, label.ttl
                )
            })
            .collect::<Vec<String>>()
            .join(", ")
        + "]"
}

fn encode_reply(agent_id: String, reply: &Reply) -> String {
    let mut output = vec![];
    output.push(format!("{}", reply.capture_timestamp.as_millis()));
    output.push(format!("{}", agent_id));
    output.push(format!("{}", reply.reply_src_addr));
    output.push(format!("{}", reply.reply_dst_addr));
    output.push(format!("{}", reply.reply_id));
    output.push(format!("{}", reply.reply_size));
    output.push(format!("{}", reply.reply_ttl));
    output.push(format!("{}", reply.reply_protocol));
    output.push(format!("{}", reply.reply_icmp_type));
    output.push(format!("{}", reply.reply_icmp_code));
    output.push(format!("{}", encode_mpls_labels(&reply.reply_mpls_labels)));
    output.push(format!("{}", reply.probe_src_addr));
    output.push(format!("{}", reply.probe_dst_addr));
    output.push(format!("{}", reply.probe_id));
    output.push(format!("{}", reply.probe_size));
    output.push(format!("{}", reply.probe_protocol));
    output.push(format!("{}", reply.quoted_ttl));
    output.push(format!("{}", reply.probe_src_port));
    output.push(format!("{}", reply.probe_dst_port));
    output.push(format!("{}", reply.probe_ttl));
    output.push(format!("{}", reply.rtt));

    output.join(",")
}

pub async fn produce(config: &AppConfig, auth: KafkaAuth, rx: Receiver<Reply>) {
    if config.kafka.out_enable == false {
        warn!("Kafka producer is disabled");
        loop {
            rx.recv().unwrap();
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

    // Send to Kafka
    let mut additional_reply = None;
    loop {
        let start_time = std::time::Instant::now();
        let mut message = String::new();
        let mut n_replies = 0;

        // Send the additional reply first
        if let Some(reply) = additional_reply {
            let reply_str = encode_reply(config.agent.id.clone(), &reply);
            message.push_str(&reply_str);
            message.push_str("\n");
            n_replies += 1;
            additional_reply = None;
        }

        loop {
            let reply = rx.recv().unwrap();
            let reply_str = encode_reply(config.agent.id.clone(), &reply);

            if message.len() + reply_str.len() + 1 > config.kafka.message_max_bytes {
                additional_reply = Some(reply);
                break;
            }

            message.push_str(&reply_str);
            message.push_str("\n");
            n_replies += 1;

            let now = std::time::Instant::now();
            if now.duration_since(start_time)
                > std::time::Duration::from_millis(config.kafka.out_max_wait_time)
            {
                // TODO: Config the duration
                break;
            }
        }

        // Remove the last newline character
        message.pop();

        debug!("{}", message);
        info!("Sending {} replies to Kafka", n_replies);

        let delivery_status = producer
            .send(
                FutureRecord::to(config.kafka.out_topic.as_str())
                    .payload(&format!("{}", message))
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
}
