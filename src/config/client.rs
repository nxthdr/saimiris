use anyhow::Result;
use std::path::PathBuf;

use crate::client::producer::MeasurementInfo;

#[derive(Debug)]
pub struct ClientConfig {
    pub measurement_infos: Vec<MeasurementInfo>,
    pub probes_file: Option<PathBuf>,
}

pub fn parse_and_validate_client_args(
    agents: &str,
    agent_src_ips: Option<String>,
    probes_file: Option<PathBuf>,
) -> Result<ClientConfig> {
    // Parse agents
    let agent_names = agents.split(',').map(String::from).collect::<Vec<String>>();

    // Parse agent source IPs if provided
    let agent_src_ips = match agent_src_ips {
        Some(src_ips_str) => {
            let parsed_ips: Vec<Option<String>> = src_ips_str
                .split(',')
                .map(str::trim)
                .map(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                })
                .collect();

            // Validate length
            if parsed_ips.len() != agent_names.len() {
                return Err(anyhow::anyhow!(
                    "Number of agent source IPs must match the number of agents"
                ));
            }

            parsed_ips
        }
        None => {
            vec![None; agent_names.len()]
        }
    };

    // Create MeasurementInfo structs
    let measurement_infos: Vec<MeasurementInfo> = agent_names
        .into_iter()
        .zip(agent_src_ips)
        .map(|(name, src_ip)| MeasurementInfo { name, src_ip })
        .collect();

    Ok(ClientConfig {
        measurement_infos,
        probes_file,
    })
}
