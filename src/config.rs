use anyhow::Result;
use config::Config;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::lookup_host;

// --- AgentConfig ---
fn default_agent_id() -> String {
    "".to_string()
}
fn default_agent_metrics_address_str() -> String {
    "0.0.0.0:8080".to_string()
}
fn default_agent_gateway_url() -> Option<String> {
    None
}
fn default_agent_key() -> Option<String> {
    None
}
fn default_agent_secret() -> Option<String> {
    None
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawAgentConfig {
    #[serde(default = "default_agent_id")]
    pub id: String,
    #[serde(default = "default_agent_metrics_address_str")]
    pub metrics_address: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GatewayConfig {
    #[serde(default = "default_agent_gateway_url")]
    pub url: Option<String>,
    #[serde(default = "default_agent_key")]
    pub agent_key: Option<String>,
    #[serde(default = "default_agent_secret")]
    pub agent_secret: Option<String>,
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
    #[serde(default = "default_rate_limiting_method_string")]
    pub rate_limiting_method: String,
}

fn default_rate_limiting_method_string() -> String {
    "auto".to_string()
}

fn default_caracat_batch_size() -> u64 {
    100
}
fn default_caracat_instance_id() -> u16 {
    0
}
fn default_caracat_interface() -> String {
    "eth0".to_string()
}
fn default_caracat_packets() -> u64 {
    1
}
fn default_caracat_probing_rate() -> u64 {
    100
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
    let caracat_configs = if raw_config.caracat.is_empty() {
        vec![CaracatConfig::default()]
    } else {
        raw_config.caracat
    };

    let gateway = raw_config.gateway.map(|g| GatewayConfig {
        url: g.url,
        agent_key: g.agent_key,
        agent_secret: g.agent_secret,
    });

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
