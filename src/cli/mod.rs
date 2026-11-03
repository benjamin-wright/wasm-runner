mod application;
mod start;
use application::{build, clean, package, run, setup, update};

use std::path::{Path, PathBuf, absolute};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wr", version, about)]
struct Cli {
    /// Private host state directory (or WR_STATE_DIR)
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
    Package(package::Package),
    Start(start::Start),
}

pub fn execute() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Setup(command) => command.execute(),
        Command::Build(command) => command.execute(),
        Command::Update(command) => command.execute(),
        Command::Clean(command) => command.execute(),
        Command::Run(command) => command.execute(cli.state_dir),
        Command::Package(command) => command.execute(),
        Command::Start(command) => command.execute(cli.state_dir),
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

pub(super) fn shutdown_signal() -> anyhow::Result<impl std::future::Future<Output = ()>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        Ok(async move {
            tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
        })
    }
    #[cfg(not(unix))]
    {
        Ok(async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("signal listener: {error}");
            }
        })
    }
}
