use anyhow::Result;
use caracat::models::Probe;
use log::trace;
use std::io::{stdin, BufRead};
use std::path::PathBuf;

use crate::auth::{KafkaAuth, SaslAuth};
use crate::client::producer::produce;
use crate::config::AppConfig;
use crate::probe::decode_probe;

fn read_probes<R: BufRead>(buf_reader: R) -> Result<Vec<Probe>> {
    let mut probes = Vec::new();
    for line in buf_reader.lines() {
        let probe = line?;
        probes.push(decode_probe(&probe)?);
    }
    Ok(probes)
}

pub async fn handle(config: &AppConfig, agents: &str, probes_file: Option<PathBuf>) -> Result<()> {
    trace!("Client handler");
    trace!("{:?}", config);

    // Configure Kafka authentication
    let auth = match config.kafka.auth_protocol.as_str() {
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

    // Read probes from file or stdin
    let probes = match probes_file {
        Some(probes_file) => {
            let file = std::fs::File::open(probes_file)?;
            let buf_reader = std::io::BufReader::new(file);
            read_probes(buf_reader)?
        }
        None => {
            let stdin = stdin();
            let buf_reader = stdin.lock();
            read_probes(buf_reader)?
        }
    };

    // Split the agents
    let agents = agents.split(',').collect::<Vec<&str>>();

    // Produce Kafka messages
    produce(config, auth, agents, probes).await;

    Ok(())
}
