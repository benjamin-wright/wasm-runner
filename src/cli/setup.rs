use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Args;

use crate::services::{manifest, workspace};

/// Generate a Cargo workspace for source functions
#[derive(Args)]
pub(super) struct Setup {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,

    /// Print the generated file paths and contents without writing files
    #[arg(long)]
    dry_run: bool,
}

impl Setup {
    pub(super) fn execute(self) -> Result<()> {
        let directory = super::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        let functions = workspace::discover_functions(&directory)?;
        let files = workspace::generate(&directory, &application, &functions)?;

        if self.dry_run {
            return print_preview(&application.name, &files);
        }

        workspace::write_workspace(&directory, &files)?;
        writeln!(
            io::stdout().lock(),
            "Workspace written to {}",
            directory.display()
        )?;
        Ok(())
    }
}

fn print_preview(name: &str, files: &BTreeMap<String, String>) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(output, "Workspace preview for {name}")?;
    writeln!(output, "Paths are relative to the application directory.")?;
    writeln!(
        output,
        "Preview only; no files are written and Cargo is not invoked.\n"
    )?;
    for name in files.keys() {
        writeln!(output, "{name}")?;
    }
    for (name, contents) in files {
        writeln!(output, "\n=== {name} ===\n{contents}")?;
    }
    Ok(())
}
