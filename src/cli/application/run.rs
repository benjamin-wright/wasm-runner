use std::{
    future::Future,
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use clap::Args;

use crate::{
    hosting::{
        applications::Applications,
        server::{self, Config},
    },
    persistence::Storage,
    project::package,
};

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
        crate::cli::runtime()?.block_on(self.serve(directory))
    }

    async fn serve(self, directory: PathBuf) -> Result<()> {
        let application = package::deployment(&directory)?;
        let shutdown = shutdown_signal()?;
        let config = Config {
            listen: self.listen,
            ..Config::default()
        };
        let storage = Arc::new(Storage::new(
            std::fs::canonicalize(&directory)?.join(".wr-data/kv"),
        ));
        let sql = Arc::new(crate::persistence::sql::SqlStorage::new(
            std::fs::canonicalize(&directory)?.join(".wr-data/sql"),
        ));
        let applications =
            Applications::with_persistence(config.clone(), Some(storage.clone()), Some(sql))?;
        applications
            .install(
                application.name.clone(),
                application.path_prefix,
                application.handlers,
                application.kv,
                application.sql,
                application.migrations,
            )
            .await?;
        let result = server::serve_applications(
            config,
            applications.clone(),
            async {
                tokio::select! {
                    () = shutdown => {},
                    () = report_storage(&applications) => {},
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
    }
}

// Register Unix listeners before preparation and readiness, not on first poll.
fn shutdown_signal() -> Result<impl Future<Output = ()>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        Ok(async move {
            tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
        })
    }
    #[cfg(not(unix))]
    {
        Ok(async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("signal listener failed: {error}");
            }
        })
    }
}

async fn report_storage(applications: &Applications) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        if let Ok(info) = applications.storage_info().await
            && info.opened
        {
            eprintln!(
                "KV storage: {}",
                serde_json::to_string(&info).expect("storage metadata")
            );
        }
    }
}
