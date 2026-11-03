use super::{
    applications::Applications,
    server::{self, Config},
    state::StateDirectory,
};
use crate::{
    deployment::SharedStatus,
    persistence::{Storage, sql::SqlStorage},
};
use anyhow::Result;
use std::{
    future::Future,
    io::{self, Write},
    net::SocketAddr,
    path::Path,
    sync::Arc,
};
use tokio::sync::watch;

pub(crate) struct Host {
    pub applications: Arc<Applications>,
    pub directory: StateDirectory,
    storage: Arc<Storage>,
    config: Config,
}

impl Host {
    pub fn new(directory: &Path, listen: SocketAddr) -> Result<Self> {
        let directory = StateDirectory::claim(directory)?;
        let config = Config {
            listen,
            ..Config::default()
        };
        let storage = Arc::new(Storage::new(directory.path.join("kv")));
        let sql = Arc::new(SqlStorage::new(directory.path.join("sql")));
        let applications =
            Applications::with_persistence(config.clone(), Some(storage.clone()), Some(sql))?;
        Ok(Self {
            applications,
            directory,
            storage,
            config,
        })
    }

    pub async fn serve<F, M>(
        self,
        admin_listen: Option<SocketAddr>,
        status: SharedStatus,
        maintenance: M,
        shutdown: impl Future<Output = ()>,
    ) -> Result<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
        M: FnOnce(watch::Receiver<bool>) -> F,
    {
        let (stop, stopped) = watch::channel(false);

        let (ready, address) = tokio::sync::oneshot::channel();
        let mut hosting = tokio::spawn(server::serve_applications(
            self.config,
            self.applications.clone(),
            until_stopped(stopped.clone()),
            move |address| {
                let _ = ready.send(address);
                println!("Listening on http://{address}");
                let _ = io::stdout().flush();
            },
        ));
        let address = match address.await {
            Ok(address) => address,
            Err(_) => return hosting.await?,
        };
        let applications = self.applications.clone();
        let admin_stopped = stopped.clone();
        let mut admin = tokio::spawn(async move {
            if let Some(listen) = admin_listen {
                crate::admin::serve(
                    listen,
                    applications,
                    status,
                    address,
                    until_stopped(admin_stopped),
                    |address| {
                        println!("Admin UI listening on http://{address}");
                        let _ = io::stdout().flush();
                    },
                )
                .await
            } else {
                until_stopped(admin_stopped).await;
                Ok(())
            }
        });
        let mut reconciliation = tokio::spawn(maintenance(stopped));
        #[derive(PartialEq, Eq)]
        enum Finished {
            Hosting,
            Admin,
            Reconciliation,
            Signal,
        }
        let (finished, result) = tokio::select! {
            result = &mut hosting => (Finished::Hosting, result),
            result = &mut admin => (Finished::Admin, result),
            result = &mut reconciliation => (Finished::Reconciliation, result),
            () = shutdown => (Finished::Signal, Ok(Ok(()))),
        };
        self.applications.stop();
        stop.send_replace(true);
        let mut outcome = result
            .map_err(anyhow::Error::from)
            .and_then(|result| result);
        for (role, task) in [
            (Finished::Hosting, hosting),
            (Finished::Admin, admin),
            (Finished::Reconciliation, reconciliation),
        ] {
            if finished != role {
                let result = task
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|result| result);
                if outcome.is_ok() {
                    outcome = result;
                }
            }
        }
        let storage = self.storage;
        let synced = tokio::task::spawn_blocking(move || storage.sync()).await?;
        outcome.and(synced)
    }
}

pub(crate) async fn until_stopped(mut stopped: watch::Receiver<bool>) {
    if !*stopped.borrow() {
        let _ = stopped.changed().await;
    }
}
