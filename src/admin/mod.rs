use std::{future::Future, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result, ensure};
use rocket::{
    State,
    fairing::AdHoc,
    http::{ContentType, Status},
    response::content::{RawCss, RawHtml},
};

use crate::control::{
    client,
    protocol::{Reply, Request},
};

mod render;

const STYLESHEET: &str = include_str!("style.css");
const METADATA_TIMEOUT: Duration = Duration::from_secs(5);

#[rocket::get("/")]
async fn index(socket: &State<PathBuf>) -> (Status, RawHtml<String>) {
    match metadata(socket).await {
        Ok(Reply::Status {
            pid,
            listen,
            applications,
            storage,
        }) => (
            Status::Ok,
            RawHtml(render::overview(pid, listen, &applications, &storage)),
        ),
        result => metadata_error(result),
    }
}

#[rocket::get("/applications/<application>/handlers/<handler>")]
async fn handler_metrics(
    socket: &State<PathBuf>,
    application: &str,
    handler: &str,
) -> (Status, RawHtml<String>) {
    let applications = match metadata(socket).await {
        Ok(Reply::Status { applications, .. }) => applications,
        result => return metadata_error(result),
    };
    let Some((app, function)) = applications
        .iter()
        .find(|app| app.name == application)
        .and_then(|app| {
            app.functions
                .iter()
                .find(|function| function.name == handler)
                .map(|function| (app, function))
        })
    else {
        return (Status::NotFound, RawHtml(render::missing_handler()));
    };
    (Status::Ok, RawHtml(render::handler(app, function)))
}

async fn metadata(socket: &State<PathBuf>) -> Result<Reply> {
    tokio::time::timeout(
        METADATA_TIMEOUT,
        client::request_async(socket, Request::Status),
    )
    .await
    .context("daemon metadata request timed out after five seconds")
    .and_then(|result| result)
}

fn metadata_error(result: Result<Reply>) -> (Status, RawHtml<String>) {
    let message = match result {
        Err(error) => format!("{error:#}"),
        Ok(_) => "Unexpected response to daemon status request".to_owned(),
    };
    (
        Status::ServiceUnavailable,
        RawHtml(render::unavailable(&message)),
    )
}

#[rocket::get("/style.css")]
fn stylesheet() -> RawCss<&'static str> {
    RawCss(STYLESHEET)
}

pub(crate) async fn serve(
    listen: SocketAddr,
    socket: PathBuf,
    shutdown: impl Future<Output = ()>,
    on_ready: impl FnOnce(SocketAddr) + Send + Sync + 'static,
) -> Result<()> {
    ensure!(
        listen.ip().is_loopback(),
        "admin UI requires a loopback IP address"
    );
    let rocket = rocket::custom(rocket::Config {
        address: listen.ip(),
        port: listen.port(),
        log_level: rocket::config::LogLevel::Off,
        shutdown: rocket::config::Shutdown {
            ctrlc: false,
            signals: Default::default(),
            grace: METADATA_TIMEOUT.as_secs() as u32 + 1,
            ..Default::default()
        },
        ..rocket::Config::default()
    })
    .manage(socket)
    .mount("/", rocket::routes![index, handler_metrics, stylesheet])
    .attach(AdHoc::on_response(
        "Admin response headers",
        |request, response| {
            Box::pin(async move {
                if response.content_type() == Some(ContentType::HTML) {
                    // A fragment-only refresh can navigate without fetching fresh metadata.
                    response.set_raw_header("Refresh", format!("5;url={}", request.uri()));
                }
                response.set_raw_header("Cache-Control", "no-store");
                response.set_raw_header(
                    "Content-Security-Policy",
                    "default-src 'none'; style-src 'self'; base-uri 'none'; frame-ancestors 'none'",
                );
            })
        },
    ))
    .attach(AdHoc::on_liftoff("Admin readiness", move |rocket| {
        Box::pin(async move {
            on_ready(SocketAddr::new(
                rocket.config().address,
                rocket.config().port,
            ));
        })
    }))
    .ignite()
    .await
    .context("preparing admin UI")?;
    let handle = rocket.shutdown();
    let server = rocket.launch();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result,
        () = shutdown => {
            handle.notify();
            server.await
        }
    }
    .map(|_| ())
    .context("running admin UI")
}
