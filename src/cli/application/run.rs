use crate::{
    deployment::Status,
    hosting::host::{Host, until_stopped},
    project::package,
};
use anyhow::Result;
use clap::Args;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, RwLock},
};

/// Serve one local application's previously built components in the foreground
#[derive(Args)]
pub(crate) struct Run {
    /// Application directory or manifest file
    #[arg(value_name = "PATH")]
    path: PathBuf,
    #[arg(long, conflicts_with = "port")]
    listen: Option<SocketAddr>,
    /// Loopback port; zero selects an available port
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    admin_listen: Option<SocketAddr>,
}

impl Run {
    pub(crate) fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        let path = std::fs::canonicalize(&self.path)?;
        let root = if path.is_file() {
            path.parent().expect("manifest parent")
        } else {
            &path
        };
        let directory = directory
            .or_else(|| std::env::var_os("WR_STATE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| root.join(".wr-data"));
        let listen = self.listen.unwrap_or_else(|| {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port.unwrap_or(8080))
        });
        if let Some(address) = self.admin_listen {
            anyhow::ensure!(
                address.ip().is_loopback(),
                "admin UI requires a loopback IP address"
            );
        }
        crate::cli::runtime()?.block_on(async {
            let shutdown = crate::cli::shutdown_signal()?;
            let application = package::deployment(&path)?;
            let host = Host::new(&directory, listen)?;
            let name = application.name.clone();
            host.applications.deploy(application).await?;
            println!("Application: {name}");
            let status = Arc::new(RwLock::new(Status {
                ready: true,
                applications: [(name, Default::default())].into(),
                ..Status::default()
            }));
            host.serve(
                self.admin_listen,
                status,
                |stopped| async {
                    until_stopped(stopped).await;
                    Ok(())
                },
                shutdown,
            )
            .await
        })
    }
}
