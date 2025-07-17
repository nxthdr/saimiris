use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::task::spawn;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, warn};

use crate::config::CaracatConfig;

// This struct matches the AgentConfig expected by the gateway
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GatewayAgentConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub batch_size: u64,
    pub instance_id: u16,
    pub dry_run: bool,
    pub min_ttl: Option<u8>,
    pub max_ttl: Option<u8>,
    pub integrity_check: bool,
    pub interface: String,
    pub src_ipv4_prefix: Option<String>,
    pub src_ipv6_prefix: Option<String>,
    pub packets: u64,
    pub probing_rate: u64,
    pub rate_limiting_method: String,
}

impl From<&CaracatConfig> for GatewayAgentConfig {
    fn from(config: &CaracatConfig) -> Self {
        Self {
            name: config.name.clone(),
            batch_size: config.batch_size,
            instance_id: config.instance_id,
            dry_run: config.dry_run,
            min_ttl: config.min_ttl,
            max_ttl: config.max_ttl,
            integrity_check: config.integrity_check,
            interface: config.interface.clone(),
            src_ipv4_prefix: config.src_ipv4_prefix.clone(),
            src_ipv6_prefix: config.src_ipv6_prefix.clone(),
            packets: config.packets,
            probing_rate: config.probing_rate,
            rate_limiting_method: config.rate_limiting_method.clone(),
        }
    }
}

pub fn spawn_healthcheck_loop(
    gateway_url: String,
    agent_id: String,
    agent_key: String,
    agent_secret: String,
    caracat_configs: Vec<CaracatConfig>,
) {
    let base_url = gateway_url.trim_end_matches('/').to_string();
    let agent_url = format!("{}/api/agent/{}", base_url, agent_id);
    let config_url = format!("{}/agent-api/agent/{}/config", base_url, agent_id);
    let health_url = format!("{}/agent-api/agent/{}/health", base_url, agent_id);
    let register_url = format!("{}/agent-api/agent/register", base_url);

    spawn(async move {
        debug!(
            "Starting healthcheck loop for agent {} with gateway {}",
            agent_id, base_url
        );
        let client = Client::new();

        // Add initial delay to allow gateway to start up
        sleep(Duration::from_secs(5)).await;

        loop {
            // Step 1: Check if agent exists (GET /agent/{id})
            let mut needs_registration = false;

            debug!("Checking if agent exists on gateway");
            match client
                .get(&agent_url)
                .header("authorization", format!("Bearer {}", agent_key))
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => {
                    debug!("Agent exists on gateway");
                }
                Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
                    debug!("Agent does not exist on gateway, will register");
                    needs_registration = true;
                }
                Ok(r) => {
                    warn!("Unexpected status when checking agent: {}", r.status());
                    needs_registration = true; // Try registration just in case
                }
                Err(e) => {
                    error!("Failed to check if agent exists: {}", e);
                    debug!("Network error during agent existence check, gateway might not be ready yet");
                    // Skip this iteration if we can't connect to the gateway
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            }

            // Step 3: Register agent if needed
            if needs_registration {
                debug!("Registering agent with gateway");
                let register_body = serde_json::json!({
                    "id": agent_id,
                    "secret": agent_secret
                });

                match client
                    .post(&register_url)
                    .header("authorization", format!("Bearer {}", agent_key))
                    .json(&register_body)
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        debug!("Successfully registered agent with gateway");
                    }
                    Ok(r) if r.status() == reqwest::StatusCode::CONFLICT => {
                        debug!("Agent already registered at gateway (unexpected conflict)");
                    }
                    Ok(r) => {
                        error!("Failed to register agent: {}", r.status());
                        // Don't continue with config/health updates if registration failed
                        debug!("Skipping config and health updates due to registration failure, will retry in 30 seconds");
                        sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                    Err(e) => {
                        error!("Failed to register agent: {}", e);
                        debug!("Network error during registration, will retry in 30 seconds");
                        sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
            }

            // Step 4: Update agent config
            let gateway_configs: Vec<GatewayAgentConfig> = caracat_configs
                .iter()
                .map(|config| GatewayAgentConfig::from(config))
                .collect();

            match client
                .post(&config_url)
                .header("authorization", format!("Bearer {}", agent_key))
                .json(&gateway_configs)
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => {
                    debug!("Successfully sent agent config to gateway");
                }
                Ok(r) => {
                    error!("Failed to send agent config: {}", r.status());
                    // Don't fail the entire loop, just continue to health check
                }
                Err(e) => {
                    error!("Failed to send agent config: {}", e);
                    debug!("Network error during config update, will retry in 30 seconds");
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            }

            // Step 5: Send healthcheck update
            let health = serde_json::json!({
                "healthy": true,
                "last_check": chrono::Utc::now().to_rfc3339(),
                "message": null
            });

            match client
                .post(&health_url)
                .header("authorization", format!("Bearer {}", agent_key))
                .json(&health)
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => {
                    debug!("Healthcheck sent to gateway");
                }
                Ok(r) => {
                    warn!("Failed to send healthcheck: {}", r.status());
                    // Don't fail the entire loop, just log and continue
                }
                Err(e) => {
                    error!("Failed to send healthcheck: {}", e);
                    debug!("Network error during healthcheck, will retry in 30 seconds");
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            }

            debug!("Healthcheck cycle completed, sleeping for 30 seconds");
            sleep(Duration::from_secs(30)).await; // TODO: make interval configurable
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caracat_to_gateway_config_conversion() {
        let caracat_config = CaracatConfig {
            name: Some("test-config".to_string()),
            batch_size: 100,
            instance_id: 1,
            dry_run: false,
            min_ttl: Some(10),
            max_ttl: Some(255),
            integrity_check: true,
            interface: "eth0".to_string(),
            src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
            src_ipv6_prefix: Some("2001:db8::/32".to_string()),
            packets: 1000,
            probing_rate: 100,
            rate_limiting_method: "None".to_string(),
        };

        let gateway_config: GatewayAgentConfig = (&caracat_config).into();

        assert_eq!(gateway_config.name, Some("test-config".to_string()));
        assert_eq!(gateway_config.batch_size, 100);
        assert_eq!(gateway_config.instance_id, 1);
        assert_eq!(gateway_config.dry_run, false);
        assert_eq!(gateway_config.min_ttl, Some(10));
        assert_eq!(gateway_config.max_ttl, Some(255));
        assert_eq!(gateway_config.integrity_check, true);
        assert_eq!(gateway_config.interface, "eth0".to_string());
        assert_eq!(
            gateway_config.src_ipv4_prefix,
            Some("192.168.1.0/24".to_string())
        );
        assert_eq!(
            gateway_config.src_ipv6_prefix,
            Some("2001:db8::/32".to_string())
        );
        assert_eq!(gateway_config.packets, 1000);
        assert_eq!(gateway_config.probing_rate, 100);
        assert_eq!(gateway_config.rate_limiting_method, "None".to_string());
    }

    #[test]
    fn test_gateway_config_serialization() {
        let gateway_config = GatewayAgentConfig {
            name: Some("test-config".to_string()),
            batch_size: 100,
            instance_id: 1,
            dry_run: false,
            min_ttl: Some(10),
            max_ttl: Some(255),
            integrity_check: true,
            interface: "eth0".to_string(),
            src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
            src_ipv6_prefix: Some("2001:db8::/32".to_string()),
            packets: 1000,
            probing_rate: 100,
            rate_limiting_method: "None".to_string(),
        };

        let serialized = serde_json::to_string(&gateway_config).unwrap();
        let deserialized: GatewayAgentConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(gateway_config.name, deserialized.name);
        assert_eq!(gateway_config.batch_size, deserialized.batch_size);
        assert_eq!(gateway_config.probing_rate, deserialized.probing_rate);
    }
}
