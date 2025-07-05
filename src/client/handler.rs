use anyhow::Result;
use caracat::models::Probe;
use csv::ReaderBuilder;
use std::io::{stdin, BufRead};
use tracing::trace;

use crate::auth::{KafkaAuth, SaslAuth};
use crate::client::producer::produce;
use crate::config::{AppConfig, ClientConfig};

pub fn read_probes_from_csv<R: BufRead>(buf_reader: R) -> Result<Vec<Probe>> {
    let probes = Vec::new();
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(buf_reader);

    rdr.deserialize().enumerate().try_fold(
        probes,
        |mut acc, (i, result): (usize, Result<Probe, _>)| {
            acc.push(result.map_err(|e: csv::Error| {
                anyhow::anyhow!(e).context(format!(
                    "Failed to deserialize probe from CSV at line {}",
                    i + 1
                ))
            })?);
            Ok(acc)
        },
    )
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
