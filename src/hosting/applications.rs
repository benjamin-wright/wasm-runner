use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, atomic::Ordering},
};

use anyhow::{Result, ensure};
use rocket::{Route, local::asynchronous::Client};
use tokio::sync::Mutex;

use super::{
    routing,
    server::{Config, Handler, State},
};

pub(crate) struct Applications {
    current: RwLock<Arc<Snapshot>>,
    changes: Mutex<()>,
    config: Config,
    pub(super) state: Arc<State>,
}

pub(super) struct Snapshot {
    routes: BTreeMap<String, Vec<Route>>,
    pub(super) client: Client,
}

impl Applications {
    pub(crate) async fn new(config: Config) -> Result<Arc<Self>> {
        let state = State::new(config.clone())?;
        let client = routing::dispatcher(Vec::new(), state.clone()).await?;
        Ok(Arc::new(Self {
            current: RwLock::new(Arc::new(Snapshot {
                routes: BTreeMap::new(),
                client,
            })),
            changes: Mutex::new(()),
            config,
            state,
        }))
    }

    pub(super) fn snapshot(&self) -> Arc<Snapshot> {
        self.current.read().expect("application registry").clone()
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.snapshot().routes.keys().cloned().collect()
    }

    pub(crate) fn stop(&self) {
        let _current = self.current.write().expect("application registry");
        self.state.stopping.store(true, Ordering::SeqCst);
    }

    pub(crate) async fn install(&self, name: String, handlers: Vec<Handler>) -> Result<()> {
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        let timeout = self.config.execution_timeout;
        let prepared =
            tokio::task::spawn_blocking(move || routing::prepare(handlers, timeout)).await??;
        let _change = self.changes.lock().await;
        let mut routes = self.snapshot().routes.clone();
        routes.insert(name, prepared);
        self.publish(routes).await
    }

    pub(crate) async fn remove(&self, name: &str) -> Result<()> {
        let _change = self.changes.lock().await;
        let mut routes = self.snapshot().routes.clone();
        ensure!(
            routes.remove(name).is_some(),
            "application {name} is not deployed"
        );
        self.publish(routes).await
    }

    async fn publish(&self, routes: BTreeMap<String, Vec<Route>>) -> Result<()> {
        let client = routing::dispatcher(
            routes.values().flatten().cloned().collect(),
            self.state.clone(),
        )
        .await?;
        let mut current = self.current.write().expect("application registry");
        ensure!(
            !self.state.stopping.load(Ordering::SeqCst),
            "daemon is stopping"
        );
        let previous = std::mem::replace(&mut *current, Arc::new(Snapshot { routes, client }));
        drop(current);
        drop(previous);
        Ok(())
    }
}
