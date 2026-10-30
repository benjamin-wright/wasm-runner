use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Upload built components and atomically create or replace an application
#[derive(Args)]
pub(crate) struct Deploy {
    #[arg(value_name = "DIRECTORY")]
    directory: PathBuf,
}

impl Deploy {
    pub(crate) fn execute(self, state_directory: Option<PathBuf>) -> Result<()> {
        #[cfg(unix)]
        {
            use crate::{
                control::{client, protocol::Request},
                daemon::process::Paths,
                project::package,
            };
            let directory = crate::cli::resolve_directory(&self.directory)?;
            let application = package::deployment(&directory)?;
            let name = application.name.clone();
            let paths = Paths::new(state_directory)?;
            client::request(&paths.socket, Request::Deploy { application })?;
            println!("Deployed {name}");
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = state_directory;
            anyhow::bail!("deployment currently requires Unix")
        }
    }
}
