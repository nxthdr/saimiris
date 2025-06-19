use anyhow::Result;
use config::Config;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::lookup_host;

// --- Constants ---
const DEFAULT_AGENT_METRICS_ADDRESS: &str = "0.0.0.0:8080";
const DEFAULT_CARACAT_BATCH_SIZE: u64 = 100;
const DEFAULT_CARACAT_INSTANCE_ID: u16 = 0;
const DEFAULT_CARACAT_PACKETS: u64 = 1;
const DEFAULT_CARACAT_PROBING_RATE: u64 = 100;
const DEFAULT_RATE_LIMITING_METHOD: &str = "auto";
const DEFAULT_KAFKA_BROKERS: &str = "localhost:9092";
const DEFAULT_KAFKA_AUTH_PROTOCOL: &str = "PLAINTEXT";
const DEFAULT_KAFKA_AUTH_SASL_USERNAME: &str = "saimiris";
const DEFAULT_KAFKA_AUTH_SASL_PASSWORD: &str = "saimiris";
const DEFAULT_KAFKA_AUTH_SASL_MECHANISM: &str = "SCRAM-SHA-512";
const DEFAULT_KAFKA_MESSAGE_MAX_BYTES: usize = 990_000;
const DEFAULT_KAFKA_IN_TOPICS: &str = "saimiris-targets";
const DEFAULT_KAFKA_IN_GROUP_ID: &str = "saimiris-agent";
const DEFAULT_KAFKA_OUT_TOPIC: &str = "saimiris-results";
const DEFAULT_KAFKA_OUT_BATCH_WAIT_TIME: u64 = 1000;
const DEFAULT_KAFKA_OUT_BATCH_WAIT_INTERVAL: u64 = 100;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawAgentConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_agent_metrics_address")]
    pub metrics_address: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GatewayConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub agent_key: Option<String>,
    #[serde(default)]
    pub agent_secret: Option<String>,
}

fn default_agent_metrics_address() -> String {
    DEFAULT_AGENT_METRICS_ADDRESS.to_string()
}

// --- CaracatConfig ---
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    #[serde(default = "default_rate_limiting_method")]
    pub rate_limiting_method: String,
}

fn default_rate_limiting_method() -> String {
    DEFAULT_RATE_LIMITING_METHOD.to_string()
}

fn default_caracat_batch_size() -> u64 {
    DEFAULT_CARACAT_BATCH_SIZE
}
fn default_caracat_instance_id() -> u16 {
    DEFAULT_CARACAT_INSTANCE_ID
}
fn default_caracat_interface() -> String {
    caracat::utilities::get_default_interface()
}
fn default_caracat_packets() -> u64 {
    DEFAULT_CARACAT_PACKETS
}
fn default_caracat_probing_rate() -> u64 {
    DEFAULT_CARACAT_PROBING_RATE
}

// --- KafkaConfig ---
fn default_kafka_brokers() -> String {
    DEFAULT_KAFKA_BROKERS.to_string()
}
fn default_kafka_auth_protocol() -> String {
    DEFAULT_KAFKA_AUTH_PROTOCOL.to_string()
}
fn default_kafka_auth_sasl_username() -> String {
    DEFAULT_KAFKA_AUTH_SASL_USERNAME.to_string()
}
fn default_kafka_auth_sasl_password() -> String {
    DEFAULT_KAFKA_AUTH_SASL_PASSWORD.to_string()
}
fn default_kafka_auth_sasl_mechanism() -> String {
    DEFAULT_KAFKA_AUTH_SASL_MECHANISM.to_string()
}
fn default_kafka_message_max_bytes() -> usize {
    DEFAULT_KAFKA_MESSAGE_MAX_BYTES
}
fn default_kafka_in_topics() -> String {
    DEFAULT_KAFKA_IN_TOPICS.to_string()
}
fn default_kafka_in_group_id() -> String {
    DEFAULT_KAFKA_IN_GROUP_ID.to_string()
}
fn default_kafka_out_enable() -> bool {
    true
}
fn default_kafka_out_topic() -> String {
    DEFAULT_KAFKA_OUT_TOPIC.to_string()
}
fn default_kafka_out_batch_wait_time() -> u64 {
    DEFAULT_KAFKA_OUT_BATCH_WAIT_TIME
}
fn default_kafka_out_batch_wait_interval() -> u64 {
    DEFAULT_KAFKA_OUT_BATCH_WAIT_INTERVAL
}

#[derive(Debug, Clone, Deserialize, Default)]
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
    gateway: Option<GatewayConfig>,
    #[serde(default)]
    caracat: Vec<CaracatConfig>,
    #[serde(default)]
    kafka: KafkaConfig,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub gateway: Option<GatewayConfig>,
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
    let mut caracat_configs = if raw_config.caracat.is_empty() {
        vec![CaracatConfig::default()]
    } else {
        raw_config.caracat
    };

    // Validate CaracatConfig fields for each caracat config
    for cfg in &mut caracat_configs {
        if cfg.batch_size == 0 {
            cfg.batch_size = default_caracat_batch_size();
        }
        if cfg.instance_id == 0 {
            cfg.instance_id = default_caracat_instance_id();
        }
        if cfg.interface.is_empty() {
            cfg.interface = default_caracat_interface();
        }
        if cfg.packets == 0 {
            cfg.packets = default_caracat_packets();
        }
        if cfg.probing_rate == 0 {
            cfg.probing_rate = default_caracat_probing_rate();
        }
        if cfg.rate_limiting_method.is_empty() {
            cfg.rate_limiting_method = default_rate_limiting_method();
        }
    }

    let gateway = raw_config.gateway;

    Ok(AppConfig {
        agent: AgentConfig {
            id: raw_config.agent.id,
            metrics_address: resolved_metrics_address,
        },
        gateway,
        caracat: caracat_configs,
        kafka: raw_config.kafka,
    })
}
