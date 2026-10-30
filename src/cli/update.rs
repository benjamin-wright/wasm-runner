use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Args;

use crate::services::{build, manifest, workspace};

/// Update application dependencies to the newest compatible versions without building
#[derive(Args)]
pub(super) struct Update {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,
}

impl Update {
    pub(super) fn execute(self) -> Result<()> {
        let directory = super::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        let functions = workspace::discover_functions(&directory)?;
        let files = workspace::generate(&directory, &application, &functions)?;
        workspace::write_workspace(&directory, &files)?;

        build::update_dependencies(&directory)?;
        writeln!(
            io::stdout().lock(),
            "Updated dependencies in {}",
            directory.join("Cargo.lock").display()
        )?;
        Ok(())
    }
}
