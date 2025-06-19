use anyhow::Result;
use reqwest::Client;
use tokio::task::spawn;
use tokio::time::{sleep, Duration};
use tracing::{info, error};
use crate::config::CaracatConfig;

pub async fn register_agent(client: &Client, gateway_url: &str, agent_id: &str, agent_key: &str, agent_secret: &str) -> Result<()> {
    let register_url = format!("{}/agent-api/agent/register", gateway_url.trim_end_matches('/'));
    let register_body = serde_json::json!({
        "id": agent_id,
        "secret": agent_secret
    });
    let resp = client
        .post(&register_url)
        .header("authorization", format!("Bearer {}", agent_key))
        .json(&register_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send registration request to {}: {}", register_url, e))?;
    if resp.status().is_success() {
        info!("Successfully registered agent with gateway");
    } else if resp.status() == reqwest::StatusCode::CONFLICT {
        info!("Agent already registered at gateway");
    } else {
        error!("Failed to register agent: {}", resp.status());
        return Err(anyhow::anyhow!("Failed to register agent: {}", resp.status()));
    }
    Ok(())
}

pub async fn send_agent_config(client: &Client, gateway_url: &str, agent_id: &str, agent_key: &str, caracat_config: &CaracatConfig) -> Result<()> {
    let config_url = format!("{}/agent-api/agent/{}/config", gateway_url.trim_end_matches('/'), agent_id);
    let resp = client
        .post(&config_url)
        .header("authorization", format!("Bearer {}", agent_key))
        .json(caracat_config)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send config request to {}: {}", config_url, e))?;
    if resp.status().is_success() {
        info!("Agent config sent to gateway");
    } else {
        error!("Failed to send agent config: {}", resp.status());
    }
    Ok(())
}

pub fn spawn_healthcheck_loop(gateway_url: String, agent_id: String, agent_key: String) {
    let health_url = format!(
        "{}/agent-api/agent/{}/health",
        gateway_url.trim_end_matches('/'),
        agent_id
    );
    spawn(async move {
        let client = Client::new();
        loop {
            let health = serde_json::json!({
                "healthy": true,
                "last_check": chrono::Utc::now().to_rfc3339(),
                "message": null
            });
            let resp = client
                .post(&health_url)
                .header("authorization", format!("Bearer {}", agent_key))
                .json(&health)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => info!("Healthcheck sent to gateway"),
                Ok(r) => error!("Failed to send healthcheck: {}", r.status()),
                Err(e) => error!("Failed to send healthcheck: {}", e),
            }
            sleep(Duration::from_secs(30)).await; // TODO: make interval configurable
        }
    });
}
