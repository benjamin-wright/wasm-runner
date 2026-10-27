mod build;
mod cli;
mod manifest;
mod run;
mod setup;

use std::{path::absolute, process::ExitCode};

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match execute(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn execute(command: Command) -> Result<()> {
    let directory = match &command {
        Command::Setup { directory, .. }
        | Command::Build { directory, .. }
        | Command::Run { directory, .. } => directory,
    };
    let directory = absolute(directory)
        .with_context(|| format!("resolving application directory {}", directory.display()))?;
    let application = manifest::load(&directory)?;

    match command {
        Command::Setup { dry_run, .. } => setup::setup(&directory, &application, dry_run),
        Command::Build {
            release, locked, ..
        } => build::build(
            &directory,
            &application,
            &build::BuildOptions { release, locked },
        ),
        Command::Run { listen, .. } => run::run(&directory, &application, listen),
    }
}
