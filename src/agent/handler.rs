use anyhow::Result;
use caracat::models::Probe;
use ipnet::IpNet;
use log::{info, trace};
use rand::seq::SliceRandom;
use rand::thread_rng;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Headers;
use rdkafka::Message;
use std::sync::mpsc::channel;
use tokio::task;

use crate::agent::consumer::init_consumer;
use crate::agent::producer::produce;
use crate::agent::receiver::ReceiveLoop;
use crate::agent::sender::send;
use crate::auth::{KafkaAuth, SaslAuth};
use crate::config::AppConfig;
use crate::target::{decode_target, Target};
use crate::utils::test_id;

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
                    protocol: target.protocol.clone(),
                });
            }
        }
    }

    Ok(probes)
}

pub async fn handle(config: &AppConfig) -> Result<()> {
    trace!("Agent handler");
    trace!("{:?}", config);

    // Test input ID
    if !test_id(Some(config.agent.agent_id.clone()), None, None) {
        return Err(anyhow::anyhow!("Invalid agent ID"));
    }

    info!("Agent ID: {}", config.agent.agent_id);

    // Configure Kafka authentication
    let kafka_auth = match config.kafka.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: config.kafka.auth_sasl_username.clone(),
            password: config.kafka.auth_sasl_password.clone(),
            mechanism: config.kafka.auth_sasl_mechanism.clone(),
        }),
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid Kafka producer authentication protocol"
            ))
        }
    };

    // MPSC channel to communicate between reveiver task and Kafka producer task
    let (tx, rx) = channel();

    // Start the receiver thread
    info!("Starting receiver");
    ReceiveLoop::new(tx, config.caracat.clone());

    // Start the Kafka producer task if enabled
    let config_task = config.clone();
    let kafka_auth_task = kafka_auth.clone();
    task::spawn(async move {
        produce(&config_task, kafka_auth_task, rx).await;
    });

    let consumer = init_consumer(config, kafka_auth.clone()).await;
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

                // Filter out the messages that are not intended for the agent
                // by looking at the agent ID in the headers
                let mut is_intended = false;
                if let Some(headers) = m.headers() {
                    for header in headers.iter() {
                        if header.key == &config.agent.agent_id {
                            is_intended = true;
                            break;
                        }
                    }
                }
                if !is_intended {
                    info!("Target not intended for this agent");

                    // TODO: handle errors
                    consumer.commit_message(&m, CommitMode::Async).unwrap();
                    continue;
                }

                // Probe Generation
                let target = decode_target(target)?;
                let probes_to_send = generate_probes(&target)?;

                // Probing
                let config_clone = config.clone();
                let (send_statistics, rate_limiter_statistics) = task::spawn_blocking(move || {
                    send(config_clone.caracat, probes_to_send.into_iter())
                })
                .await??;

                info!("{}", send_statistics);
                info!("{}", rate_limiter_statistics);

                // Commit the consumed message
                // TODO: handle errors
                consumer.commit_message(&m, CommitMode::Async).unwrap();
            }
        };
    }
}
