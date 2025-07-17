pub mod agent;
pub mod caracat;
pub mod client;
pub mod kafka;

use anyhow::Result;
use config::Config;
use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{IpAddr, SocketAddr};
use tokio::net::lookup_host;

pub use agent::{AgentConfig, RawAgentConfig};
pub use caracat::CaracatConfig;
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
        cfg.validate_and_normalize();
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
