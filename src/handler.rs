use anyhow::Result;
use caracat::models::Probe;
use caracat::rate_limiter::RateLimitingMethod;
use log::{info, warn};
use rdkafka::message::BorrowedMessage;
use rdkafka::message::{Headers, Message};
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::time::Duration;
use tokio::task;

use crate::prober::probe;

/// Probing configuration.
#[derive(Debug)]
pub struct Config {
    /// Number of probes to send before calling the rate limiter.
    pub batch_size: u64,
    /// Identifier encoded in the probes (random by default).
    pub instance_id: u16,
    /// Whether to actually send the probes on the network or not.
    pub dry_run: bool,
    /// Do not send probes with ttl < min_ttl.
    pub min_ttl: Option<u8>,
    /// Do not send probes with ttl > max_ttl.
    pub max_ttl: Option<u8>,
    /// Check that replies match valid probes.
    pub integrity_check: bool,
    /// Interface from which to send the packets.
    pub interface: String,
    /// Source IPv4 address
    pub src_ipv4_addr: Option<Ipv4Addr>,
    /// Source IPv6 address
    pub src_ipv6_addr: Option<Ipv6Addr>,
    /// Maximum number of probes to send (unlimited by default).
    pub max_probes: Option<u64>,
    /// Number of packets to send per probe.
    pub packets: u64,
    /// Probing rate in packets per second.
    pub probing_rate: u64,
    /// Method to use to limit the packets rate.
    pub rate_limiting_method: RateLimitingMethod,
    /// Time in seconds to wait after sending the probes to stop the receiver.
    pub receiver_wait_time: Duration,
}

fn create_config() -> Config {
    Config {
        batch_size: 128,
        dry_run: false,
        min_ttl: None,
        max_ttl: None,
        integrity_check: true,
        instance_id: 0,
        interface: caracat::utilities::get_default_interface(),
        src_ipv4_addr: None,
        src_ipv6_addr: None,
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
