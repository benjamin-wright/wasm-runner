use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
};

use anyhow::Result;

use crate::manifest::ApplicationDefinition;

pub(crate) fn setup(
    directory: &Path,
    application: &ApplicationDefinition,
    dry_run: bool,
) -> Result<()> {
    let functions = wasm_runner_setup::discover_functions(directory)?;
    let files = workspace_files(directory, application, &functions)?;

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

pub(crate) fn workspace_files(
    directory: &Path,
    application: &ApplicationDefinition,
    function_names: &[String],
) -> Result<BTreeMap<String, String>> {
    let empty = BTreeMap::new();
    let functions = function_names
        .iter()
        .map(|name| {
            let dependencies = application
                .functions
                .get(name)
                .map_or(&empty, |function| &function.dependencies);
            (name.clone(), dependencies)
        })
        .collect();
    wasm_runner_setup::generate(directory, &application.dependencies, &functions)
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
