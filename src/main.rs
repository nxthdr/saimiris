mod agent;
mod auth;
mod client;
mod config;
mod probe;
mod utils;

use anyhow::Result;
use chrono::Local;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use log::error;
use std::io::Write;

use crate::config::app_config;

#[derive(Debug, Parser)]
#[clap(name = "Saimiris", version)]
pub struct App {
    #[clap(flatten)]
    global_opts: GlobalOpts,

    #[clap(subcommand)]
    command: Command,
}
#[derive(Debug, Subcommand)]
#[command(version, about, long_about = None)]
enum Command {
    Agent {
        /// Configuration file
        #[arg(short, long)]
        config: String,
    },

    Client {
        /// Configuration file
        #[arg(short, long)]
        config: String,

        /// Agent IDs (comma separated)
        #[arg(index = 1)]
        agents: String,

        /// Target (eg., 2606:4700:4700::1111/128,icmp,1,32,1)
        #[arg(index = 2)]
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
        .filter_module("saimiris", cli.verbose.log_level_filter())
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = App::parse();
    set_logging(&cli.global_opts);

    match cli.command {
        Command::Agent { config } => {
            let app_config = app_config(&config);
            match agent::handle(&app_config).await {
                Ok(_) => (),
                Err(e) => error!("Error: {}", e),
            }
        }
        Command::Client {
            config,
            agents,
            target,
        } => {
            let app_config = app_config(&config);
            match client::handle(&app_config, &agents, &target).await {
                Ok(_) => (),
                Err(e) => error!("Error: {}", e),
            }
        }
    }

    Ok(())
}
