use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wr", version, about)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Generate a Cargo workspace for source functions
    Setup {
        /// Application directory containing application.yaml
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// Print the generated file paths and contents without writing files
        #[arg(long)]
        dry_run: bool,
    },
    /// Build HTTP function components from functions/*.rs
    Build {
        /// Application directory containing application.yaml
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// Build optimized components
        #[arg(long)]
        release: bool,

        /// Require an up-to-date Cargo.lock without changing it
        #[arg(long)]
        locked: bool,
    },
    /// Load an application YAML manifest and print metadata (serving not yet implemented)
    Run {
        /// Application directory containing application.yaml
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// Numeric IP address and port to listen on
        #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
    },
}
