use anyhow::Result;
use clap::Args;
use std::{net::SocketAddr, path::PathBuf};

/// Inspect the daemon through a local, read-only browser UI
#[derive(Args)]
pub(super) struct Admin {
    /// Loopback IP and port for the UI (port 0 selects an available port)
    #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:0")]
    listen: SocketAddr,
}

impl Admin {
    pub(super) fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        #[cfg(unix)]
        {
            use crate::daemon::process::Paths;
            use std::io::{self, Write};
            use tokio::signal::unix::{SignalKind, signal};

            anyhow::ensure!(
                self.listen.ip().is_loopback(),
                "admin UI requires a loopback IP address"
            );
            let paths = Paths::new(directory)?;
            super::runtime()?.block_on(async {
                let mut terminate = signal(SignalKind::terminate())?;
                let mut interrupt = signal(SignalKind::interrupt())?;
                crate::admin::serve(
                    self.listen,
                    paths.socket,
                    async {
                        tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
                    },
                    |address| {
                        println!("Admin UI listening on http://{address}");
                        if let Err(error) = io::stdout().flush() {
                            eprintln!("flushing readiness output: {error}");
                        }
                    },
                )
                .await
            })
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            anyhow::bail!("admin UI currently requires Unix")
        }
    }
}
