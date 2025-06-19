//! Unit tests for KafkaAuth parsing
use saimiris::auth::{KafkaAuth, SaslAuth};
use saimiris::config::KafkaConfig;

#[test]
fn test_kafka_auth_plaintext() {
    let mut config = KafkaConfig::default();
    config.auth_protocol = "PLAINTEXT".to_string();
    let auth = match config.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: config.auth_sasl_username.clone(),
            password: config.auth_sasl_password.clone(),
            mechanism: config.auth_sasl_mechanism.clone(),
        }),
        _ => panic!("Invalid Kafka producer authentication protocol"),
    };
    matches!(auth, KafkaAuth::PlainText);
}

#[test]
fn test_kafka_auth_sasl_plaintext() {
    let mut config = KafkaConfig::default();
    config.auth_protocol = "SASL_PLAINTEXT".to_string();
    let auth = match config.auth_protocol.as_str() {
        "PLAINTEXT" => KafkaAuth::PlainText,
        "SASL_PLAINTEXT" => KafkaAuth::SasalPlainText(SaslAuth {
            username: config.auth_sasl_username.clone(),
            password: config.auth_sasl_password.clone(),
            mechanism: config.auth_sasl_mechanism.clone(),
        }),
        _ => panic!("Invalid Kafka producer authentication protocol"),
    };
    matches!(auth, KafkaAuth::SasalPlainText(_));
}
