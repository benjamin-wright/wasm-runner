use anyhow::Result;
use clap::{Args, Subcommand};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Args)]
pub(super) struct Daemon {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a background daemon, or report the already running instance
    Start {
        #[arg(long, value_name = "IP:PORT", default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
    },
    /// Query the running daemon
    Status,
    /// Drain requests and stop the daemon
    Stop,
    #[command(hide = true)]
    Serve {
        #[arg(long)]
        listen: SocketAddr,
    },
}

impl Daemon {
    pub(super) fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        #[cfg(unix)]
        {
            use crate::{
                control::{
                    client,
                    protocol::{Reply, Request},
                },
                daemon::{self, process::Paths},
            };
            let paths = Paths::new(directory)?;
            match self.command {
                Command::Start { listen } => {
                    match client::request(&paths.socket, Request::Status) {
                        Ok(Reply::Status { pid, listen, .. }) => {
                            println!(
                                "Daemon already running (PID {pid})\nListening on http://{listen}\nControl socket: {}\nLog: {}",
                                paths.socket.display(),
                                paths.log.display()
                            );
                            return Ok(());
                        }
                        Ok(_) => anyhow::bail!("unexpected response to daemon status request"),
                        Err(error)
                            if matches!(
                                error
                                    .downcast_ref::<std::io::Error>()
                                    .map(std::io::Error::kind),
                                Some(
                                    std::io::ErrorKind::NotFound
                                        | std::io::ErrorKind::ConnectionRefused
                                )
                            ) => {}
                        Err(error) => return Err(error.context("checking for a running daemon")),
                    }
                    let ready = daemon::process::start(&paths, listen)?;
                    println!(
                        "Daemon started\n{ready}\nControl socket: {}\nLog: {}",
                        paths.socket.display(),
                        paths.log.display()
                    );
                }
                Command::Serve { listen } => {
                    super::runtime()?.block_on(daemon::serve(paths, listen))?
                }
                Command::Status => {
                    if let Reply::Status {
                        pid,
                        listen,
                        applications,
                    } = client::request(&paths.socket, Request::Status)?
                    {
                        println!(
                            "Daemon running (PID {pid})\nListening on http://{listen}\nApplications: {}",
                            applications.len()
                        );
                        for name in applications {
                            println!("{name}");
                        }
                    }
                }
                Command::Stop => {
                    client::request(&paths.socket, Request::Stop)?;
                    paths.wait_stopped()?;
                    println!("Daemon stopped");
                }
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            anyhow::bail!("daemon commands currently require Unix")
        }
    }
}
