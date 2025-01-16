mod consumer;
mod handler;
mod prober;
mod producer;

use anyhow::Result;
use chrono::Local;
use clap::Parser as CliParser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use std::io::Write;

// use crate::consumer::consume;
use crate::handler::handle;

#[derive(CliParser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Prober ID
    #[clap(long, default_value = "0")]
    prober_id: u16,

    /// Kafka brokers
    #[clap(long, default_value = "localhost:9092")]
    brokers: String,

    /// Kafka consumer topics (comma separated)
    #[clap(long, default_value = "osiris")]
    in_topics: String,

    /// Kafka consumer group ID
    #[clap(long, default_value = "osiris")]
    in_group_id: String,

    /// Kafka producer topic
    #[clap(long, default_value = "results")]
    out_topic: String,

    /// Target (eg., 2606:4700:4700::1111/128,1,32,1)
    #[arg(index = 1)]
    target: String,

    /// Verbosity level
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn set_logging(cli: &Cli) {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    set_logging(&cli);

    match handle(
        cli.prober_id,
        &cli.brokers,
        &cli.in_topics,
        &cli.in_group_id,
        &cli.out_topic,
        &cli.target,
    )
    .await
    {
        Ok(_) => (),
        Err(e) => log::error!("Error: {}", e),
    }

    Ok(())
}
