use crate::{
    hosting::{deployment::Deployment, server::Handler},
    project::manifest,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) const LIMIT: usize = 128 * 1024 * 1024;
pub(crate) const RELEASE_TYPE: &str = "application/vnd.wr.release.v1+json";
pub(crate) const COMPONENT_TYPE: &str = "application/wasm";
pub(crate) const MIGRATIONS_TYPE: &str = "application/vnd.wr.migrations.v1+json";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub manifest: String,
    pub blob: String,
    pub size: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Release {
    pub format_version: u32,
    pub source_commit: String,
    pub version: String,
    pub application: serde_json::Value,
    pub functions: BTreeMap<String, Artifact>,
    pub migrations: Option<Artifact>,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub(crate) fn validate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 71
            && value.starts_with("sha256:")
            && value[7..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "expected a lowercase SHA-256 digest"
    );
    Ok(())
}

impl Artifact {
    pub fn validate(&self) -> Result<()> {
        validate_digest(&self.manifest)?;
        validate_digest(&self.blob)?;
        ensure!(self.size <= LIMIT as u64, "artifact exceeds 128 MiB");
        Ok(())
    }
    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            bytes.len() as u64 == self.size && digest(bytes) == self.blob,
            "artifact size or digest mismatch"
        );
        Ok(())
    }
}

impl Release {
    pub fn validate(&self) -> Result<manifest::ApplicationDefinition> {
        ensure!(self.format_version == 1, "unsupported release format");
        ensure!(
            matches!(self.source_commit.len(), 40 | 64)
                && self.source_commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "release requires a source commit ID"
        );
        let application = manifest::parse(&serde_json::to_string(&self.application)?)?;
        ensure!(
            application.runtime_config() == self.application,
            "release application must contain only normalized runtime configuration"
        );
        ensure!(
            self.functions.len() == application.functions.len()
                && application
                    .functions
                    .keys()
                    .all(|name| self.functions.contains_key(name)),
            "release function set does not match runtime configuration"
        );
        ensure!(
            application.sql.enabled == self.migrations.is_some(),
            "SQL-enabled release requires a migration bundle"
        );
        let mut size = 0;
        for artifact in self.functions.values().chain(self.migrations.iter()) {
            artifact.validate()?;
            size += artifact.size;
        }
        ensure!(size <= LIMIT as u64, "release exceeds 128 MiB");
        Ok(application)
    }

    pub fn deployment(
        &self,
        mut read: impl FnMut(&Artifact) -> Result<Vec<u8>>,
    ) -> Result<Deployment> {
        let application = self.validate()?;
        let mut handlers = Vec::new();
        for (name, artifact) in &self.functions {
            let function = &application.functions[name];
            handlers.push(Handler {
                name: name.clone(),
                method: function.http.method.to_string(),
                path: function.http.path.clone(),
                component: read(artifact)?,
            });
        }
        let migrations = self
            .migrations
            .as_ref()
            .map(|artifact| -> Result<_> {
                serde_json::from_slice(&read(artifact)?).context("decoding migration bundle")
            })
            .transpose()?
            .unwrap_or_default();
        let deployment = Deployment {
            name: application.name,
            path_prefix: application.path_prefix,
            kv: application.kv,
            sql: application.sql,
            migrations,
            handlers,
        };
        deployment.validate()?;
        Ok(deployment)
    }
}
