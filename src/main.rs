mod prober;

use anyhow::Result;
use caracat::high_level::Config;
use caracat::models::Probe;
use chrono::Local;
use clap::Parser as CliParser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use std::{io::Write, time::Duration};

use crate::prober::probe;

#[derive(CliParser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct CLI {
    /// Verbosity level
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn set_logging(cli: &CLI) {
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter_module("osiris", cli.verbose.log_level_filter())
        .init();
}

fn main() -> Result<()> {
    let cli = CLI::parse();
    set_logging(&cli);

    let config = Config {
        allowed_prefixes_file: None,
        blocked_prefixes_file: None,
        batch_size: 128,
        dry_run: false,
        extra_string: None,
        min_ttl: None,
        max_ttl: None,
        integrity_check: true,
        instance_id: 0,
        interface: caracat::utilities::get_default_interface(),
        max_probes: None,
        packets: 1,
        probing_rate: 100,
        rate_limiting_method: caracat::rate_limiter::RateLimitingMethod::Auto,
        receiver_wait_time: Duration::new(3, 0),
    };

    let addr = "2606:4700:4700::1111".parse().unwrap();

    let min_ttl = 1;
    let max_ttl = 32;

    let mut probes = vec![];
    for i in min_ttl..=max_ttl {
        probes.push(Probe {
            dst_addr: addr,
            src_port: 24000,
            dst_port: 33434,
            ttl: i,
            protocol: caracat::models::L4::ICMPv6,
        });
    }

    let _ = probe(config, probes.into_iter(), Some(String::from("./test.csv")));

    Ok(())
}
