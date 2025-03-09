mod agent;
mod auth;
mod client;
mod config;
mod probe;
mod utils;

use anyhow::Result;
use chrono::Local;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use env_logger::Builder;
use log::error;
use std::io::{stdin, IsTerminal, Write};
use std::path::PathBuf;

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

        /// Probes file (read stdin if not provided)
        #[arg(short, long)]
        probes_file: Option<PathBuf>,
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
            probes_file,
        } => {
            if stdin().is_terminal() {
                App::command().print_help().unwrap();
                ::std::process::exit(2);
            }

            let app_config = app_config(&config);
            match client::handle(&app_config, &agents, probes_file).await {
                Ok(_) => (),
                Err(e) => error!("Error: {}", e),
            }
        }
    }

    Ok(())
}
