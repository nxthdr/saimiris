use anyhow::Result;
use caracat::models::Probe;
use ipnet::IpNet;
use log::info;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Headers;
use rdkafka::Message;
use tokio::task;

use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::AppConfig;
use crate::prober::prober::{load_caracat_config, probe};
use crate::prober::producer::produce;

use crate::consumer::init_consumer;

struct Target {
    prefix: IpNet,
    min_ttl: u8,
    max_ttl: u8,
    n_flows: u64,
}

fn decode_payload(payload: &str) -> Result<Target> {
    let parts: Vec<&str> = payload.split(',').collect();
    Ok(Target {
        prefix: parts[0].parse()?,
        min_ttl: parts[1].parse()?,
        max_ttl: parts[2].parse()?,
        n_flows: parts[3].parse()?,
    })
}

fn generate_probes(target: &Target) -> Result<Vec<Probe>> {
    // TODO: We should pass an iterator instead of a vector.
    let mut probes = vec![];

    // First start by dividing the prefix into /24s or /64s, if necessary.
    let subnets = match target.prefix {
        IpNet::V4(_) => {
            let prefix_len = target.prefix.prefix_len();
            let target_len = if prefix_len > 24 { prefix_len } else { 24 };
            target.prefix.subnets(target_len)
        }
        IpNet::V6(_) => {
            let prefix_len = target.prefix.prefix_len();
            let target_len = if prefix_len > 64 { prefix_len } else { 64 };
            target.prefix.subnets(target_len)
        }
    }?;

    // Iterate over the subnets and generate the probes.
    for subnet in subnets {
        // Right now the probe generation is simplistic, we just iterate over the hosts.
        // If we need more flows than hosts, we will we explicitely fail.
        // TODO: implement mapper-like generator such as the ones in diamond-miner.
        // https://github.com/dioptra-io/diamond-miner/blob/main/diamond_miner/mappers.py
        let mut prefix_hosts = subnet.hosts();
        if target.n_flows > prefix_hosts.count().try_into()? {
            return Err(anyhow::anyhow!("Not enough hosts in the prefix"));
        }

        for _ in 0..target.n_flows {
            let dst_addr = prefix_hosts.next().unwrap();

            // Randomize the probes order within a flow.
            // In YARRP we randomize the probes over the entire probing space.
            // This is of course a very big simplication, but it's not that silly.
            // The rational is to avoid results errors due to path changes.
            // So, for now, probes belonging to the same traceroute flow will be sent close in time.
            // TODO: is this shuffle fast?
            let mut ttls: Vec<u8> = (target.min_ttl..target.max_ttl).collect();
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

pub async fn handle(config: &AppConfig) -> Result<()> {
    // Configure Kafka authentication
    let out_auth = match config.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: config.auth_sasl_username.clone(),
            password: config.auth_sasl_password.clone(),
            mechanism: config.auth_sasl_mechanism.clone(),
        }),
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid Kafka producer authentication protocol"
            ))
        }
    };

    let consumer = init_consumer(config, out_auth.clone()).await;
    loop {
        match consumer.recv().await {
            // Err(e) => return Err(anyhow::anyhow!("Kafka error: {}", e)),
            Err(e) => {
                info!("Kafka error: {}", e);
            }
            Ok(m) => {
                let target = match m.payload_view::<str>() {
                    None => "",
                    Some(Ok(s)) => s,
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!(
                            "Kafka error: Error while deserializing message payload: {:?}",
                            e
                        ))
                    }
                };

                info!(
                    "key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                    m.key(),
                    target,
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

                // Probe Generation
                let caracat_config: super::prober::CaracatConfig = load_caracat_config();
                let target = decode_payload(target)?;
                let probes_to_send = generate_probes(&target)?;

                // Probing
                let result =
                    task::spawn_blocking(move || probe(caracat_config, probes_to_send.into_iter()))
                        .await?;
                let (_, _, results) = result?;

                // Produce the results to Kafka topic
                produce(config, out_auth.clone(), results).await;

                // Commit the consumed message
                let _ = consumer.commit_message(&m, CommitMode::Async).unwrap();
            }
        };
    }
}
