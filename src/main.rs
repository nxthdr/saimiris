mod config;
mod consumer;
mod handler;
mod prober;
mod producer;

use anyhow::Result;
use chrono::Local;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use std::io::Write;

// use crate::consumer::consume;
use crate::config::{load_config, prober_config};
use crate::handler::handle;

#[derive(Debug, Parser)]
#[clap(name = "Osiris", version)]
pub struct App {
    #[clap(flatten)]
    global_opts: GlobalOpts,

    #[clap(subcommand)]
    command: Command,
}
#[derive(Debug, Subcommand)]
#[command(version, about, long_about = None)]
enum Command {
    Prober {
        /// Configuration file
        #[arg(short, long)]
        config: String,

        /// Target (eg., 2606:4700:4700::1111/128,1,32,1)
        #[arg(index = 1)]
        target: String,
    },
}

#[derive(Debug, Args)]
struct GlobalOpts {
    /// Verbosity level
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn set_logging(cli: &GlobalOpts) {
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
    let cli = App::parse();
    set_logging(&cli.global_opts);

    match cli.command {
        Command::Prober { config, target } => {
            let app_config = load_config(&config);
            let prober_config = prober_config(app_config);

            match handle(&prober_config, &target).await {
                Ok(_) => (),
                Err(e) => log::error!("Error: {}", e),
            }
        }
    }

    Ok(())
}
