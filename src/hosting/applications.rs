use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, atomic::Ordering},
};

use crate::persistence::{
    Storage, StorageInfo,
    sql::{Migration, SqlConfig, SqlInfo, SqlStorage, validate_migrations},
};
use crate::runtime::Capabilities;
use anyhow::{Result, ensure};
use rocket::local::asynchronous::Client;
use serde::{Deserialize, Serialize};

use super::{
    metrics::{HandlerMetrics, Snapshot},
    routing,
    server::{Config, Handler, State},
};

pub(crate) struct Applications {
    current: RwLock<BTreeMap<String, Arc<Application>>>,
    config: Config,
    pub(super) state: Arc<State>,
    storage: Option<Arc<Storage>>,
    sql: Option<Arc<SqlStorage>>,
    publication: Arc<tokio::sync::Mutex<()>>,
}

pub(super) struct Application {
    path_prefix: String,
    pub(super) client: Client,
    kv: bool,
    sql: bool,
    metrics: HandlerMetrics,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ApplicationInfo {
    pub name: String,
    pub path_prefix: String,
    pub functions: Vec<FunctionInfo>,
    #[serde(default)]
    pub kv: bool,
    #[serde(default)]
    pub sql: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct FunctionInfo {
    pub name: String,
    pub method: String,
    pub path: String,
    pub metrics: Snapshot,
}

impl Applications {
    pub(crate) fn new(config: Config) -> Result<Arc<Self>> {
        Self::with_storage(config, None)
    }

    pub(crate) fn with_storage(config: Config, storage: Option<Arc<Storage>>) -> Result<Arc<Self>> {
        Self::with_persistence(config, storage, None)
    }

    pub(crate) fn with_persistence(
        config: Config,
        storage: Option<Arc<Storage>>,
        sql: Option<Arc<SqlStorage>>,
    ) -> Result<Arc<Self>> {
        let state = State::new(config.clone())?;
        Ok(Arc::new(Self {
            current: RwLock::new(BTreeMap::new()),
            config,
            state,
            storage,
            sql,
            publication: Arc::new(tokio::sync::Mutex::new(())),
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
            .map(|(name, application)| {
                let mut functions: Vec<_> = application
                    .client
                    .rocket()
                    .routes()
                    .map(|route| {
                        let name = route.name.as_deref().expect("named application route");
                        FunctionInfo {
                            name: name.to_owned(),
                            method: route.method.to_string(),
                            path: route.uri.to_string(),
                            metrics: application.metrics[name].snapshot(),
                        }
                    })
                    .collect();
                functions.sort_by(|left, right| left.name.cmp(&right.name));
                ApplicationInfo {
                    name: name.clone(),
                    path_prefix: application.path_prefix.clone(),
                    functions,
                    kv: application.kv,
                    sql: application.sql,
                }
            })
            .collect()
    }

    pub(crate) async fn storage_info(&self) -> Result<StorageInfo> {
        let storage = self.storage.clone();
        Ok(tokio::task::spawn_blocking(move || {
            storage.map(|storage| storage.inspect()).unwrap_or_default()
        })
        .await?)
    }

    pub(crate) async fn sql_info(&self) -> Result<Vec<SqlInfo>> {
        let sql = self.sql.clone();
        Ok(
            tokio::task::spawn_blocking(move || sql.map(|sql| sql.inspect()).unwrap_or_default())
                .await?,
        )
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
        kv: bool,
        sql: SqlConfig,
        migrations: Vec<Migration>,
    ) -> Result<()> {
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        validate_prefix(&path_prefix)?;
        sql.validate()?;
        ensure!(
            sql.enabled || migrations.is_empty(),
            "migrations require SQL to be enabled"
        );
        validate_migrations(&migrations, sql.migration_target)?;
        let timeout = self.config.execution_timeout;
        let storage = self.storage.clone();
        let identity = name.clone();
        let sql_storage = self.sql.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let store = if kv {
                Some(
                    storage
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("KV storage is not configured"))?
                        .application(&identity)?,
                )
            } else {
                None
            };
            let database = if sql.enabled {
                Some(
                    sql_storage
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("SQL storage is not configured"))?
                        .application(&identity)?,
                )
            } else {
                None
            };
            let prepared = routing::prepare(
                handlers,
                timeout,
                Capabilities {
                    kv: store,
                    sql: database.clone(),
                },
            )?;
            Ok::<_, anyhow::Error>((prepared, database))
        })
        .await??;
        let (prepared, database) = prepared;
        let client = routing::dispatcher(prepared.routes, self.state.clone()).await?;
        let publication = self.publication.clone().lock_owned().await;
        self.validate_publication(&name, &path_prefix)?;
        // Keep the publication guard in the blocking job if this future is cancelled.
        let _publication = if let Some(database) = database {
            tokio::task::spawn_blocking(move || {
                let guard = publication;
                database.migrate(&migrations, sql.migration_target)?;
                Ok::<_, anyhow::Error>(guard)
            })
            .await??
        } else {
            publication
        };
        let application = Arc::new(Application {
            path_prefix,
            client,
            kv,
            sql: sql.enabled,
            metrics: prepared.metrics,
        });
        let mut current = self.current.write().expect("application registry");
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        let previous = current.insert(name, application);
        drop(current);
        drop(previous);
        Ok(())
    }

    fn validate_publication(&self, name: &str, path_prefix: &str) -> Result<()> {
        let current = self.current.read().expect("application registry");
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        for (other_name, other) in current
            .iter()
            .filter(|(other_name, _)| other_name.as_str() != name)
        {
            ensure!(
                relative_path(path_prefix, &other.path_prefix).is_none()
                    && relative_path(&other.path_prefix, path_prefix).is_none(),
                "path-prefix {} for {name} overlaps {} for {other_name}",
                path_prefix,
                other.path_prefix
            );
        }
        Ok(())
    }

    pub(crate) async fn remove(&self, name: &str) -> Result<()> {
        let _publication = self.publication.lock().await;
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
