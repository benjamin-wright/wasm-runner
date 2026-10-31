use std::{
    collections::BTreeMap, fmt::Write, future::Future, net::SocketAddr, path::PathBuf,
    time::Duration,
};

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
            applications,
            storage,
        }) => {
            write!(
                content,
                r#"<section id="daemon">
  <h2>Daemon</h2>
  <dl>
    <div><dt>Status</dt><dd>Running</dd></div>
    <div><dt>Process ID</dt><dd>{pid}</dd></div>
    <div><dt>Hosting address</dt><dd><code>http://{listen}</code></dd></div>
  </dl>"#,
            )
            .unwrap();
            render_database(&mut content, &storage);
            content.push_str("</section>");

            let mut owners = BTreeMap::new();
            for application in &applications {
                owners.insert(application.name.as_str(), (Some(application), None));
            }
            for store in &storage.keyspaces {
                owners.entry(store.name.as_str()).or_insert((None, None)).1 = Some(store);
            }
            write!(content, "<section id=\"applications\"><h2>Applications <span class=\"count\">{}</span></h2>", owners.len()).unwrap();
            if owners.is_empty() {
                content.push_str("<p class=\"empty\">No deployed applications.</p>");
            } else {
                content.push_str(
                    r#"<div class="application-table"><table>
  <caption class="visually-hidden">Applications, deployment status, routes, and keyspace file sizes</caption>
  <thead><tr><th scope="col">Name</th><th scope="col">Status</th><th scope="col">Path prefix</th><th scope="col" class="functions-column">Functions</th><th scope="col">Keyspace file bytes</th></tr></thead>"#,
                );
                for (name, (application, store)) in owners {
                    let status = if application.is_some() {
                        "Enabled"
                    } else {
                        "Disabled"
                    };
                    write!(content, "<tbody data-application=\"{}\"><tr><th scope=\"row\">{}</th><td><span class=\"status {}\">{status}</span></td>", escape(name), escape(name), status.to_ascii_lowercase()).unwrap();
                    if let Some(application) = application {
                        write!(
                            content,
                            "<td><code>{}</code></td><td><ul class=\"functions\">",
                            escape(&application.path_prefix)
                        )
                        .unwrap();
                        for function in &application.functions {
                            write!(content, "<li><strong>{}</strong><span><code>{}</code> <code>{}</code></span></li>", escape(&function.name), escape(&function.method), escape(&function.path)).unwrap();
                        }
                        content.push_str("</ul></td>");
                    } else {
                        content.push_str("<td>—</td><td>—</td>");
                    }
                    let file_bytes = if application.is_some_and(|application| !application.kv) {
                        "—".to_owned()
                    } else {
                        store
                            .and_then(|store| store.disk_bytes)
                            .map(|bytes| bytes.to_string())
                            .unwrap_or_else(|| "Unavailable".to_owned())
                    };
                    write!(
                        content,
                        "<td class=\"keyspace-bytes\">{file_bytes}</td></tr></tbody>"
                    )
                    .unwrap();
                }
                content.push_str("</table></div>");
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
  <p>Check that the selected daemon is running.</p>
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

fn render_database(content: &mut String, storage: &crate::persistence::StorageInfo) {
    content.push_str("<h3>KV storage</h3>");
    if !storage.opened {
        content.push_str("<p class=\"empty\">Not initialized.</p>");
        return;
    }
    let number = |value: Option<u64>| {
        value
            .map(|n| n.to_string())
            .unwrap_or_else(|| "Unavailable".into())
    };
    write!(content, "<dl><div><dt>Database disk bytes</dt><dd>{}</dd></div><div><dt>Journal fragments</dt><dd>{}</dd></div><div><dt>Storage errors</dt><dd>{}</dd></div></dl>",
        number(storage.disk_bytes), number(storage.journal_count.map(|n| n as u64)), storage.storage_errors).unwrap();
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
