use anyhow::Result;
use caracat::rate_limiter::RateLimitingMethod;
use config::Config;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::lookup_host;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Agent identifier.
    /// Default: random
    pub id: String,

    /// Metrics listener address (IP or FQDN) for Prometheus endpoint
    /// Default: 0.0.0.0:8080
    pub metrics_address: SocketAddr,
}

#[derive(Debug, Clone)]
pub struct CaracatConfig {
    /// Number of probes to send before calling the rate limiter.
    /// Default: 100
    pub batch_size: u64,

    /// Identifier encoded in the probes (random by default).
    /// Default: 0
    pub instance_id: u16,

    /// Whether to actually send the probes on the network or not.
    /// Default: false
    pub dry_run: bool,

    /// Do not send probes with ttl < min_ttl.
    /// Default: None
    pub min_ttl: Option<u8>,

    /// Do not send probes with ttl > max_ttl.
    /// Default: None
    pub max_ttl: Option<u8>,

    /// Check that replies match valid probes.
    /// Default: false
    pub integrity_check: bool,

    /// Interface from which to send the packets.
    /// Default: default interface
    pub interface: String,

    /// Source IPv4 address
    /// Default: None
    pub src_ipv4_addr: Option<Ipv4Addr>,

    /// Source IPv6 address
    /// Default: None
    pub src_ipv6_addr: Option<Ipv6Addr>,

    /// Number of packets to send per probe.
    /// Default: 1
    pub packets: u64,

    /// Probing rate in packets per second.
    /// Default: 100
    pub probing_rate: u64,

    /// Method to use to limit the packets rate.
    /// Default: Auto
    pub rate_limiting_method: RateLimitingMethod,
}

#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Kafka brokers
    /// Default: localhost:9092
    pub brokers: String,

    /// Kafka Authentication Protocol
    /// Default: PLAINTEXT
    pub auth_protocol: String,

    /// Kafka Authentication SASL Username
    /// Default: saimiris
    pub auth_sasl_username: String,

    /// Kafka Authentication SASL Password
    /// Default: saimiris
    pub auth_sasl_password: String,

    /// Kafka Authentication SASL Mechanism
    /// Default: SCRAM-SHA-512
    pub auth_sasl_mechanism: String,

    /// Kafka message max bytes
    /// Default: 1048576
    pub message_max_bytes: usize,

    /// Kafka consumer topics (comma separated)
    /// Default: saimiris-targets
    pub in_topics: String,

    /// Kafka consumer group ID
    /// Default: saimiris-agent
    pub in_group_id: String,

    /// Enable Kafka producer
    /// Default: true
    pub out_enable: bool,

    /// Kafka producer topic
    /// Default: saimiris-results
    pub out_topic: String,

    /// Kafka producer batch wait time
    /// Default: 1000
    pub out_batch_wait_time: u64,

    /// Kafka producer batch wait interval
    /// Default: 100
    pub out_batch_wait_interval: u64,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub caracat: CaracatConfig,
    pub kafka: KafkaConfig,
}

fn load_config(config_path: &str) -> Config {
    Config::builder()
        .add_source(config::File::with_name(config_path))
        .add_source(config::Environment::with_prefix("SAIMIRIS"))
        .build()
        .unwrap()
}

pub async fn resolve_address(address: String) -> Result<SocketAddr> {
    match lookup_host(&address).await?.next() {
        Some(addr) => Ok(addr),
        None => anyhow::bail!("Failed to resolve address: {}", address),
    }
}

pub async fn app_config(config_path: &str) -> Result<AppConfig> {
    let config = load_config(config_path);

    let metric_address = resolve_address(
        config
            .get_string("agent.metrics_address")
            .unwrap_or("0.0.0.0:8080".to_string()),
    )
    .await?;
    Ok(AppConfig {
        // Agent configuration
        agent: AgentConfig {
            id: config.get_string("agent.id").unwrap_or("none".to_string()),
            metrics_address: metric_address,
        },

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
            packets: config.get_int("caracat.packets").unwrap_or(1) as u64,
            probing_rate: config.get_int("caracat.probing_rate").unwrap_or(100) as u64,
            rate_limiting_method: caracat::rate_limiter::RateLimitingMethod::Auto, // TODO
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
            message_max_bytes: config.get_int("kafka.message_max_bytes").unwrap_or(990000) as usize,
            in_topics: config
                .get_string("kafka.in_topics")
                .unwrap_or("saimiris-targets".to_string()),
            in_group_id: config
                .get_string("kafka.in_group_id")
                .unwrap_or("saimiris-agent".to_string()),
            out_enable: config.get_bool("kafka.out_enable").unwrap_or(true),
            out_topic: config
                .get_string("kafka.out_topic")
                .unwrap_or("saimiris-results".to_string()),
            out_batch_wait_time: config.get_int("kafka.out_batch_wait_time").unwrap_or(1000) as u64,
            out_batch_wait_interval: config
                .get_int("kafka.out_batch_wait_interval")
                .unwrap_or(100) as u64,
        },
    })
}
