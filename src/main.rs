mod consumer;
mod handler;
mod prober;

use anyhow::Result;
use chrono::Local;
use clap::Parser as CliParser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use std::io::Write;

use crate::consumer::consume;

#[derive(CliParser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Cli {
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

    let brokers = "localhost:9092";
    let group_id = "osiris";
    let topics = vec!["osiris"];

    consume(brokers, group_id, &topics).await;

    Ok(())
}
