use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Args;

use crate::services::workspace;

/// Remove generated workspace files and build outputs, preserving user configuration
#[derive(Args)]
pub(super) struct Clean {
    /// Application directory to clean
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,
}

impl Clean {
    pub(super) fn execute(self) -> Result<()> {
        let directory = super::resolve_directory(&self.directory)?;
        workspace::clean_workspace(&directory)?;
        writeln!(
            io::stdout().lock(),
            "Workspace cleaned in {}",
            directory.display()
        )?;
        Ok(())
    }
}
