use std::{collections::BTreeMap, fmt::Write, net::SocketAddr};

use crate::{
    hosting::applications::{ApplicationInfo, FunctionInfo},
    persistence::{StorageInfo, StoreInfo, sql::SqlInfo},
};

const PAGE_TEMPLATE: &str = include_str!("page.html");

pub(super) fn overview(
    pid: u32,
    listen: SocketAddr,
    applications: &[ApplicationInfo],
    storage: &StorageInfo,
    sql: &[SqlInfo],
) -> String {
    let mut content = String::new();
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
    render_database(&mut content, storage);
    content.push_str("</section>");

    render_applications(&mut content, applications, storage, sql);
    page(&content)
}

fn render_applications(
    content: &mut String,
    applications: &[ApplicationInfo],
    storage: &StorageInfo,
    sql: &[SqlInfo],
) {
    let mut owners = BTreeMap::new();
    for application in applications {
        owners.insert(application.name.as_str(), (Some(application), None, None));
    }
    for store in &storage.keyspaces {
        owners
            .entry(store.name.as_str())
            .or_insert((None, None, None))
            .1 = Some(store);
    }
    for database in sql {
        owners
            .entry(database.name.as_str())
            .or_insert((None, None, None))
            .2 = Some(database);
    }
    write!(content, r#"<section id="applications"><h2>Applications <span class="count">{}</span></h2><p class="metrics-note">Handler latency over the last five minutes.</p>"#, owners.len()).unwrap();
    if owners.is_empty() {
        content.push_str(r#"<p class="empty">No deployed applications.</p></section>"#);
        return;
    }
    content.push_str(
        r#"<div class="application-table"><table>
  <caption class="visually-hidden">Applications, deployment status, routes, and keyspace file sizes</caption>
  <thead><tr><th scope="col">Name</th><th scope="col">Status</th><th scope="col">Path prefix</th><th scope="col" class="functions-column">Functions</th><th scope="col">Keyspace file bytes</th><th scope="col">SQL database</th></tr></thead>"#,
    );
    for (name, (application, store, database)) in owners {
        render_application(content, name, application, store, database);
    }
    content.push_str("</table></div></section>");
}

fn render_application(
    content: &mut String,
    name: &str,
    application: Option<&ApplicationInfo>,
    store: Option<&StoreInfo>,
    sql: Option<&SqlInfo>,
) {
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
            write!(content, r#"<li><a href="/applications/{}/handlers/{}"><strong>{}</strong></a><span><code>{}</code> <code>{}</code></span><span class="handler-latency">Min {} · Max {}</span></li>"#,
                escape(name), escape(&function.name), escape(&function.name),
                escape(&function.method), escape(&function.path),
                latency(function.metrics.latency_min_ms),
                latency(function.metrics.latency_max_ms)).unwrap();
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
    write!(content, "<td class=\"keyspace-bytes\">{file_bytes}</td>").unwrap();
    content.push_str("<td class=\"sql-database\">");
    if let Some(sql) = sql {
        let enabled = application.is_some_and(|app| app.sql);
        let bytes = sql
            .disk_bytes
            .map(|bytes| bytes.to_string())
            .unwrap_or_else(|| "Unavailable".into());
        let failures: u64 = sql.operations.values().map(|stats| stats.failures).sum();
        write!(
            content,
            "{} · Migration {} · {} bytes · {} failures",
            if enabled { "Enabled" } else { "Disabled" },
            sql.migration,
            bytes,
            failures
        )
        .unwrap();
    } else {
        content.push('—');
    }
    content.push_str("</td></tr></tbody>");
}

pub(super) fn handler(app: &ApplicationInfo, function: &FunctionInfo) -> String {
    let mut content = String::from(r#"<p><a href="/#applications">← Applications</a></p>"#);
    let metrics = &function.metrics;
    write!(content, r#"<section><p class="eyebrow">{}</p><h2>{} · Handler metrics</h2>
<p><code>{}</code> <code>{}</code> · Application prefix <code>{}</code></p>
<p>Rolling five-minute window · one-second resolution · current deployment</p>
<dl class="metrics-summary">
<div><dt>Completed calls</dt><dd>{}</dd></div>
<div><dt>Minimum latency</dt><dd>{}</dd></div>
<div><dt>Maximum latency</dt><dd>{}</dd></div>
<div><dt>Mean latency</dt><dd>{}</dd></div>
</dl>
<p class="metrics-note">Latency measures matched-handler dispatch, including execution and response validation. It excludes initial request upload and response delivery. Requests rejected before handler selection and cancelled dispatches are not counted.</p>
<h3>Response codes</h3>"#,
        escape(&app.name), escape(&function.name), escape(&function.method),
        escape(&function.path), escape(&app.path_prefix), metrics.calls,
        latency(metrics.latency_min_ms), latency(metrics.latency_max_ms),
        latency(metrics.latency_mean_ms)).unwrap();
    if metrics.calls == 0 {
        content.push_str("<p class=\"empty\">No completed calls in this window.</p>");
    } else {
        content.push_str("<table><caption class=\"visually-hidden\">Response code counts in this window</caption><thead><tr><th scope=\"col\">HTTP status</th><th scope=\"col\">Calls</th></tr></thead><tbody>");
        for (code, count) in &metrics.response_codes {
            write!(
                content,
                "<tr><th scope=\"row\">{}</th><td>{count}</td></tr>",
                escape(code)
            )
            .unwrap();
        }
        content.push_str("</tbody></table>");
    }
    content.push_str("</section>");
    page(&content)
}

pub(super) fn missing_handler() -> String {
    page(
        r#"<p><a href="/#applications">← Applications</a></p><section><h2>Handler not found</h2><p>This handler is not currently deployed.</p></section>"#,
    )
}

pub(super) fn unavailable(message: &str) -> String {
    page(&format!(
        r#"<section class="error" role="alert">
  <h2>Daemon metadata unavailable</h2>
  <p>{}</p>
  <p>Check that the selected daemon is running.</p>
</section>"#,
        escape(message)
    ))
}

fn page(content: &str) -> String {
    PAGE_TEMPLATE.replace("<!-- content -->", content)
}

fn latency(value: Option<f64>) -> String {
    value
        .map(|ms| format!("{ms:.3} ms"))
        .unwrap_or_else(|| "—".into())
}

fn render_database(content: &mut String, storage: &StorageInfo) {
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
