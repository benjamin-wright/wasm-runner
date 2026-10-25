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
    /// Run an application from a YAML manifest (not yet implemented)
    Run {
        /// Path to the application manifest
        #[arg(value_name = "MANIFEST")]
        manifest: PathBuf,

        /// Numeric IP address and port to listen on
        #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
    },
}
