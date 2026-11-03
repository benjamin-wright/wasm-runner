use std::{future::Future, net::SocketAddr, time::Duration};

use anyhow::{Context, Result, ensure};
use rocket::{
    State,
    fairing::AdHoc,
    http::{ContentType, Status},
    response::content::{RawCss, RawHtml},
};

use crate::{deployment::SharedStatus, hosting::applications::Applications};
use std::sync::Arc;

mod render;
const STYLESHEET: &str = include_str!("style.css");
const METADATA_TIMEOUT: Duration = Duration::from_secs(5);

struct Source {
    applications: Arc<Applications>,
    status: SharedStatus,
    listen: SocketAddr,
}

#[rocket::get("/")]
async fn index(source: &State<Source>) -> (Status, RawHtml<String>) {
    let metadata = tokio::time::timeout(METADATA_TIMEOUT, async {
        tokio::try_join!(
            source.applications.storage_info(),
            source.applications.sql_info()
        )
    })
    .await;
    match metadata {
        Ok(Ok((storage, sql))) => {
            let status = source.status.read().expect("deployment status").clone();
            (
                Status::Ok,
                RawHtml(render::overview(
                    std::process::id(),
                    source.listen,
                    &source.applications.list(),
                    &storage,
                    &sql,
                    &status,
                )),
            )
        }
        result => (
            Status::ServiceUnavailable,
            RawHtml(render::unavailable(&format!("{result:?}"))),
        ),
    }
}

#[rocket::get("/status")]
async fn status(source: &State<Source>) -> rocket::response::content::RawJson<String> {
    let status = source.status.read().expect("deployment status").clone();
    rocket::response::content::RawJson(serde_json::to_string(&status).expect("serializable status"))
}

#[rocket::get("/live")]
fn live() -> &'static str {
    "live"
}

#[rocket::get("/ready")]
fn ready(source: &State<Source>) -> (Status, &'static str) {
    if source.status.read().expect("deployment status").ready {
        (Status::Ok, "ready")
    } else {
        (Status::ServiceUnavailable, "waiting for applications")
    }
}

#[rocket::get("/applications/<application>/handlers/<handler>")]
fn handler_metrics(
    source: &State<Source>,
    application: &str,
    handler: &str,
) -> (Status, RawHtml<String>) {
    let applications = source.applications.list();
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

#[rocket::get("/style.css")]
fn stylesheet() -> RawCss<&'static str> {
    RawCss(STYLESHEET)
}

pub(crate) async fn serve(
    listen: SocketAddr,
    applications: Arc<Applications>,
    deployment_status: SharedStatus,
    hosting_address: SocketAddr,
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
    .manage(Source {
        applications,
        status: deployment_status,
        listen: hosting_address,
    })
    .mount(
        "/",
        rocket::routes![index, handler_metrics, stylesheet, status, live, ready],
    )
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
