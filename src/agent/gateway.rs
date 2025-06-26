use anyhow::Result;
use reqwest::Client;
use tokio::task::spawn;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use crate::config::CaracatConfig;

pub async fn register_agent(
    client: &Client,
    gateway_url: &str,
    agent_id: &str,
    agent_key: &str,
    agent_secret: &str,
) -> Result<()> {
    let register_url = format!(
        "{}/agent-api/agent/register",
        gateway_url.trim_end_matches('/')
    );
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
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to send registration request to {}: {}",
                register_url,
                e
            )
        })?;
    if resp.status().is_success() {
        info!("Successfully registered agent with gateway");
    } else if resp.status() == reqwest::StatusCode::CONFLICT {
        info!("Agent already registered at gateway");
    } else {
        error!("Failed to register agent: {}", resp.status());
        return Err(anyhow::anyhow!(
            "Failed to register agent: {}",
            resp.status()
        ));
    }
    Ok(())
}

pub async fn send_agent_config(
    client: &Client,
    gateway_url: &str,
    agent_id: &str,
    agent_key: &str,
    caracat_configs: &[CaracatConfig],
) -> Result<()> {
    let config_url = format!(
        "{}/agent-api/agent/{}/config",
        gateway_url.trim_end_matches('/'),
        agent_id
    );
    let resp = client
        .post(&config_url)
        .header("authorization", format!("Bearer {}", agent_key))
        .json(caracat_configs)
        .send()
        .await?;
    if resp.status().is_success() {
        info!("Agent config sent to gateway");
    } else {
        error!("Failed to send agent config: {}", resp.status());
    }
    Ok(())
}

pub fn spawn_healthcheck_loop(
    gateway_url: String,
    agent_id: String,
    agent_key: String,
    caracat_configs: Vec<CaracatConfig>,
) {
    let agent_secret = agent_key.clone(); // We're reusing the agent key as the secret for re-registration
    let base_url = gateway_url.trim_end_matches('/').to_string();
    let agent_url = format!("{}/agent-api/agent/{}", base_url, agent_id);
    let config_url = format!("{}/agent-api/agent/{}/config", base_url, agent_id);
    let health_url = format!("{}/agent-api/agent/{}/health", base_url, agent_id);
    let register_url = format!("{}/agent-api/agent/register", base_url);

    spawn(async move {
        let client = Client::new();
        loop {
            // Step 1: Check if agent exists (GET /agent/{id})
            let mut needs_registration = false;
            let mut needs_config_update = false;

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
                    // Skip this iteration if we can't connect to the gateway
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            }

            // Step 2: If agent exists, check if config exists (GET /agent/{id}/config)
            if !needs_registration {
                debug!("Checking if agent config exists on gateway");
                match client
                    .get(&config_url)
                    .header("authorization", format!("Bearer {}", agent_key))
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        debug!("Agent config exists on gateway");
                    }
                    Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
                        debug!("Agent config does not exist on gateway, will update");
                        needs_config_update = true;
                    }
                    Ok(r) => {
                        warn!(
                            "Unexpected status when checking agent config: {}",
                            r.status()
                        );
                        needs_config_update = true; // Try updating config just in case
                    }
                    Err(e) => {
                        error!("Failed to check if agent config exists: {}", e);
                    }
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
                        needs_config_update = true; // Always update config after registration
                    }
                    Ok(r) if r.status() == reqwest::StatusCode::CONFLICT => {
                        debug!("Agent already registered at gateway (unexpected conflict)");
                    }
                    Ok(r) => {
                        error!("Failed to register agent: {}", r.status());
                    }
                    Err(e) => {
                        error!("Failed to register agent: {}", e);
                    }
                }
            }

            // Step 4: Update agent config if needed
            if needs_config_update {
                debug!("Updating agent configuration on gateway");
                match client
                    .post(&config_url)
                    .header("authorization", format!("Bearer {}", agent_key))
                    .json(&caracat_configs)
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        debug!("Successfully sent agent config to gateway");
                    }
                    Ok(r) => {
                        error!("Failed to send agent config: {}", r.status());
                    }
                    Err(e) => {
                        error!("Failed to send agent config: {}", e);
                    }
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
                }
                Err(e) => {
                    error!("Failed to send healthcheck: {}", e);
                }
            }

            sleep(Duration::from_secs(30)).await; // TODO: make interval configurable
        }
    });
}
