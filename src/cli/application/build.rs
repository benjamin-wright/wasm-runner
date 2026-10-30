use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Args;

use crate::project::{build, manifest, workspace};

/// Build HTTP function components from functions/*.rs
#[derive(Args)]
pub(crate) struct Build {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,

    /// Build optimized components
    #[arg(long)]
    release: bool,

    /// Allow Cargo to create or change Cargo.lock (builds are locked by default)
    #[arg(long)]
    unlocked: bool,
}

impl Build {
    pub(crate) fn execute(self) -> Result<()> {
        let directory = crate::cli::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        let functions = workspace::discover_functions(&directory)?;
        let files = workspace::generate(&directory, &application, &functions)?;
        workspace::write_workspace(&directory, &files)?;

        let options = build::BuildOptions {
            release: self.release,
            locked: !self.unlocked,
        };
        build::compile_workspace(&directory, &options)?;
        let published = build::publish_outputs(&directory, &functions, &options)?;

        let mut output = io::stdout().lock();
        for path in published {
            writeln!(output, "Built {}", path.display())?;
        }
        Ok(())
    }
}
