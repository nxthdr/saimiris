use std::net::SocketAddr;

// --- Constants ---
const DEFAULT_AGENT_METRICS_ADDRESS: &str = "0.0.0.0:8080";

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct RawAgentConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_agent_metrics_address")]
    pub metrics_address: String,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub id: String,
    pub metrics_address: SocketAddr,
}

fn default_agent_metrics_address() -> String {
    DEFAULT_AGENT_METRICS_ADDRESS.to_string()
}
