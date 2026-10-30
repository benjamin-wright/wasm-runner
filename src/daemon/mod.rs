#[cfg(unix)]
pub(crate) mod process;

#[cfg(unix)]
pub(crate) async fn serve(
    paths: process::Paths,
    listen: std::net::SocketAddr,
) -> anyhow::Result<()> {
    use crate::{
        control,
        hosting::{
            applications::Applications,
            server::{self, Config},
        },
    };
    use std::{
        fs,
        io::{self, Write},
        os::unix::fs::PermissionsExt,
    };
    use tokio::{
        net::UnixListener,
        sync::{oneshot, watch},
    };

    let _guard = paths.claim()?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let listener = UnixListener::bind(&paths.socket)?;
    fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o600))?;
    let config = Config {
        listen,
        ..Config::default()
    };
    let applications = Applications::new(config.clone())?;
    let (stop, stopped) = watch::channel(false);
    let (ready, address) = oneshot::channel();
    let mut host_stopped = stopped.clone();
    let mut hosting = tokio::spawn(server::serve_applications(
        config,
        applications.clone(),
        async move {
            if !*host_stopped.borrow() {
                let _ = host_stopped.changed().await;
            }
        },
        move |address| {
            let _ = ready.send(address);
        },
    ));
    let address = match address.await {
        Ok(address) => address,
        Err(_) => return hosting.await?,
    };
    let mut control = tokio::spawn(control::server::serve(
        listener,
        applications.clone(),
        address,
        stop.clone(),
        stopped.clone(),
    ));
    println!("Listening on http://{address}");
    io::stdout().flush()?;
    let mut requested = stopped;
    enum Finished {
        Hosting,
        Control,
        Signal,
    }
    let (finished, result) = tokio::select! {
        result = &mut hosting => (Finished::Hosting, result),
        result = &mut control => (Finished::Control, result),
        _ = terminate.recv() => (Finished::Signal, Ok(Ok(()))),
        _ = interrupt.recv() => (Finished::Signal, Ok(Ok(()))),
        _ = requested.changed() => (Finished::Signal, Ok(Ok(()))),
    };
    applications.stop();
    stop.send_replace(true);
    let mut outcome = result
        .map_err(anyhow::Error::from)
        .and_then(|result| result);
    if !matches!(finished, Finished::Hosting) {
        let result = hosting
            .await
            .map_err(anyhow::Error::from)
            .and_then(|result| result);
        if outcome.is_ok() {
            outcome = result;
        }
    }
    if !matches!(finished, Finished::Control) {
        let result = control
            .await
            .map_err(anyhow::Error::from)
            .and_then(|result| result);
        if outcome.is_ok() {
            outcome = result;
        }
    }
    outcome
}
