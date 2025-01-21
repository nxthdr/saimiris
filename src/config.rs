use std::{
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use caracat::rate_limiter::RateLimitingMethod;
use config::Config;

#[derive(Debug, Clone)]
pub struct CaracatConfig {
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

#[derive(Debug, Clone)]
pub struct KafkaConfig {
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

#[derive(Debug, Clone)]
pub struct ProberConfig {
    /// Prober identifier.
    pub prober_id: u16,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub caracat: CaracatConfig,
    pub kafka: KafkaConfig,
    pub prober: ProberConfig,
}

pub fn load_config(config_path: &str) -> Config {
    Config::builder()
        .add_source(config::File::with_name(config_path))
        .add_source(config::Environment::with_prefix("SAIMIRIS"))
        .build()
        .unwrap()
}

pub fn prober_config(config: Config) -> AppConfig {
    AppConfig {
        // Caracat configuration
        caracat: CaracatConfig {
            batch_size: config.get_int("caracat.batch_size").unwrap_or(100) as u64,
            instance_id: config.get_int("caracat.instance_id").unwrap_or(0) as u16,
            dry_run: config.get_bool("caracat.dry_run").unwrap_or(false),
            min_ttl: config.get_int("caracat.min_ttl").ok().map(|x| x as u8),
            max_ttl: config.get_int("caracat.max_ttl").ok().map(|x| x as u8),
            integrity_check: config.get_bool("caracat.integrity_check").unwrap_or(false),
            interface: config
                .get_string("caracat.interface")
                .unwrap_or(caracat::utilities::get_default_interface()),
            src_ipv4_addr: config
                .get_string("caracat.src_ipv4_addr")
                .ok()
                .and_then(|x| x.parse().ok()),
            src_ipv6_addr: config
                .get_string("caracat.src_ipv6_addr")
                .ok()
                .and_then(|x| x.parse().ok()),
            max_probes: config.get_int("caracat.max_probes").ok().map(|x| x as u64),
            packets: config.get_int("caracat.packets").unwrap_or(1) as u64,
            probing_rate: config.get_int("caracat.probing_rate").unwrap_or(100) as u64,
            rate_limiting_method: caracat::rate_limiter::RateLimitingMethod::Auto, // TODO
            receiver_wait_time: Duration::from_secs(
                config.get_int("caracat.receiver_wait_time").unwrap_or(3) as u64,
            ),
        },

        // Kafka configuration
        kafka: KafkaConfig {
            brokers: config
                .get_string("kafka.brokers")
                .unwrap_or("localhost:9092".to_string()),
            auth_protocol: config
                .get_string("kafka.auth_protocol")
                .unwrap_or("PLAINTEXT".to_string()),
            auth_sasl_username: config
                .get_string("kafka.auth_sasl_username")
                .unwrap_or("saimiris".to_string()),
            auth_sasl_password: config
                .get_string("kafka.auth_sasl_password")
                .unwrap_or("saimiris".to_string()),
            auth_sasl_mechanism: config
                .get_string("kafka.auth_sasl_mechanism")
                .unwrap_or("SCRAM-SHA-512".to_string()),
            in_topics: config
                .get_string("kafka.in_topics")
                .unwrap_or("saimiris".to_string()),
            in_group_id: config
                .get_string("kafka.in_group_id")
                .unwrap_or("saimiris".to_string()),
            out_topic: config
                .get_string("kafka.out_topic")
                .unwrap_or("saimiris-results".to_string()),
        },

        // Prober configuration
        prober: ProberConfig {
            prober_id: config.get_int("prober.prober_id").unwrap_or(0) as u16,
        },
    }
}
