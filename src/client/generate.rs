use anyhow::Result;
use caracat::models::Probe;
use ipnet::IpNet;
use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::client::target::Target;

pub fn generate_probes(target: &Target) -> Result<Vec<Probe>> {
    // TODO: We should pass an iterator instead of a vector.
    let mut probes = vec![];

    // First start by dividing the prefix into /24s or /64s, if necessary.
    let subnets = match target.prefix {
        IpNet::V4(_) => {
            let prefix_len = target.prefix.prefix_len();
            let target_len = if prefix_len > 24 { prefix_len } else { 24 };
            target.prefix.subnets(target_len)
        }
        IpNet::V6(_) => {
            let prefix_len = target.prefix.prefix_len();
            let target_len = if prefix_len > 64 { prefix_len } else { 64 };
            target.prefix.subnets(target_len)
        }
    }?;

    // Iterate over the subnets and generate the probes.
    for subnet in subnets {
        // Right now the probe generation is simplistic, we just iterate over the hosts.
        // If we need more flows than hosts, we will we explicitely fail.
        // TODO: implement mapper-like generator such as the ones in diamond-miner.
        // https://github.com/dioptra-io/diamond-miner/blob/main/diamond_miner/mappers.py
        let mut prefix_hosts = subnet.hosts();
        if target.n_flows > prefix_hosts.count().try_into()? {
            return Err(anyhow::anyhow!("Not enough hosts in the prefix"));
        }

        for _ in 0..target.n_flows {
            let dst_addr = prefix_hosts.next().unwrap();

            // Randomize the probes order within a flow.
            // In YARRP we randomize the probes over the entire probing space.
            // This is of course a very big simplication, but it's not that silly.
            // The rational is to avoid results errors due to path changes.
            // So, for now, probes belonging to the same traceroute flow will be sent close in time.
            // TODO: is this shuffle fast?
            let mut ttls: Vec<u8> = (target.min_ttl..target.max_ttl).collect();
            ttls.shuffle(&mut thread_rng());

            for i in ttls {
                probes.push(Probe {
                    dst_addr,
                    src_port: 24000,
                    dst_port: 33434,
                    ttl: i,
                    protocol: target.protocol.clone(),
                });
            }
        }
    }

    Ok(probes)
}
