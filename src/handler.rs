use anyhow::Result;
use caracat::high_level::Config;
use caracat::models::Probe;
use log::{info, warn};
use rdkafka::message::BorrowedMessage;
use rdkafka::message::{Headers, Message};
use std::net::IpAddr;
use std::time::Duration;
use tokio::task;

use crate::prober::probe;

fn create_config() -> Config {
    Config {
        allowed_prefixes_file: None,
        blocked_prefixes_file: None,
        batch_size: 128,
        dry_run: false,
        extra_string: None,
        min_ttl: None,
        max_ttl: None,
        integrity_check: true,
        instance_id: 0,
        interface: caracat::utilities::get_default_interface(),
        max_probes: None,
        packets: 1,
        probing_rate: 100,
        rate_limiting_method: caracat::rate_limiter::RateLimitingMethod::Auto,
        receiver_wait_time: Duration::new(3, 0),
    }
}

struct Payload {
    dst_addr: IpAddr,
    min_ttl: u8,
    max_ttl: u8,
}

fn decode_payload(payload: &str) -> Result<Payload> {
    let parts: Vec<&str> = payload.split(',').collect();
    Ok(Payload {
        dst_addr: parts[0].parse()?,
        min_ttl: parts[1].parse()?,
        max_ttl: parts[2].parse()?,
    })
}

pub async fn handle(m: &BorrowedMessage<'_>) -> Result<()> {
    let payload = match m.payload_view::<str>() {
        None => "",
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            warn!("Error while deserializing message payload: {:?}", e);
            ""
        }
    };

    info!(
        "key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
        m.key(),
        payload,
        m.topic(),
        m.partition(),
        m.offset(),
        m.timestamp()
    );

    if let Some(headers) = m.headers() {
        for header in headers.iter() {
            info!("  Header {:#?}: {:?}", header.key, header.value);
        }
    }

    // Probing
    let config = create_config();
    let payload = decode_payload(payload)?;

    let mut probes_to_send = vec![];
    for i in payload.min_ttl..=payload.max_ttl {
        probes_to_send.push(Probe {
            dst_addr: payload.dst_addr.clone(),
            src_port: 24000,
            dst_port: 33434,
            ttl: i,
            protocol: caracat::models::L4::ICMPv6,
        });
    }
    let result = task::spawn_blocking(move || {
        probe(
            config,
            probes_to_send.into_iter(),
            Some(String::from("./test.csv")),
        )
    })
    .await?;

    if let Err(e) = result {
        warn!("Error while probing: {:?}", e);
    }

    Ok(())
}
