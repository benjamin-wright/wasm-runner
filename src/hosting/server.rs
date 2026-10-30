//! HTTP transport and bounded dispatch for reusable component runtimes.
use std::{
    collections::HashMap,
    future::Future,
    io::Cursor,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use http::header::{HeaderName, HeaderValue};
use rocket::{
    Data, Request as HttpRequest, Response as HttpResponse, Route,
    data::ToByteUnit,
    fairing::AdHoc,
    http::{Method, Status},
    route::Outcome,
};
use tokio::sync::Semaphore;

use super::applications::Applications;
use crate::runtime::{ExecutionError, ModuleId, Request, Response, Runtime};

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    pub execution_timeout: Duration,
    pub max_concurrent_executions: usize,
    pub max_request_body: usize,
    pub request_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: ([127, 0, 0, 1], 8080).into(),
            execution_timeout: Duration::from_secs(5),
            max_concurrent_executions: 16,
            max_request_body: 1024 * 1024,
            request_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    pub name: String,
    pub method: String,
    pub path: String,
    pub component: Vec<u8>,
}

pub(super) struct State {
    config: Config,
    slots: Arc<Semaphore>,
    pub(super) stopping: AtomicBool,
}

#[derive(Clone)]
pub(super) struct RegisteredHandler {
    pub(super) name: String,
    pub(super) module: Arc<ModuleId>,
    pub(super) runtime: Arc<Runtime>,
}

/// Prepare and serve components inside a Tokio runtime. Rocket validates route
/// collisions before binding; `on_ready` receives the bound address at liftoff.
/// The caller owns shutdown signals. Return only after admitted jobs finish.
pub async fn serve(
    config: Config,
    handlers: Vec<Handler>,
    shutdown: impl Future<Output = ()>,
    on_ready: impl FnOnce(SocketAddr) + Send + Sync + 'static,
) -> Result<()> {
    let applications = Applications::new(config.clone())?;
    applications
        .install("application".into(), "/".into(), handlers)
        .await?;
    serve_applications(config, applications, shutdown, on_ready).await
}

pub(crate) async fn serve_applications(
    config: Config,
    applications: Arc<Applications>,
    shutdown: impl Future<Output = ()>,
    on_ready: impl FnOnce(SocketAddr) + Send + Sync + 'static,
) -> Result<()> {
    let state = applications.state.clone();
    let routes = [
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
        Method::Trace,
        Method::Connect,
    ]
    .into_iter()
    .map(|method| Route::new(method, "/<path..>", Gateway))
    .collect::<Vec<_>>();
    // Configure directly so Rocket.toml and environment variables cannot override
    // the embedding application's settings or install process signal handlers.
    let rocket = rocket::custom(rocket::Config {
        address: config.listen.ip(),
        port: config.listen.port(),
        log_level: rocket::config::LogLevel::Off,
        shutdown: rocket::config::Shutdown {
            ctrlc: false,
            #[cfg(unix)]
            signals: Default::default(),
            grace: config
                .request_timeout
                .saturating_add(config.execution_timeout)
                .as_secs()
                .saturating_add(2)
                .min(u32::MAX as u64) as u32,
            ..Default::default()
        },
        ..rocket::Config::default()
    })
    .manage(applications.clone())
    .mount("/", routes)
    .attach(AdHoc::on_liftoff("Report readiness", move |rocket| {
        Box::pin(async move {
            on_ready(SocketAddr::new(
                rocket.config().address,
                rocket.config().port,
            ));
        })
    }))
    .ignite()
    .await
    .map_err(|error| anyhow::anyhow!("preparing HTTP server: {error}: {:?}", error.kind()))?;

    let handle = rocket.shutdown();
    let server = rocket.launch();
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result,
        () = shutdown => {
            applications.stop();
            handle.notify();
            server.await
        }
    };
    applications.stop();
    let _drained = state
        .slots
        .acquire_many(state.config.max_concurrent_executions as u32)
        .await?;
    result.map(|_| ()).context("running HTTP server")
}

impl State {
    pub(super) fn new(config: Config) -> Result<Arc<Self>> {
        ensure!(
            config.max_concurrent_executions > 0
                && config.max_concurrent_executions <= u32::MAX as usize,
            "invalid execution concurrency"
        );
        ensure!(
            !config.request_timeout.is_zero(),
            "request timeout must be positive"
        );
        ensure!(
            !config.execution_timeout.is_zero(),
            "execution timeout must be positive"
        );
        Ok(Arc::new(Self {
            slots: Arc::new(Semaphore::new(config.max_concurrent_executions)),
            config,
            stopping: AtomicBool::new(false),
        }))
    }
}

#[derive(Clone)]
struct Gateway;

#[rocket::async_trait]
impl rocket::route::Handler for Gateway {
    async fn handle<'r>(&self, request: &'r HttpRequest<'_>, data: Data<'r>) -> Outcome<'r> {
        let applications = request
            .rocket()
            .state::<Arc<Applications>>()
            .expect("applications");
        let state = &applications.state;
        if state.stopping.load(Ordering::SeqCst) {
            return Outcome::Success(empty_response(Status::ServiceUnavailable));
        }
        let Some((application, mut uri)) = applications.select(request.uri().path().as_str())
        else {
            return Outcome::Forward((data, Status::NotFound));
        };
        if let Some(query) = request.uri().query() {
            uri.push('?');
            uri.push_str(query.as_str());
        }
        let body = match read_body(data, &state.config).await {
            Ok(body) => body,
            Err(status) => return Outcome::Success(empty_response(status)),
        };
        let mut local = application.client.req(request.method(), uri);
        for header in request.headers().iter() {
            local = local.header(rocket::http::Header::new(
                header.name().to_string(),
                header.value().to_string(),
            ));
        }
        let response = local.body(body).dispatch().await;
        let mut builder = HttpResponse::build();
        builder.status(response.status());
        for header in response.headers().iter() {
            builder.raw_header_adjoin(header.name().to_string(), header.value().to_string());
        }
        // Local dispatch strips HEAD bodies but retains their representation size.
        let size = response.body().preset_size();
        let body = response.into_bytes().await.unwrap_or_default();
        builder.sized_body(size.unwrap_or(body.len()), Cursor::new(body));
        Outcome::Success(builder.finalize())
    }
}

async fn read_body(data: Data<'_>, config: &Config) -> Result<Vec<u8>, Status> {
    match tokio::time::timeout(
        config.request_timeout,
        data.open((config.max_request_body as u64).bytes())
            .into_bytes(),
    )
    .await
    {
        Err(_) => Err(Status::RequestTimeout),
        Ok(Err(_)) => Err(Status::BadRequest),
        Ok(Ok(body)) if !body.is_complete() => Err(Status::PayloadTooLarge),
        Ok(Ok(body)) => Ok(body.into_inner()),
    }
}

#[rocket::async_trait]
impl rocket::route::Handler for RegisteredHandler {
    async fn handle<'r>(&self, request: &'r HttpRequest<'_>, data: Data<'r>) -> Outcome<'r> {
        let state = request
            .rocket()
            .state::<Arc<State>>()
            .expect("server state")
            .clone();
        Outcome::Success(dispatch(request, data, state, self.clone()).await)
    }
}

async fn dispatch(
    request: &HttpRequest<'_>,
    data: Data<'_>,
    state: Arc<State>,
    handler: RegisteredHandler,
) -> HttpResponse<'static> {
    if state.stopping.load(Ordering::SeqCst) {
        return empty_response(Status::ServiceUnavailable);
    }
    let body = match read_body(data, &state.config).await {
        Ok(body) => body,
        Err(status) => return empty_response(status),
    };
    let headers = request
        .headers()
        .iter()
        .map(|header| (header.name().to_string(), header.value().to_owned()))
        .collect();
    let mut query: Vec<(String, Vec<String>)> = Vec::new();
    let mut positions = HashMap::new();
    let raw_query = request
        .uri()
        .query()
        .map(|query| query.as_str())
        .unwrap_or("");
    for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
        let next = query.len();
        let position = *positions.entry(key.to_string()).or_insert_with(|| {
            query.push((key.to_string(), Vec::new()));
            next
        });
        query[position].1.push(value.into_owned());
    }
    // Rocket owns matching and decoding. Only adapt its named captures to WIT;
    // trailing captures are represented as decoded segments joined with '/'.
    let mut path_params = Vec::new();
    if let Some(route) = request.route() {
        for (index, segment) in route
            .uri
            .origin
            .path()
            .raw_segments()
            .filter(|s| !s.is_empty())
            .enumerate()
        {
            if let Some(name) = segment.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                let (name, value) = if let Some(name) = name.strip_suffix("..") {
                    (
                        name,
                        request
                            .routed_segments(index..)
                            .collect::<Vec<_>>()
                            .join("/"),
                    )
                } else {
                    (name, request.routed_segment(index).unwrap_or("").to_owned())
                };
                if name != "_" {
                    path_params.push((name.to_string(), value));
                }
            }
        }
    }
    let guest_request = Request {
        method: request.method().to_string(),
        path: request.uri().path().as_str().to_owned(),
        path_params,
        query,
        headers,
        body,
    };
    let Ok(slot) = state.slots.clone().try_acquire_owned() else {
        return empty_response(Status::ServiceUnavailable);
    };
    if state.stopping.load(Ordering::SeqCst) {
        return empty_response(Status::ServiceUnavailable);
    }
    let name = handler.name;
    let execution = tokio::task::spawn_blocking(move || {
        // The blocking job owns admission even if its request future is dropped.
        let _slot = slot;
        handler.runtime.execute(&handler.module, &guest_request)
    })
    .await;
    match execution {
        Ok(Ok(response)) => match translate_response(response) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("handler {name}: invalid response: {error:#}");
                empty_response(Status::InternalServerError)
            }
        },
        Ok(Err(error)) => {
            eprintln!("handler {name}: {error}");
            empty_response(if matches!(error, ExecutionError::Timeout) {
                Status::GatewayTimeout
            } else {
                Status::InternalServerError
            })
        }
        Err(error) => {
            eprintln!("handler {name}: blocking execution failed: {error}");
            empty_response(Status::InternalServerError)
        }
    }
}

fn empty_response(status: Status) -> HttpResponse<'static> {
    HttpResponse::build().status(status).finalize()
}

fn translate_response(response: Response) -> Result<HttpResponse<'static>> {
    ensure!(
        (200..=599).contains(&response.status),
        "invalid final HTTP status {}",
        response.status
    );
    let mut builder = HttpResponse::build();
    builder.status(Status::new(response.status));
    let headers = response
        .headers
        .into_iter()
        .map(|(name, value)| Ok((HeaderName::try_from(name)?, HeaderValue::try_from(value)?)))
        .collect::<Result<Vec<_>>>()?;
    let connection_headers = headers
        .iter()
        .filter(|(name, _)| name == "connection")
        .flat_map(|(_, value)| {
            value
                .to_str()
                .unwrap_or("")
                .split(',')
                .map(|name| name.trim().to_ascii_lowercase())
        })
        .collect::<Vec<_>>();
    for (name, value) in headers {
        if !matches!(
            name.as_str(),
            "connection"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
                | "content-length"
        ) && !connection_headers
            .iter()
            .any(|header| header == name.as_str())
        {
            builder.raw_header_adjoin(name.to_string(), value.to_str()?.to_owned());
        }
    }
    if !matches!(response.status, 204 | 205 | 304) {
        builder.sized_body(response.body.len(), Cursor::new(response.body));
    }
    Ok(builder.finalize())
}
