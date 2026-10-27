use std::{
    io::{self, Write},
    net::SocketAddr,
    path::Path,
};

use crate::manifest::ApplicationDefinition;
use anyhow::Result;

pub(crate) fn run(
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
