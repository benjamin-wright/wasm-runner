use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub(super) struct Apps {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List deployed applications
    List,
}

/// Remove an application from the daemon
#[derive(Args)]
pub(super) struct Remove {
    name: String,
}

impl Apps {
    pub(super) fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        #[cfg(unix)]
        {
            use crate::{
                control::{
                    client,
                    protocol::{Reply, Request},
                },
                daemon::process::Paths,
            };
            let paths = Paths::new(directory)?;
            match self.command {
                Command::List => {
                    if let Reply::Applications { applications } =
                        client::request(&paths.socket, Request::List)?
                    {
                        for application in applications {
                            println!("{}\t{}", application.name, application.path_prefix);
                        }
                    }
                }
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            anyhow::bail!("application management currently requires Unix")
        }
    }
}

impl Remove {
    pub(super) fn execute(self, directory: Option<PathBuf>) -> Result<()> {
        #[cfg(unix)]
        {
            use crate::{
                control::{client, protocol::Request},
                daemon::process::Paths,
            };
            let paths = Paths::new(directory)?;
            client::request(
                &paths.socket,
                Request::Remove {
                    name: self.name.clone(),
                },
            )?;
            println!("Removed {}", self.name);
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            anyhow::bail!("application management currently requires Unix")
        }
    }
}
