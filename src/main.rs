mod agent;
mod auth;
mod client;
mod config;
mod probe;
mod probe_capnp;
mod reply;
mod reply_capnp;
mod utils;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::io::{stdin, IsTerminal};
use std::path::PathBuf;
use tracing::error;

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

        /// Probes file (read stdin if not provided)
        #[arg(short, long)]
        probes_file: Option<PathBuf>,

        /// Agent IDs (comma separated)
        #[arg(index = 1)]
        agents: String,
    },
}

#[derive(Debug, Args)]
struct GlobalOpts {
    /// Verbosity level
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn set_tracing(cli: &GlobalOpts) -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_max_level(cli.verbose)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = App::parse();
    set_tracing(&cli.global_opts)?;

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
            if probes_file.is_none() && stdin().is_terminal() {
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
