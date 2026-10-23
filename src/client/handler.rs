use anyhow::Result;
use caracat::models::Probe;
use csv::ReaderBuilder;
use std::io::{stdin, BufRead};
use tracing::trace;

use crate::auth::{KafkaAuth, SaslAuth};
use crate::client::producer::produce;
use crate::config::{AppConfig, ClientConfig};

pub fn read_probes_from_csv<R: BufRead>(buf_reader: R) -> Result<Vec<Probe>> {
    let mut probes = Vec::new();
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(buf_reader);

    for (i, result) in rdr.records().enumerate() {
        let record = result.map_err(|e| {
            anyhow::anyhow!(e).context(format!("Failed to read CSV record at line {}", i + 1))
        })?;

        // Parse CSV fields manually and construct Probe
        // Assuming CSV format: dst_addr,src_port,dst_port,ttl,protocol
        if record.len() < 5 {
            return Err(anyhow::anyhow!(
                "Invalid CSV format at line {}: expected 5 fields, got {}",
                i + 1,
                record.len()
            ));
        }

        let dst_addr = record[0].parse().map_err(|e| {
            anyhow::anyhow!("Failed to parse dst_addr at line {}: {}", i + 1, e)
        })?;

        let src_port = record[1].parse().map_err(|e| {
            anyhow::anyhow!("Failed to parse src_port at line {}: {}", i + 1, e)
        })?;

        let dst_port = record[2].parse().map_err(|e| {
            anyhow::anyhow!("Failed to parse dst_port at line {}: {}", i + 1, e)
        })?;

        let ttl = record[3].parse().map_err(|e| {
            anyhow::anyhow!("Failed to parse ttl at line {}: {}", i + 1, e)
        })?;

        let protocol = match record[4].to_lowercase().as_str() {
            "udp" => caracat::models::L4::UDP,
            "icmp" => caracat::models::L4::ICMP,
            "icmpv6" => caracat::models::L4::ICMPv6,
            other => {
                return Err(anyhow::anyhow!(
                    "Invalid protocol '{}' at line {}",
                    other,
                    i + 1
                ))
            }
        };

        probes.push(Probe {
            dst_addr,
            src_port,
            dst_port,
            ttl,
            protocol,
        });
    }

    Ok(probes)
}

pub async fn handle(config: &AppConfig, client_config: ClientConfig) -> Result<()> {
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
    let probes = match client_config.probes_file {
        Some(probes_file) => {
            let file = std::fs::File::open(probes_file)?;
            let buf_reader = std::io::BufReader::new(file);
            read_probes_from_csv(buf_reader)?
        }
        None => {
            let stdin = stdin();
            let buf_reader = stdin.lock();
            read_probes_from_csv(buf_reader)?
        }
    };

    // Produce Kafka messages
    produce(config, auth, client_config.measurement_infos, probes).await;

    Ok(())
}
