pub mod agent;
pub mod client;
pub mod kafka;

use anyhow::Result;
use config::Config;
use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{IpAddr, SocketAddr};
use tokio::net::lookup_host;

pub use agent::{AgentConfig, RawAgentConfig};
pub use client::{parse_and_validate_client_args, ClientConfig};
pub use kafka::KafkaConfig;

// --- IP prefix validation utilities ---
pub fn validate_ip_against_prefixes(
    ip_str: &str,
    ipv4_prefix: &Option<String>,
    ipv6_prefix: &Option<String>,
) -> Result<()> {
    let ip: IpAddr = ip_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid IP address format: {}", ip_str))?;

    match ip {
        IpAddr::V4(ipv4) => {
            if let Some(prefix_str) = ipv4_prefix {
                let prefix: Ipv4Net = prefix_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid IPv4 prefix format: {}", prefix_str))?;
                if !prefix.contains(&ipv4) {
                    return Err(anyhow::anyhow!(
                        "IPv4 address {} is not within the allowed prefix {}",
                        ip_str,
                        prefix_str
                    ));
                }
            } else {
                return Err(anyhow::anyhow!(
                    "IPv4 address {} provided but no IPv4 prefix configured for agent",
                    ip_str
                ));
            }
        }
        IpAddr::V6(ipv6) => {
            if let Some(prefix_str) = ipv6_prefix {
                let prefix: Ipv6Net = prefix_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid IPv6 prefix format: {}", prefix_str))?;
                if !prefix.contains(&ipv6) {
                    return Err(anyhow::anyhow!(
                        "IPv6 address {} is not within the allowed prefix {}",
                        ip_str,
                        prefix_str
                    ));
                }
            } else {
                return Err(anyhow::anyhow!(
                    "IPv6 address {} provided but no IPv6 prefix configured for agent",
                    ip_str
                ));
            }
        }
    }

    Ok(())
}

// --- Shared utilities ---
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

// --- Gateway config (shared between agent and potentially client) ---
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct GatewayConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub agent_key: Option<String>,
    #[serde(default)]
    pub agent_secret: Option<String>,
}

// --- Main app config structure ---
#[derive(Debug, Clone, serde::Deserialize)]
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

// --- Caracat config (agent-specific but kept here since it's used in AppConfig) ---
// Constants
const DEFAULT_CARACAT_BATCH_SIZE: u64 = 100;
const DEFAULT_CARACAT_INSTANCE_ID: u16 = 0;
const DEFAULT_CARACAT_PACKETS: u64 = 1;
const DEFAULT_CARACAT_PROBING_RATE: u64 = 100;
const DEFAULT_RATE_LIMITING_METHOD: &str = "auto";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
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
    pub src_ipv4_prefix: Option<String>,
    #[serde(default)]
    pub src_ipv6_prefix: Option<String>,
    #[serde(default = "default_caracat_packets")]
    pub packets: u64,
    #[serde(default = "default_caracat_probing_rate")]
    pub probing_rate: u64,
    #[serde(default = "default_rate_limiting_method")]
    pub rate_limiting_method: String,
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

fn default_rate_limiting_method() -> String {
    DEFAULT_RATE_LIMITING_METHOD.to_string()
}

// --- Main app config loading ---
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
