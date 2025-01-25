use anyhow::Result;
use ipnet::IpNet;

pub struct Target {
    pub prefix: IpNet,
    pub protocol: caracat::models::L4,
    pub min_ttl: u8,
    pub max_ttl: u8,
    pub n_flows: u64,
}

pub fn decode_target(payload: &str) -> Result<Target> {
    let parts: Vec<&str> = payload.split(',').collect();
    let prefix: IpNet = parts[0].parse()?;

    // Check if the prefix is IPv4 or IPv6
    let is_ipv4 = match prefix {
        IpNet::V4(_) => true,
        IpNet::V6(_) => false,
    };

    Ok(Target {
        prefix: parts[0].parse()?,
        protocol: {
            match parts[1].to_lowercase().as_str() {
                "icmp" => {
                    if is_ipv4 {
                        caracat::models::L4::ICMP
                    } else {
                        caracat::models::L4::ICMPv6
                    }
                }
                "udp" => caracat::models::L4::UDP,
                _ => {
                    return Err(anyhow::anyhow!("Invalid protocol: {}", parts[4]));
                }
            }
        },
        min_ttl: parts[2].parse()?,
        max_ttl: parts[3].parse()?,
        n_flows: parts[4].parse()?,
    })
}
