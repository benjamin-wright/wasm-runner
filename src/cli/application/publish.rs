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
        let auth_mode = match &auth {
            oci_client::secrets::RegistryAuth::Anonymous => "anonymous",
            oci_client::secrets::RegistryAuth::Basic(_, _) => "basic",
            _ => "configured",
        };
        println!("Registry authentication: {auth_mode}");
        let digest = super::super::runtime()?.block_on(registry.publish(&prepared, &auth))?;
        println!("Published {reference} ({digest})");
        Ok(())
    }
}
