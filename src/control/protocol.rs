use crate::hosting::server::Handler;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_MESSAGE: usize = 128 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Deployment {
    pub name: String,
    pub handlers: Vec<Handler>,
}

impl Deployment {
    pub fn validate(&self) -> Result<()> {
        fn identifier(name: &str) -> bool {
            !name.is_empty()
                && name.len() <= 64
                && name.as_bytes()[0].is_ascii_lowercase()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        }
        ensure!(identifier(&self.name), "invalid application name");
        ensure!(!self.handlers.is_empty(), "application must have handlers");
        let mut names = std::collections::HashSet::new();
        for handler in &self.handlers {
            ensure!(
                identifier(&handler.name) && names.insert(&handler.name),
                "invalid or duplicate handler name"
            );
            ensure!(
                matches!(
                    handler.method.as_str(),
                    "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
                ),
                "unsupported HTTP method"
            );
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Deploy { application: Deployment },
    Remove { name: String },
    List,
    Status,
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub(crate) enum Reply {
    Ok,
    Status {
        pid: u32,
        listen: std::net::SocketAddr,
        applications: Vec<String>,
    },
    Applications {
        names: Vec<String>,
    },
    Error {
        message: String,
    },
}
