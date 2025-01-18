use caracat::models::{MPLSLabel, Reply};
use log::info;
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct SaslAuth {
    pub username: String,
    pub password: String,
    pub mechanism: String,
}

pub enum KafkaAuth {
    SASL(SaslAuth),
    PLAINTEXT,
}

fn format_mpls_labels(mpls_labels: &Vec<MPLSLabel>) -> String {
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

fn format_reply(prober_id: u16, reply: &Reply) -> String {
    let mut output = vec![];
    output.push(format!("{}", reply.capture_timestamp.as_millis()));
    output.push(format!("{}", prober_id));
    output.push(format!("{}", reply.reply_src_addr));
    output.push(format!("{}", reply.reply_dst_addr));
    output.push(format!("{}", reply.reply_id));
    output.push(format!("{}", reply.reply_size));
    output.push(format!("{}", reply.reply_ttl));
    output.push(format!("{}", reply.reply_protocol));
    output.push(format!("{}", reply.reply_icmp_type));
    output.push(format!("{}", reply.reply_icmp_code));
    output.push(format!("{}", format_mpls_labels(&reply.reply_mpls_labels)));
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

pub async fn produce(
    brokers: &str,
    topic_name: &str,
    prober_id: u16,
    auth: KafkaAuth,
    results: Arc<Mutex<Vec<Reply>>>,
) {
    let producer: &FutureProducer = match auth {
        KafkaAuth::PLAINTEXT => &ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error"),
        KafkaAuth::SASL(scram_auth) => &ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("sasl.username", scram_auth.username)
            .set("sasl.password", scram_auth.password)
            .set("sasl.mechanisms", scram_auth.mechanism)
            .set("security.protocol", "SASL_PLAINTEXT")
            .create()
            .expect("Producer creation error"),
    };

    for result in results.lock().unwrap().iter() {
        let delivery_status = producer
            .send(
                FutureRecord::to(topic_name)
                    .payload(&format!("{}", format_reply(prober_id, result)))
                    .key(&format!("Key"))
                    .headers(OwnedHeaders::new().insert(Header {
                        key: "header_key",
                        value: Some("header_value"),
                    })),
                Duration::from_secs(0),
            )
            .await;
        info!("{:?}", delivery_status);
    }
}
