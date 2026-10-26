mod cli;
mod manifest;
mod run;
mod setup;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Setup { manifest, dry_run } => setup::setup(&manifest, dry_run),
        Command::Run { manifest, listen } => run::run(&manifest, listen),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
