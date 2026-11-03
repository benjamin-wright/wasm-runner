use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Package built components and migrations as OCI layouts for CI to publish
#[derive(Args)]
pub(crate) struct Package {
    path: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    source_commit: String,
}

impl Package {
    pub fn execute(self) -> Result<()> {
        crate::deployment::package::write(&self.path, &self.output, self.source_commit)
    }
}
