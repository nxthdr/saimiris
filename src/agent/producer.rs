use caracat::models::{MPLSLabel, Reply};
use log::{debug, error, info, warn};
use rdkafka::config::ClientConfig;
use rdkafka::message::OwnedHeaders;
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
    let mut additional_message = None;
    loop {
        let start_time = std::time::Instant::now();
        let mut final_message = String::new();
        let mut n_messages = 0;

        // Send the additional reply first
        if let Some(message) = additional_message {
            let message_str = encode_reply(config.agent.id.clone(), &message);
            final_message.push_str(&message_str);
            final_message.push_str("\n");
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
            let message_str = encode_reply(config.agent.id.clone(), &message);

            // Max message size is 1048576 bytes (including headers)
            if final_message.len() + message_str.len() + 1 > config.kafka.message_max_bytes {
                additional_message = Some(message);
                break;
            }

            final_message.push_str(&message_str);
            final_message.push_str("\n");
            n_messages += 1;
        }

        if final_message.is_empty() {
            continue;
        }

        // Remove the last newline character
        final_message.pop();

        debug!("{}", final_message);
        info!("Sending {} replies to Kafka", n_messages);

        let delivery_status = producer
            .send(
                FutureRecord::to(config.kafka.out_topic.as_str())
                    .payload(&format!("{}", final_message))
                    .key(&format!("")) // TODO
                    .headers(OwnedHeaders::new()), // TODO
                Duration::from_secs(0),
            )
            .await;

        match delivery_status {
            Ok(status) => info!("{:?}", status),
            Err((error, _)) => {
                error!("{}", error.to_string());
            }
        }
    }
}
