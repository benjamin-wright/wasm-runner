use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
};

use anyhow::Result;

use crate::manifest;

pub(crate) fn setup(path: &Path, dry_run: bool) -> Result<()> {
    let application = manifest::load(path)?;
    let functions: BTreeMap<_, _> = application
        .functions
        .iter()
        .filter(|(_, function)| function.build_from_source)
        .map(|(name, function)| (name.clone(), &function.dependencies))
        .collect();
    let directory = path
        .parent()
        .expect("a successfully read manifest file has a parent directory");
    let files = wasm_runner_setup::generate(directory, &application.dependencies, &functions)?;

    if dry_run {
        return print_preview(&application.name, &files);
    }

    wasm_runner_setup::write_workspace(directory, &files)?;
    writeln!(
        io::stdout().lock(),
        "Workspace written to {}",
        directory.display()
    )?;
    Ok(())
}

fn print_preview(name: &str, files: &BTreeMap<String, String>) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(output, "Workspace preview for {name}")?;
    writeln!(
        output,
        "Paths are relative to the application manifest directory."
    )?;
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
