use std::{fmt::Write, future::Future, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result, ensure};
use rocket::{
    State,
    fairing::AdHoc,
    http::Status,
    response::content::{RawCss, RawHtml},
};

use crate::control::{
    client,
    protocol::{Reply, Request},
};

const PAGE_TEMPLATE: &str = include_str!("page.html");
const STYLESHEET: &str = include_str!("style.css");
const METADATA_TIMEOUT: Duration = Duration::from_secs(5);

#[rocket::get("/")]
async fn index(socket: &State<PathBuf>) -> (Status, RawHtml<String>) {
    let result = tokio::time::timeout(
        METADATA_TIMEOUT,
        client::request_async(socket, Request::Status),
    )
    .await
    .context("daemon metadata request timed out after five seconds")
    .and_then(|result| result);

    let mut content = String::new();
    let status = match result {
        Ok(Reply::Status {
            pid,
            listen,
            mut applications,
        }) => {
            write!(
                content,
                r#"<section id="daemon">
  <h2>Daemon</h2>
  <dl>
    <div><dt>Status</dt><dd>Running</dd></div>
    <div><dt>Process ID</dt><dd>{pid}</dd></div>
    <div><dt>Hosting address</dt><dd><code>http://{listen}</code></dd></div>
  </dl>
</section>
<section id="applications">
  <h2>Applications <span class="count">{}</span></h2>"#,
                applications.len()
            )
            .unwrap();
            applications.sort_by(|left, right| left.name.cmp(&right.name));
            if applications.is_empty() {
                content.push_str("<p class=\"empty\">No deployed applications.</p>");
            } else {
                content.push_str(
                    r#"<table>
  <caption class="visually-hidden">Deployed applications, path prefixes, and functions with application-relative routes</caption>
  <thead><tr><th scope="col">Name</th><th scope="col">Path prefix</th><th scope="col" class="functions-column">Functions <span class="route-note">Application-relative routes</span></th></tr></thead>
  <tbody>"#,
                );
                for application in applications {
                    write!(
                        content,
                        "<tr><th scope=\"row\">{}</th><td><code>{}</code></td><td><ul class=\"functions\">",
                        escape(&application.name),
                        escape(&application.path_prefix)
                    )
                    .unwrap();
                    for function in application.functions {
                        write!(
                            content,
                            "<li><strong>{}</strong><span><code>{}</code> <code>{}</code></span></li>",
                            escape(&function.name),
                            escape(&function.method),
                            escape(&function.path)
                        )
                        .unwrap();
                    }
                    content.push_str("</ul></td></tr>");
                }
                content.push_str("</tbody></table>");
            }
            content.push_str("</section>");
            Status::Ok
        }
        result => {
            let message = match result {
                Err(error) => format!("{error:#}"),
                Ok(_) => "Unexpected response to daemon status request".to_owned(),
            };
            write!(
                content,
                r#"<section class="error" role="alert">
  <h2>Daemon metadata unavailable</h2>
  <p>{}</p>
  <p>Check that the selected daemon is running, then refresh this page.</p>
</section>"#,
                escape(&message)
            )
            .unwrap();
            Status::ServiceUnavailable
        }
    };
    (
        status,
        RawHtml(PAGE_TEMPLATE.replace("<!-- content -->", &content)),
    )
}

#[rocket::get("/style.css")]
fn stylesheet() -> RawCss<&'static str> {
    RawCss(STYLESHEET)
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
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
    .mount("/", rocket::routes![index, stylesheet])
    .attach(AdHoc::on_response(
        "Admin response headers",
        |_, response| {
            Box::pin(async move {
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
