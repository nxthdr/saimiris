mod agent;
mod auth;
mod client;
mod config;
mod probe;
mod probe_capnp;
mod reply;
mod reply_capnp;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use metrics::describe_counter;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::io::{stdin, IsTerminal};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{error, trace};

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

        /// Optional comma-separated list of source IP addresses, one for each agent.
        /// If provided, the number of IPs must match the number of agents.
        #[arg(long)]
        agent_src_ips: Option<String>,
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

fn set_metrics(metrics_address: SocketAddr) {
    let prom_builder = PrometheusBuilder::new();
    prom_builder
        .with_http_listener(metrics_address)
        .install()
        .expect("Failed to install Prometheus metrics exporter");

    // Producer metrics
    metrics::describe_counter!(
        "risotto_kafka_messages_total",
        "Total number of Kafka messages produced"
    );

    // Receiver Metrics
    describe_counter!(
        "saimiris_receiver_received_valid_total",
        "Total number of valid replies received from the caracat receiver thread"
    );
    describe_counter!(
        "saimiris_receiver_received_invalid_total",
        "Total number of invalid replies received that failed the integrity check"
    );

    // Sender Metrics
    describe_counter!(
        "saimiris_sender_read_total",
        "Total number of probes read from the sender thread"
    );
    describe_counter!(
        "saimiris_sender_probes_sent_total",
        "Total number of probes sent by the sender thread"
    );
    describe_counter!(
        "saimiris_sender_failed_total",
        "Total number of errors encountered by the sender thread while sending probes"
    );
    describe_counter!(
        "saimiris_sender_filtered_total",
        "Total number of probes filtered by the sender thread (low/high TTL)"
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = App::parse();
    set_tracing(&cli.global_opts)?;

    match cli.command {
        Command::Agent { config } => {
            let app_config = app_config(&config).await?;
            trace!("{:?}", app_config);
            set_metrics(app_config.agent.metrics_address);
            match agent::handle(&app_config).await {
                Ok(_) => (),
                Err(e) => error!("Error: {}", e),
            }
        }
        Command::Client {
            config,
            agents,
            probes_file,
            agent_src_ips,
        } => {
            if probes_file.is_none() && stdin().is_terminal() {
                App::command().print_help().unwrap();
                ::std::process::exit(2);
            }

            let app_config = app_config(&config).await?;
            trace!("{:?}", app_config);
            match client::handle(&app_config, &agents, agent_src_ips, probes_file).await {
                Ok(_) => (),
                Err(e) => error!("Error: {}", e),
            }
        }
    }

    Ok(())
}
