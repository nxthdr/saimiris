use reqwest::Client;
use tokio::task::spawn;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, warn};

use crate::config::CaracatConfig;

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
        debug!("Starting healthcheck loop for agent {} with gateway {}", agent_id, base_url);
        let client = Client::new();
        
        // Add initial delay to allow gateway to start up
        debug!("Waiting 5 seconds before first gateway connection attempt...");
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
