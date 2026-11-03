use crate::deployment::{registry::PublicArtifacts, repository::Repositories};
use crate::{
    deployment::{Reconciler, Status, config::Source},
    hosting::host::Host,
};
use anyhow::Result;
use clap::Args;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

/// Reconcile public Git configuration and OCI releases in the foreground
#[derive(Args)]
pub(super) struct Start {
    #[arg(long, alias = "config_repo")]
    config_repo: String,
    #[arg(long, alias = "config_branch", default_value = "main")]
    config_branch: String,
    #[arg(long, default_value = "config.yaml")]
    config_path: String,
    #[arg(long, default_value = "30", value_parser = clap::value_parser!(u64).range(1..))]
    poll_interval: u64,
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
    /// Optional separate loopback listener for inspection and health checks
    #[arg(long)]
    admin_listen: Option<SocketAddr>,
}

impl Start {
    pub fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        let source = Source {
            repo: self.config_repo,
            branch: self.config_branch,
            path: self.config_path,
        };
        source.validate()?;
        if let Some(listen) = self.admin_listen {
            anyhow::ensure!(
                listen.ip().is_loopback(),
                "admin UI requires a loopback IP address"
            );
        }
        let directory = directory
            .or_else(|| std::env::var_os("WR_STATE_DIR").map(PathBuf::from))
            .ok_or_else(|| anyhow::anyhow!("wr start requires --state-dir or WR_STATE_DIR"))?;
        super::runtime()?.block_on(async {
            let shutdown = super::shutdown_signal()?;
            let host = Host::new(&directory, self.listen)?;
            let status = Arc::new(RwLock::new(Status::default()));
            let reconciler = Reconciler::new(
                host.directory.path.clone(),
                source,
                host.applications.clone(),
                status.clone(),
                Repositories::new(host.directory.path.join("repositories")),
                PublicArtifacts,
            )?;
            host.serve(
                self.admin_listen,
                status,
                |stopped| reconciler.run(Duration::from_secs(self.poll_interval), stopped),
                shutdown,
            )
            .await
        })
    }
}
