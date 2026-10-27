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
        | Command::Clean { directory }
        | Command::Run { directory, .. } => directory,
    };
    let directory = absolute(directory)
        .with_context(|| format!("resolving application directory {}", directory.display()))?;
    match command {
        Command::Setup { dry_run, .. } => {
            let application = manifest::load(&directory)?;
            setup::setup(&directory, &application, dry_run)
        }
        Command::Build {
            release, locked, ..
        } => {
            let application = manifest::load(&directory)?;
            build::build(
                &directory,
                &application,
                &build::BuildOptions { release, locked },
            )
        }
        Command::Clean { .. } => setup::clean(&directory),
        Command::Run { listen, .. } => {
            let application = manifest::load(&directory)?;
            run::run(&directory, &application, listen)
        }
    }
}
