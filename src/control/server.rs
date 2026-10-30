use super::protocol::{MAX_MESSAGE, Reply, Request};
use crate::hosting::applications::Applications;
use anyhow::{Context, Result, ensure};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Semaphore, watch},
    task::JoinSet,
};

pub(crate) async fn serve(
    listener: UnixListener,
    applications: Arc<Applications>,
    listen: SocketAddr,
    stop: watch::Sender<bool>,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    let slots = Arc::new(Semaphore::new(4));
    let mut tasks = JoinSet::new();
    let result = loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => break Ok(()),
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if let Err(error) = result { eprintln!("control task: {error}"); }
            }
            accepted = listener.accept() => {
                let (stream, _) = match accepted { Ok(value) => value, Err(error) => break Err(error.into()) };
                let Ok(slot) = slots.clone().try_acquire_owned() else { continue; };
                let applications = applications.clone();
                let stop = stop.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    if let Err(error) = connection(stream, applications, listen, stop).await {
                        eprintln!("control request: {error:#}");
                    }
                });
            }
        }
    };
    drop(listener);
    applications.stop();
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result {
            eprintln!("control task: {error}");
        }
    }
    result
}

async fn connection(
    mut stream: UnixStream,
    applications: Arc<Applications>,
    listen: SocketAddr,
    stop: watch::Sender<bool>,
) -> Result<()> {
    let operation = async {
        let request: Request = tokio::time::timeout(Duration::from_secs(30), async {
            let size = stream.read_u32().await? as usize;
            ensure!(size <= MAX_MESSAGE, "control message exceeds 128 MiB");
            let mut bytes = vec![0; size];
            stream.read_exact(&mut bytes).await?;
            serde_json::from_slice(&bytes).context("invalid control request")
        })
        .await
        .context("control request timed out")??;
        ensure!(!*stop.borrow(), "daemon is stopping");
        match request {
            Request::Deploy { application } => {
                application.validate()?;
                applications
                    .install(application.name, application.handlers)
                    .await?;
                Ok(Reply::Ok)
            }
            Request::Remove { name } => {
                applications.remove(&name).await?;
                Ok(Reply::Ok)
            }
            Request::List => Ok(Reply::Applications {
                names: applications.names(),
            }),
            Request::Status => Ok(Reply::Status {
                pid: std::process::id(),
                listen,
                applications: applications.names(),
            }),
            Request::Stop => {
                applications.stop();
                stop.send_replace(true);
                Ok(Reply::Ok)
            }
        }
    }
    .await;
    let reply = operation.unwrap_or_else(|error: anyhow::Error| Reply::Error {
        message: format!("{error:#}"),
    });
    let bytes = serde_json::to_vec(&reply)?;
    tokio::time::timeout(Duration::from_secs(30), async {
        stream.write_u32(bytes.len() as u32).await?;
        stream.write_all(&bytes).await
    })
    .await??;
    Ok(())
}
