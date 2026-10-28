use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use super::workspace;

const TARGET: &str = "wasm32-wasip2";

pub(crate) struct BuildOptions {
    pub release: bool,
    pub locked: bool,
}

pub(crate) fn compile_workspace(directory: &Path, options: &BuildOptions) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .current_dir(directory)
        .args(["build", "--workspace", "--lib", "--target", TARGET])
        .arg("--manifest-path")
        .arg(directory.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(directory.join("target"));
    if options.release {
        command.arg("--release");
    }
    if options.locked {
        command.arg("--locked");
    }
    let status = command.status().context(
        "running Cargo; install the Rust toolchain and wasm32-wasip2 target (see README)",
    )?;
    if !status.success() {
        bail!("Cargo build failed ({status}); components were not published");
    }
    Ok(())
}

pub(crate) fn publish_outputs(
    directory: &Path,
    function_names: &[String],
    options: &BuildOptions,
) -> Result<Vec<PathBuf>> {
    let profile = if options.release { "release" } else { "debug" };
    let artifacts = directory.join("target").join(TARGET).join(profile);
    let components = directory.join(".wr/components");
    let outputs: Vec<_> = function_names
        .iter()
        .map(|name| {
            (
                artifacts.join(format!("{}.wasm", workspace::library_name(name))),
                components.join(format!("{name}.wasm")),
            )
        })
        .collect();
    for (source, _) in &outputs {
        if !source.is_file() {
            bail!(
                "Cargo did not produce expected component {}",
                source.display()
            );
        }
    }
    fs::create_dir_all(&components)
        .with_context(|| format!("creating component directory {}", components.display()))?;
    let mut published = Vec::new();
    for (source, destination) in outputs {
        fs::copy(&source, &destination).with_context(|| {
            format!("copying {} to {}", source.display(), destination.display())
        })?;
        published.push(destination);
    }
    Ok(published)
}
