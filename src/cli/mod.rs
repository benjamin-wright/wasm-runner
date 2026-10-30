mod build;
mod clean;
mod run;
mod setup;

use std::path::{Path, PathBuf, absolute};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wr", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Setup(setup::Setup),
    Build(build::Build),
    Clean(clean::Clean),
    Run(run::Run),
}

pub fn execute() -> Result<()> {
    match Cli::parse().command {
        Command::Setup(command) => command.execute(),
        Command::Build(command) => command.execute(),
        Command::Clean(command) => command.execute(),
        Command::Run(command) => command.execute(),
    }
}

fn resolve_directory(directory: &Path) -> Result<PathBuf> {
    absolute(directory)
        .with_context(|| format!("resolving application directory {}", directory.display()))
}
