use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub repo: String,
    pub branch: String,
    pub path: String,
}

impl Source {
    pub fn validate(&self) -> Result<()> {
        let url = self.repo.parse::<http::Uri>()?;
        let host = url.host().unwrap_or_default();
        ensure!(
            url.scheme_str() == Some("https")
                || (url.scheme_str() == Some("http")
                    && matches!(host, "localhost" | "127.0.0.1" | "[::1]")),
            "repository requires HTTPS (HTTP is allowed only on loopback)"
        );
        ensure!(
            !host.is_empty() && !self.repo.contains('@') && url.query().is_none(),
            "repository URL must not contain credentials or a query"
        );
        ensure!(
            !self.branch.is_empty()
                && !self.branch.starts_with('-')
                && !self
                    .branch
                    .contains([':', '\\', ' ', '~', '^', '?', '*', '['])
                && !self.branch.contains("..")
                && !self.branch.contains("@{")
                && !self.branch.chars().any(char::is_control),
            "invalid branch"
        );
        ensure!(
            !self.path.is_empty()
                && self
                    .path
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".."))
                && !self.path.contains('\\')
                && !self.path.chars().any(char::is_control),
            "manifest path must be repository relative"
        );
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub applications: BTreeMap<String, Source>,
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let config: Self = yaml_serde::from_slice(bytes)?;
        ensure!(
            config.applications.len() <= 1000,
            "too many application registrations"
        );
        for (name, source) in &config.applications {
            ensure!(
                crate::identifier::is_valid(name),
                "invalid registration name {name}"
            );
            source.validate()?;
        }
        Ok(config)
    }
}
