use caracat::models::{MPLSLabel, Reply};
use rdkafka::config::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn format_reply(reply: &Reply) -> String {
    let mut output = vec![];
    output.push(format!("{}", reply.capture_timestamp.as_millis()));
    output.push(format!("{}", reply.reply_src_addr));
    output.push(format!("{}", reply.reply_dst_addr));
    output.push(format!("{}", reply.reply_id));
    output.push(format!("{}", reply.reply_size));
    output.push(format!("{}", reply.reply_ttl));
    output.push(format!("{}", reply.reply_protocol));
    output.push(format!("{}", reply.reply_icmp_type));
    output.push(format!("{}", reply.reply_icmp_code));
    output.push(format!("{}", format_mpls_labels(&reply.reply_mpls_labels)));
    // TODO probe_src_addr
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

pub async fn produce(brokers: &str, topic_name: &str, results: Arc<Mutex<Vec<Reply>>>) {
    let producer: &FutureProducer = &ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("message.timeout.ms", "5000")
        .create()
        .expect("Producer creation error");

    for result in results.lock().unwrap().iter() {
        let _ = producer
            .send(
                FutureRecord::to(topic_name)
                    .payload(&format!("{}", format_reply(result)))
                    .key(&format!("Key"))
                    .headers(OwnedHeaders::new().insert(Header {
                        key: "header_key",
                        value: Some("header_value"),
                    })),
                Duration::from_secs(0),
            )
            .await;
    }
    // let delivery_status = info!("Delivery status: {:?}", delivery_status);
}
