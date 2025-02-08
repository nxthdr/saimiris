use anyhow::Result;
use log::trace;
use std::io::{stdin, BufRead};

use crate::auth::{KafkaAuth, SaslAuth};
use crate::client::producer::produce;
use crate::config::AppConfig;
use crate::probe::decode_probe;

pub async fn handle(config: &AppConfig, agents: &str) -> Result<()> {
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

    // Get probes from stdin
    let mut probes = Vec::new();
    for line in stdin().lock().lines() {
        let probe = line?;
        probes.push(decode_probe(&probe)?);
    }

    // Split agents
    let agents = agents.split(',').collect::<Vec<&str>>();

    produce(config, auth, agents, probes).await;

    Ok(())
}
