use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::net::SocketAddr;

#[derive(Parser)]
#[command(
    name = "logtape",
    version = "0.1.0",
    about = "Record and analyze HTTP traffic as structured events"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Tap {
        #[command(subcommand)]
        protocol: TapProtocol,
    },
    Lint {
        #[arg(help = "Input JSONL file")]
        input: String,
        #[arg(long, default_value_t = false)]
        strict: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Fmt {
        #[arg(long, value_name = "FORMAT")]
        to: String,
        #[arg(help = "Input file")]
        input: String,
    },
}

#[derive(Subcommand)]
pub enum TapProtocol {
    Http {
        #[arg(long)]
        listen: SocketAddr,
        #[arg(long)]
        upstream: String,
        #[arg(long, default_value = "-")]
        out: String,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        service_version: Option<String>,
        #[arg(long, default_value_t = 4096)]
        body_max: usize,
        #[arg(long, default_value = "authorization,cookie,set-cookie")]
        redact_headers: String,
        #[arg(long, default_value = "token,auth")]
        redact_query: String,
    },
}

pub fn parse_csv_set(csv: &str) -> HashSet<String> {
    csv.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}
