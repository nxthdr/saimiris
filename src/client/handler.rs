use anyhow::Result;
use log::trace;

use crate::auth::{KafkaAuth, SaslAuth};
use crate::client::producer::produce;
use crate::config::AppConfig;
use crate::target::decode_target;

pub async fn handle(config: &AppConfig, agents: &str, target: &str) -> Result<()> {
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

    let agents = agents.split(',').collect::<Vec<&str>>();
    decode_target(target)?;

    produce(config, auth, agents, target).await;

    Ok(())
}
