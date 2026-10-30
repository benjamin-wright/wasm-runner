mod application;
mod apps;
mod daemon;
use application::{build, clean, deploy, run, setup, update};

use std::path::{Path, PathBuf, absolute};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wr", version, about)]
struct Cli {
    /// Private daemon state directory (defaults to WR_STATE_DIR or ~/.wr/daemon)
    #[arg(long, global = true, value_name = "DIRECTORY")]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Setup(setup::Setup),
    Build(build::Build),
    Update(update::Update),
    Clean(clean::Clean),
    Run(run::Run),
    Deploy(deploy::Deploy),
    Daemon(daemon::Daemon),
    Apps(apps::Apps),
    Remove(apps::Remove),
}

pub fn execute() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Setup(command) => command.execute(),
        Command::Build(command) => command.execute(),
        Command::Update(command) => command.execute(),
        Command::Clean(command) => command.execute(),
        Command::Run(command) => command.execute(),
        Command::Deploy(command) => command.execute(cli.state_dir),
        Command::Daemon(command) => command.execute(cli.state_dir),
        Command::Apps(command) => command.execute(cli.state_dir),
        Command::Remove(command) => command.execute(cli.state_dir),
    }
}

pub(super) fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

fn resolve_directory(directory: &Path) -> Result<PathBuf> {
    absolute(directory)
        .with_context(|| format!("resolving application directory {}", directory.display()))
}
