use std::{
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
};

use crate::hosting::{
    applications::Applications,
    server::{self, Config},
};
use anyhow::Result;
use clap::Args;

use crate::project::{manifest, package};

/// Serve an application's previously built HTTP components
#[derive(Args)]
pub(crate) struct Run {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,

    /// Numeric IP address and port to listen on
    #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
}

impl Run {
    pub(crate) fn execute(self) -> Result<()> {
        let directory = crate::cli::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        let handlers = package::load_handlers(&application)?;
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
                let config = Config {
                    listen: self.listen,
                    ..Config::default()
                };
                let storage = std::sync::Arc::new(crate::persistence::Storage::new(std::fs::canonicalize(&directory)?.join(".wr-data/kv")));
                let applications = Applications::with_storage(config.clone(), Some(storage.clone()))?;
                applications
                    .install(application.name.clone(), application.path_prefix, handlers, application.kv)
                    .await?;
                let diagnostics = applications.clone();
                let result = server::serve_applications(
                    config,
                    applications,
                    async move {
                        let signals = async {
                            #[cfg(unix)]
                            tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
                            #[cfg(not(unix))]
                            if let Err(error) = tokio::signal::ctrl_c().await {
                                eprintln!("signal listener failed: {error}");
                            }
                        };
                        let metrics = async {
                            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                            loop {
                                interval.tick().await;
                                if let Ok(info) = diagnostics.storage_info().await && info.opened {
                                    eprintln!("KV storage: {}", serde_json::to_string(&info).expect("storage metadata"));
                                }
                            }
                        };
                        tokio::select! {
                            () = signals => {},
                            () = metrics => {},
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
                .await;
                let synced = tokio::task::spawn_blocking(move || storage.sync()).await?;
                result.and(synced)
            })
    }
}
