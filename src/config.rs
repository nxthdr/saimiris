use config::Config;

#[allow(dead_code)]
pub struct AppConfig {
    /// Prober ID
    pub prober_id: u16,

    /// Kafka brokers
    pub brokers: String,

    /// Kafka Authentication Protocol
    pub auth_protocol: String,

    /// Kafka Authentication SASL Username
    pub auth_sasl_username: String,

    /// Kafka Authentication SASL Password
    pub auth_sasl_password: String,

    /// Kafka Authentication SASL Mechanism
    pub auth_sasl_mechanism: String,

    /// Kafka consumer topics (comma separated)
    pub in_topics: String,

    /// Kafka consumer group ID
    pub in_group_id: String,

    /// Kafka producer topic
    pub out_topic: String,
}

pub fn load_config(config_path: &str) -> Config {
    Config::builder()
        .add_source(config::File::with_name(config_path))
        .add_source(config::Environment::with_prefix("OSIRIS"))
        .build()
        .unwrap()
}

pub fn prober_config(config: Config) -> AppConfig {
    AppConfig {
        prober_id: config.get_int("prober.id").unwrap_or(0) as u16,
        brokers: config
            .get_string("kafka.brokers")
            .unwrap_or("localhost:9092".to_string()),
        auth_protocol: config
            .get_string("kafka.auth_protocol")
            .unwrap_or("PLAINTEXT".to_string()),
        auth_sasl_username: config
            .get_string("kafka.auth_sasl_username")
            .unwrap_or("osiris".to_string()),
        auth_sasl_password: config
            .get_string("kafka.auth_sasl_password")
            .unwrap_or("osiris".to_string()),
        auth_sasl_mechanism: config
            .get_string("kafka.auth_sasl_mechanism")
            .unwrap_or("SCRAM-SHA-512".to_string()),
        in_topics: config
            .get_string("kafka.in_topics")
            .unwrap_or("osiris".to_string()),
        in_group_id: config
            .get_string("kafka.in_group_id")
            .unwrap_or("osiris".to_string()),
        out_topic: config
            .get_string("kafka.out_topic")
            .unwrap_or("osiris-results".to_string()),
    }
}
