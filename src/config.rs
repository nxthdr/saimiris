use anyhow::Result;
use caracat::rate_limiter::RateLimitingMethod;
use config::Config;
use serde::{de, Deserialize, Deserializer};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::lookup_host;

// --- AgentConfig ---
fn default_agent_id() -> String {
    "".to_string()
}
fn default_agent_metrics_address_str() -> String {
    "0.0.0.0:8080".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawAgentConfig {
    #[serde(default = "default_agent_id")]
    pub id: String,
    #[serde(default = "default_agent_metrics_address_str")]
    pub metrics_address: String,
}

// --- CaracatConfig ---
fn default_caracat_batch_size() -> u64 {
    100
}
fn default_caracat_instance_id() -> u16 {
    0
}
fn default_caracat_interface() -> String {
    caracat::utilities::get_default_interface()
}
fn default_caracat_packets() -> u64 {
    1
}
fn default_caracat_probing_rate() -> u64 {
    100
}
fn default_caracat_rate_limiting_method() -> RateLimitingMethod {
    RateLimitingMethod::Auto
}

fn deserialize_rate_limiting_method<'de, D>(deserializer: D) -> Result<RateLimitingMethod, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_lowercase().as_str() {
        "auto" => Ok(RateLimitingMethod::Auto),
        "active" => Ok(RateLimitingMethod::Active),
        "sleep" => Ok(RateLimitingMethod::Sleep),
        "none" => Ok(RateLimitingMethod::None),
        _ => Err(de::Error::unknown_variant(
            &s,
            &["auto", "active", "sleep", "none"],
        )),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaracatConfig {
    #[serde(default = "default_caracat_batch_size")]
    pub batch_size: u64,
    #[serde(default = "default_caracat_instance_id")]
    pub instance_id: u16,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub min_ttl: Option<u8>,
    #[serde(default)]
    pub max_ttl: Option<u8>,
    #[serde(default)]
    pub integrity_check: bool,
    #[serde(default = "default_caracat_interface")]
    pub interface: String,
    #[serde(default)]
    pub src_ipv4_addr: Option<Ipv4Addr>,
    #[serde(default)]
    pub src_ipv6_addr: Option<Ipv6Addr>,
    #[serde(default = "default_caracat_packets")]
    pub packets: u64,
    #[serde(default = "default_caracat_probing_rate")]
    pub probing_rate: u64,
    #[serde(
        default = "default_caracat_rate_limiting_method",
        deserialize_with = "deserialize_rate_limiting_method"
    )]
    pub rate_limiting_method: RateLimitingMethod,
}

// --- KafkaConfig ---
fn default_kafka_brokers() -> String {
    "localhost:9092".to_string()
}
fn default_kafka_auth_protocol() -> String {
    "PLAINTEXT".to_string()
}
fn default_kafka_auth_sasl_username() -> String {
    "saimiris".to_string()
}
fn default_kafka_auth_sasl_password() -> String {
    "saimiris".to_string()
}
fn default_kafka_auth_sasl_mechanism() -> String {
    "SCRAM-SHA-512".to_string()
}
fn default_kafka_message_max_bytes() -> usize {
    990_000
}
fn default_kafka_in_topics() -> String {
    "saimiris-targets".to_string()
}
fn default_kafka_in_group_id() -> String {
    "saimiris-agent".to_string()
}
fn default_kafka_out_enable() -> bool {
    true
}
fn default_kafka_out_topic() -> String {
    "saimiris-results".to_string()
}
fn default_kafka_out_batch_wait_time() -> u64 {
    1000
}
fn default_kafka_out_batch_wait_interval() -> u64 {
    100
}

#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    #[serde(default = "default_kafka_brokers")]
    pub brokers: String,
    #[serde(default = "default_kafka_auth_protocol")]
    pub auth_protocol: String,
    #[serde(default = "default_kafka_auth_sasl_username")]
    pub auth_sasl_username: String,
    #[serde(default = "default_kafka_auth_sasl_password")]
    pub auth_sasl_password: String,
    #[serde(default = "default_kafka_auth_sasl_mechanism")]
    pub auth_sasl_mechanism: String,
    #[serde(default = "default_kafka_message_max_bytes")]
    pub message_max_bytes: usize,
    #[serde(default = "default_kafka_in_topics")]
    pub in_topics: String,
    #[serde(default = "default_kafka_in_group_id")]
    pub in_group_id: String,
    #[serde(default = "default_kafka_out_enable")]
    pub out_enable: bool,
    #[serde(default = "default_kafka_out_topic")]
    pub out_topic: String,
    #[serde(default = "default_kafka_out_batch_wait_time")]
    pub out_batch_wait_time: u64,
    #[serde(default = "default_kafka_out_batch_wait_interval")]
    pub out_batch_wait_interval: u64,
}

// --- AppConfig ---
#[derive(Debug, Clone, Deserialize)]
pub struct RawAppConfig {
    #[serde(default)]
    agent: RawAgentConfig,
    #[serde(default)]
    caracat: Vec<CaracatConfig>,
    #[serde(default)]
    kafka: KafkaConfig,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub caracat: Vec<CaracatConfig>,
    pub kafka: KafkaConfig,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub id: String,
    pub metrics_address: SocketAddr,
}

// --- Loading ---
fn load_config_source(config_path: &str) -> Result<Config> {
    Config::builder()
        .add_source(config::File::with_name(config_path).required(false))
        .add_source(config::Environment::with_prefix("SAIMIRIS").separator("__"))
        .build()
        .map_err(Into::into)
}

pub async fn resolve_address(address: String) -> Result<SocketAddr> {
    match lookup_host(&address).await?.next() {
        Some(addr) => Ok(addr),
        None => anyhow::bail!("Failed to resolve address: {}", address),
    }
}

pub async fn app_config(config_path: &str) -> Result<AppConfig> {
    let config_source = load_config_source(config_path)?;

    let raw_config: RawAppConfig = config_source.try_deserialize()?;

    let resolved_metrics_address =
        resolve_address(raw_config.agent.metrics_address.clone()).await?;

    // use default caracat config if not provided
    let caracat_configs = if raw_config.caracat.is_empty() {
        vec![CaracatConfig::default()]
    } else {
        raw_config.caracat
    };

    Ok(AppConfig {
        agent: AgentConfig {
            id: raw_config.agent.id,
            metrics_address: resolved_metrics_address,
        },
        caracat: caracat_configs,
        kafka: raw_config.kafka,
    })
}

impl Default for CaracatConfig {
    fn default() -> Self {
        Self {
            batch_size: default_caracat_batch_size(),
            instance_id: default_caracat_instance_id(),
            dry_run: false,
            min_ttl: None,
            max_ttl: None,
            integrity_check: false,
            interface: default_caracat_interface(),
            src_ipv4_addr: None,
            src_ipv6_addr: None,
            packets: default_caracat_packets(),
            probing_rate: default_caracat_probing_rate(),
            rate_limiting_method: default_caracat_rate_limiting_method(),
        }
    }
}

impl Default for RawAgentConfig {
    fn default() -> Self {
        Self {
            id: default_agent_id(),
            metrics_address: default_agent_metrics_address_str(),
        }
    }
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: default_kafka_brokers(),
            auth_protocol: default_kafka_auth_protocol(),
            auth_sasl_username: default_kafka_auth_sasl_username(),
            auth_sasl_password: default_kafka_auth_sasl_password(),
            auth_sasl_mechanism: default_kafka_auth_sasl_mechanism(),
            message_max_bytes: default_kafka_message_max_bytes(),
            in_topics: default_kafka_in_topics(),
            in_group_id: default_kafka_in_group_id(),
            out_enable: default_kafka_out_enable(),
            out_topic: default_kafka_out_topic(),
            out_batch_wait_time: default_kafka_out_batch_wait_time(),
            out_batch_wait_interval: default_kafka_out_batch_wait_interval(),
        }
    }
}
