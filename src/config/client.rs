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
    probes_file: Option<PathBuf>,
) -> Result<ClientConfig> {
    let agents = agents.trim();
    if agents.is_empty() {
        return Err(anyhow::anyhow!("At least one agent must be specified"));
    }

    // Parse agents in format: agent1:ip1,agent2:ip2
    // Handle IPv6 addresses in brackets: agent1:[2001:db8::1]
    let measurement_infos: Vec<MeasurementInfo> = agents
        .split(',')
        .map(|agent_spec| {
            let agent_spec = agent_spec.trim();
            if agent_spec.is_empty() {
                return Err(anyhow::anyhow!("Empty agent specification provided"));
            }

            // For IPv6 addresses in brackets, handle them specially
            let (agent_name, ip_str) = if let Some(bracket_start) = agent_spec.find('[') {
                let agent_name = agent_spec[..bracket_start].trim_end_matches(':');
                if let Some(bracket_end) = agent_spec.find(']') {
                    let ip_str = &agent_spec[bracket_start + 1..bracket_end];
                    (agent_name, ip_str)
                } else {
                    return Err(anyhow::anyhow!(
                        "Invalid agent specification '{}'. IPv6 addresses must be enclosed in brackets: 'agent_name:[ipv6_address]'",
                        agent_spec
                    ));
                }
            } else {
                // Regular IPv4 format
                let parts: Vec<&str> = agent_spec.split(':').collect();
                if parts.len() != 2 {
                    return Err(anyhow::anyhow!(
                        "Invalid agent specification '{}'. Expected format: 'agent_name:ip_address' or 'agent_name:[ipv6_address]'",
                        agent_spec
                    ));
                }
                (parts[0].trim(), parts[1].trim())
            };

            if agent_name.is_empty() {
                return Err(anyhow::anyhow!(
                    "Empty agent name in specification '{}'",
                    agent_spec
                ));
            }

            if ip_str.is_empty() {
                return Err(anyhow::anyhow!(
                    "Empty IP address in specification '{}'",
                    agent_spec
                ));
            }

            // Validate IP address format
            ip_str.parse::<std::net::IpAddr>().map_err(|_| {
                anyhow::anyhow!("Invalid IP address format '{}' in specification '{}'", ip_str, agent_spec)
            })?;

            Ok(MeasurementInfo {
                name: agent_name.to_string(),
                src_ip: Some(ip_str.to_string()),
                // Default measurement tracking value - can be overridden later
                measurement_id: None,
            })
        })
        .collect::<Result<Vec<MeasurementInfo>>>()?;

    if measurement_infos.is_empty() {
        return Err(anyhow::anyhow!("At least one agent must be specified"));
    }

    Ok(ClientConfig {
        measurement_infos,
        probes_file,
    })
}

impl ClientConfig {
    /// Set measurement tracking information for all agents in this configuration
    pub fn with_measurement_tracking(mut self, measurement_id: Option<String>) -> Self {
        for agent in &mut self.measurement_infos {
            agent.measurement_id = measurement_id.clone();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_format_agent_ip_pairs() {
        let result = parse_and_validate_client_args("agent1:192.168.1.1,agent2:10.0.0.1", None);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.measurement_infos.len(), 2);
        assert_eq!(config.measurement_infos[0].name, "agent1");
        assert_eq!(
            config.measurement_infos[0].src_ip,
            Some("192.168.1.1".to_string())
        );
        assert_eq!(config.measurement_infos[1].name, "agent2");
        assert_eq!(
            config.measurement_infos[1].src_ip,
            Some("10.0.0.1".to_string())
        );
    }

    #[test]
    fn test_new_format_with_ipv6() {
        let result =
            parse_and_validate_client_args("agent1:[2001:db8::1],agent2:192.168.1.1", None);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.measurement_infos.len(), 2);
        assert_eq!(
            config.measurement_infos[0].src_ip,
            Some("2001:db8::1".to_string())
        );
        assert_eq!(
            config.measurement_infos[1].src_ip,
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_invalid_ip_in_new_format() {
        let result = parse_and_validate_client_args("agent1:invalid_ip,agent2:192.168.1.1", None);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid IP address format"));
    }

    #[test]
    fn test_malformed_agent_spec() {
        let result =
            parse_and_validate_client_args("agent1:192.168.1.1:extra,agent2:10.0.0.1", None);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid agent specification"));
    }

    #[test]
    fn test_empty_agent_name() {
        let result = parse_and_validate_client_args(":192.168.1.1,agent2:10.0.0.1", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty agent name"));
    }

    #[test]
    fn test_empty_agents() {
        let result = parse_and_validate_client_args("", None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("At least one agent must be specified"));
    }

    #[test]
    fn test_ipv6_without_brackets_error() {
        let result = parse_and_validate_client_args("agent1:2001:db8::1", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Expected format"));
    }

    #[test]
    fn test_ipv6_malformed_brackets() {
        let result = parse_and_validate_client_args("agent1:[2001:db8::1", None);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("IPv6 addresses must be enclosed in brackets"));
    }

    #[test]
    fn test_mixed_ipv4_ipv6_new_format() {
        let result = parse_and_validate_client_args(
            "agent1:192.168.1.1,agent2:[2001:db8::1],agent3:10.0.0.1",
            None,
        );

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.measurement_infos.len(), 3);
        assert_eq!(
            config.measurement_infos[0].src_ip,
            Some("192.168.1.1".to_string())
        );
        assert_eq!(
            config.measurement_infos[1].src_ip,
            Some("2001:db8::1".to_string())
        );
        assert_eq!(
            config.measurement_infos[2].src_ip,
            Some("10.0.0.1".to_string())
        );
    }

    #[test]
    fn test_empty_ip_address() {
        let result = parse_and_validate_client_args("agent1:,agent2:192.168.1.1", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty IP address"));
    }

    #[test]
    fn test_missing_colon_separator() {
        let result = parse_and_validate_client_args("agent1192.168.1.1,agent2:10.0.0.1", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Expected format"));
    }

    #[test]
    fn test_single_agent() {
        let result = parse_and_validate_client_args("agent1:192.168.1.1", None);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.measurement_infos.len(), 1);
        assert_eq!(config.measurement_infos[0].name, "agent1");
        assert_eq!(
            config.measurement_infos[0].src_ip,
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_whitespace_handling() {
        let result =
            parse_and_validate_client_args(" agent1 : 192.168.1.1 , agent2 : 10.0.0.1 ", None);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.measurement_infos.len(), 2);
        assert_eq!(config.measurement_infos[0].name, "agent1");
        assert_eq!(
            config.measurement_infos[0].src_ip,
            Some("192.168.1.1".to_string())
        );
        assert_eq!(config.measurement_infos[1].name, "agent2");
        assert_eq!(
            config.measurement_infos[1].src_ip,
            Some("10.0.0.1".to_string())
        );
    }
}
