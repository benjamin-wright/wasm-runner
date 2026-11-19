use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Package built components and migrations as one OCI layout for CI to publish
#[derive(Args)]
pub(crate) struct Package {
    path: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

impl Package {
    pub fn execute(self) -> Result<()> {
        crate::deployment::package::write(&self.path, &self.output)
    }
}
