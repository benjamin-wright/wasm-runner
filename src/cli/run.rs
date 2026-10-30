use std::{
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
};

use crate::services::server::{self, Config};
use anyhow::Result;
use clap::Args;

use crate::services::{loader, manifest};

/// Serve an application's previously built HTTP components
#[derive(Args)]
pub(super) struct Run {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,

    /// Numeric IP address and port to listen on
    #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
}

impl Run {
    pub(super) fn execute(self) -> Result<()> {
        let directory = super::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        let handlers = loader::load_handlers(&application)?;
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(async {
                // Install signal listeners before announcing readiness.
                #[cfg(unix)]
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                #[cfg(unix)]
                let mut interrupt =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
                server::serve(
                    Config {
                        listen: self.listen,
                        ..Config::default()
                    },
                    handlers,
                    async {
                        #[cfg(unix)]
                        tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
                        #[cfg(not(unix))]
                        if let Err(error) = tokio::signal::ctrl_c().await {
                            eprintln!("signal listener failed: {error}");
                        }
                    },
                    move |address| {
                        println!("Application: {}", application.name);
                        println!("Listening on http://{address}");
                        if let Err(error) = io::stdout().flush() {
                            eprintln!("flushing readiness output: {error}");
                        }
                    },
                )
                .await
            })
    }
}
