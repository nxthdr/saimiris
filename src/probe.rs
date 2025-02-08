use anyhow::Result;
use caracat::models::Probe;

pub fn encode_protocol(protocol: caracat::models::L4) -> String {
    match protocol {
        caracat::models::L4::ICMP => "icmp".to_string(),
        caracat::models::L4::ICMPv6 => "icmpv6".to_string(),
        caracat::models::L4::UDP => "udp".to_string(),
    }
}

pub fn decode_protocol(protocol: &str) -> Result<caracat::models::L4> {
    match protocol {
        "icmp" => Ok(caracat::models::L4::ICMP),
        "icmpv6" => Ok(caracat::models::L4::ICMPv6),
        "udp" => Ok(caracat::models::L4::UDP),
        _ => Err(anyhow::anyhow!("Invalid protocol: {}", protocol)),
    }
}

pub fn encode_probe(probe: &Probe) -> String {
    format!(
        "{},{},{},{},{}",
        probe.dst_addr,
        probe.src_port,
        probe.dst_port,
        probe.ttl,
        encode_protocol(probe.protocol),
    )
}

pub fn decode_probe(probe: &str) -> Result<Probe> {
    let fields: Vec<&str> = probe.split(',').collect();
    if fields.len() != 5 {
        return Err(anyhow::anyhow!("Invalid probe format: {}", probe));
    }

    Ok(Probe {
        dst_addr: fields[0].parse()?,
        src_port: fields[1].parse()?,
        dst_port: fields[2].parse()?,
        ttl: fields[3].parse()?,
        protocol: decode_protocol(fields[4])?,
    })
}
