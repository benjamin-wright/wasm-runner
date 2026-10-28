use std::{
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Args;

use crate::services::manifest::{self, ApplicationDefinition};

/// Load an application YAML manifest and print metadata (serving not yet implemented)
#[derive(Args)]
pub(super) struct Run {
    /// Application directory containing application.yaml
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,

    /// Numeric IP address and port to listen on
    #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
}

impl Run {
    pub(super) fn execute(self) -> Result<()> {
        let directory = super::resolve_directory(&self.directory)?;
        let application = manifest::load(&directory)?;
        print_metadata(&directory, &application, self.listen)
    }
}

fn print_metadata(
    directory: &Path,
    application: &ApplicationDefinition,
    listen: SocketAddr,
) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(output, "Application: {}", application.name)?;
    writeln!(
        output,
        "Manifest: {}",
        directory.join("application.yaml").display()
    )?;
    writeln!(output, "Configured listener: {listen}")?;
    writeln!(output, "Functions: {}", application.functions.len())?;
    let mut functions: Vec<_> = application.functions.iter().collect();
    functions.sort_unstable_by_key(|(name, _)| *name);
    for (name, function) in functions {
        writeln!(
            output,
            "{:<7} {} -> {name} ({})",
            function.http.method,
            function.http.path,
            function.module.display()
        )?;
    }
    writeln!(
        output,
        "Metadata only; component execution and HTTP serving are not implemented yet."
    )?;
    Ok(())
}
