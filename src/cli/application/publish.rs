use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Package built components and publish the application OCI artifact
#[derive(Args)]
pub(crate) struct Publish {
    path: PathBuf,
}

impl Publish {
    pub fn execute(self) -> Result<()> {
        let prepared = crate::deployment::package::prepare(&self.path)?;
        let auth = crate::deployment::registry::auth_from_environment()?;
        let registry = crate::deployment::registry::Registry::new(&prepared.registry)?;
        let reference = format!(
            "{}/{}:{}",
            prepared.registry, prepared.name, prepared.version
        );
        let digest = super::super::runtime()?.block_on(registry.publish(&prepared, &auth))?;
        println!("Published {reference} ({digest})");
        Ok(())
    }
}
