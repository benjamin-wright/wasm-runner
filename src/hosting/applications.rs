use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, atomic::Ordering},
};

use anyhow::{Result, ensure};
use rocket::local::asynchronous::Client;
use serde::{Deserialize, Serialize};

use super::{
    routing,
    server::{Config, Handler, State},
};

pub(crate) struct Applications {
    current: RwLock<BTreeMap<String, Arc<Application>>>,
    config: Config,
    pub(super) state: Arc<State>,
}

pub(super) struct Application {
    path_prefix: String,
    pub(super) client: Client,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ApplicationInfo {
    pub name: String,
    pub path_prefix: String,
}

impl Applications {
    pub(crate) fn new(config: Config) -> Result<Arc<Self>> {
        let state = State::new(config.clone())?;
        Ok(Arc::new(Self {
            current: RwLock::new(BTreeMap::new()),
            config,
            state,
        }))
    }

    pub(super) fn select(&self, path: &str) -> Option<(Arc<Application>, String)> {
        self.current
            .read()
            .expect("application registry")
            .values()
            .find_map(|application| {
                relative_path(&application.path_prefix, path)
                    .map(|path| (application.clone(), path.to_owned()))
            })
    }

    pub(crate) fn list(&self) -> Vec<ApplicationInfo> {
        self.current
            .read()
            .expect("application registry")
            .iter()
            .map(|(name, application)| ApplicationInfo {
                name: name.clone(),
                path_prefix: application.path_prefix.clone(),
            })
            .collect()
    }

    pub(crate) fn stop(&self) {
        let _current = self.current.write().expect("application registry");
        self.state.stopping.store(true, Ordering::SeqCst);
    }

    pub(crate) async fn install(
        &self,
        name: String,
        path_prefix: String,
        handlers: Vec<Handler>,
    ) -> Result<()> {
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        validate_prefix(&path_prefix)?;
        let timeout = self.config.execution_timeout;
        let prepared =
            tokio::task::spawn_blocking(move || routing::prepare(handlers, timeout)).await??;
        let client = routing::dispatcher(prepared, self.state.clone()).await?;
        let application = Arc::new(Application {
            path_prefix,
            client,
        });
        let mut current = self.current.write().expect("application registry");
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        for (other_name, other) in current
            .iter()
            .filter(|(other_name, _)| **other_name != name)
        {
            ensure!(
                relative_path(&application.path_prefix, &other.path_prefix).is_none()
                    && relative_path(&other.path_prefix, &application.path_prefix).is_none(),
                "path-prefix {} for {name} overlaps {} for {other_name}",
                application.path_prefix,
                other.path_prefix
            );
        }
        let previous = current.insert(name, application);
        drop(current);
        drop(previous);
        Ok(())
    }

    pub(crate) fn remove(&self, name: &str) -> Result<()> {
        let mut current = self.current.write().expect("application registry");
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        let previous = current.remove(name);
        drop(current);
        ensure!(previous.is_some(), "application {name} is not deployed");
        Ok(())
    }
}

fn validate_prefix(prefix: &str) -> Result<()> {
    ensure!(
        prefix == "/"
            || prefix.strip_prefix('/').is_some_and(|path| {
                path.split('/').all(|segment| {
                    !segment.is_empty()
                        && segment != "."
                        && segment != ".."
                        && segment.bytes().all(|c| {
                            c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~')
                        })
                })
            }),
        "invalid path-prefix {prefix:?}: expected / or literal path segments without trailing slash, dot segments, or encoding"
    );
    Ok(())
}

fn relative_path<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    if prefix == "/" {
        return Some(path);
    }
    match path.strip_prefix(prefix) {
        Some("") => Some("/"),
        Some(path) if path.starts_with('/') => Some(path),
        _ => None,
    }
}
