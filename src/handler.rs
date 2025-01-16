use anyhow::Result;
use caracat::models::Probe;
use caracat::rate_limiter::RateLimitingMethod;
use ipnet::IpNet;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::time::Duration;
use tokio::task;

use crate::prober::probe;
use crate::producer::produce;

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
    prefix: IpNet,
    min_ttl: u8,
    max_ttl: u8,
    n_flows: u64,
}

fn decode_payload(payload: &str) -> Result<Payload> {
    let parts: Vec<&str> = payload.split(',').collect();
    Ok(Payload {
        prefix: parts[0].parse()?,
        min_ttl: parts[1].parse()?,
        max_ttl: parts[2].parse()?,
        n_flows: parts[3].parse()?,
    })
}

fn generate_probes(payload: &Payload) -> Result<Vec<Probe>> {
    // TODO: We should pass an iterator instead of a vector.
    let mut probes = vec![];

    // First start by dividing the prefix into /24s or /64s, if necessary.
    let subnets = match payload.prefix {
        IpNet::V4(_) => {
            let prefix_len = payload.prefix.prefix_len();
            let target_len = if prefix_len > 24 { prefix_len } else { 24 };
            payload.prefix.subnets(target_len)
        }
        IpNet::V6(_) => {
            let prefix_len = payload.prefix.prefix_len();
            let target_len = if prefix_len > 64 { prefix_len } else { 64 };
            payload.prefix.subnets(target_len)
        }
    }?;

    // Iterate over the subnets and generate the probes.
    for subnet in subnets {
        // Right now the probe generation is simplistic, we just iterate over the hosts.
        // If we need more flows than hosts, we will we explicitely fail.
        // TODO: implement mapper-like generator such as the ones in diamond-miner.
        // https://github.com/dioptra-io/diamond-miner/blob/main/diamond_miner/mappers.py
        let mut prefix_hosts = subnet.hosts();
        if payload.n_flows > prefix_hosts.count().try_into()? {
            return Err(anyhow::anyhow!("Not enough hosts in the prefix"));
        }

        for _ in 0..payload.n_flows {
            let dst_addr = prefix_hosts.next().unwrap();

            // Randomize the probes order within a flow.
            // In YARRP we randomize the probes over the entire probing space.
            // This is of course a very big simplication, but it's not that silly.
            // The rational is to avoid results errors due to path changes.
            // So, for now, probes belonging to the same traceroute flow will be sent close in time.
            // TODO: is this shuffle fast?
            let mut ttls: Vec<u8> = (payload.min_ttl..payload.max_ttl).collect();
            ttls.shuffle(&mut thread_rng());

            for i in ttls {
                probes.push(Probe {
                    dst_addr,
                    src_port: 24000,
                    dst_port: 33434,
                    ttl: i,
                    protocol: caracat::models::L4::ICMPv6,
                });
            }
        }
    }

    Ok(probes)
}

pub async fn handle(
    brokers: &str,
    _in_group_id: &str,
    _in_topics: &[&str],
    out_topic: &str,
) -> Result<()> {
    let payload = "2606:4700:4700::1111/128,1,32,1";

    // Probing
    let config = create_config();
    let payload = decode_payload(payload)?;

    let probes_to_send = generate_probes(&payload);
    let probes_to_send = match probes_to_send {
        Ok(probes) => probes,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    let result = task::spawn_blocking(move || probe(config, probes_to_send.into_iter())).await?;

    let (_, _, results) = result?;
    produce(brokers, out_topic, results).await;

    Ok(())
}
