use anyhow::{anyhow, Context, Result};
use capnp::message::{Builder, ReaderOptions};
use capnp::{serialize, ErrorKind};
use caracat::models::Probe;
use std::convert::TryInto;
use std::io::Cursor;
use std::net::{IpAddr, Ipv6Addr};

use crate::probe_capnp::probe;

pub fn serialize_ip_addr(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(addr) => addr.to_ipv6_mapped().octets().to_vec(),
        IpAddr::V6(addr) => addr.octets().to_vec(),
    }
}

pub fn serialize_protocol(protocol: caracat::models::L4) -> probe::Protocol {
    match protocol {
        // caracat::models::L4::TCP => probe::Protocol::Tcp, // Not supported by caracat yet
        caracat::models::L4::UDP => probe::Protocol::Udp,
        caracat::models::L4::ICMP => probe::Protocol::Icmp,
        caracat::models::L4::ICMPv6 => probe::Protocol::Icmpv6,
    }
}

pub fn serialize_probe(probe: &Probe) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut p = message.init_root::<probe::Builder>();
        p.set_dst_addr(&serialize_ip_addr(probe.dst_addr));
        p.set_src_port(probe.src_port);
        p.set_dst_port(probe.dst_port);
        p.set_ttl(probe.ttl);
        p.set_protocol(serialize_protocol(probe.protocol));
    }

    serialize::write_message_to_words(&message)
}

fn deserialize_protocol(protocol: probe::Protocol) -> Result<caracat::models::L4> {
    match protocol {
        probe::Protocol::Udp => Ok(caracat::models::L4::UDP),
        probe::Protocol::Icmp => Ok(caracat::models::L4::ICMP),
        probe::Protocol::Icmpv6 => Ok(caracat::models::L4::ICMPv6),
        probe::Protocol::Tcp => Err(anyhow!(
            "TCP protocol not currently supported by caracat model used here"
        )), // Or handle TCP if needed
    }
}

fn deserialize_ip_addr(data: &[u8]) -> Result<IpAddr> {
    let bytes: [u8; 16] = data.try_into().map_err(|_| {
        anyhow!(
            "Invalid IP address byte length: expected 16, got {}",
            data.len()
        )
    })?;
    let ipv6_addr = Ipv6Addr::from(bytes);
    if let Some(ipv4_addr) = ipv6_addr.to_ipv4_mapped() {
        Ok(IpAddr::V4(ipv4_addr))
    } else {
        Ok(IpAddr::V6(ipv6_addr))
    }
}

fn deserialize_single_probe_from_reader(p: probe::Reader) -> Result<Probe> {
    let dst_addr_bytes = p.get_dst_addr().context("Failed to get dst_addr")?;
    let dst_addr = deserialize_ip_addr(dst_addr_bytes)?;

    let src_port = p.get_src_port();
    let dst_port = p.get_dst_port();
    let ttl = p.get_ttl();

    let capnp_protocol = p.get_protocol().context("Failed to get protocol")?;
    let protocol = deserialize_protocol(capnp_protocol)?;

    Ok(Probe {
        dst_addr,
        src_port,
        dst_port,
        ttl,
        protocol,
    })
}

#[allow(dead_code)]
pub fn deserialize_probe(probe_bytes: Vec<u8>) -> Result<Probe> {
    let mut cursor = Cursor::new(probe_bytes);
    let message_reader = serialize::read_message(&mut cursor, ReaderOptions::new())
        .context("Failed to read single capnp message")?;
    let p = message_reader
        .get_root::<probe::Reader>()
        .context("Failed to get probe root reader for single message")?;
    deserialize_single_probe_from_reader(p)
}

pub fn deserialize_probes(probes_bytes: Vec<u8>) -> Result<Vec<Probe>> {
    let mut probes = Vec::new();
    let mut cursor = Cursor::new(probes_bytes);

    loop {
        match serialize::read_message(&mut cursor, ReaderOptions::new()) {
            Ok(message_reader) => {
                let p = message_reader
                    .get_root::<probe::Reader>()
                    .context("Failed to get probe root reader in stream")?;
                let probe = deserialize_single_probe_from_reader(p)
                    .context("Failed to deserialize probe from reader in stream")?;
                probes.push(probe);
            }
            Err(e) => {
                if e.kind == ErrorKind::PrematureEndOfFile {
                    // Reached end of stream after reading complete messages
                    break;
                }

                return Err(e).context("Failed to read capnp message from stream");
            }
        }
        // Check if cursor is at the end to prevent infinite loops on zero-byte reads (unlikely with capnp)
        if cursor.position() as usize == cursor.get_ref().len() {
            break;
        }
    }

    Ok(probes)
}
