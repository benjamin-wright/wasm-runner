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
    http::{Status, uri::Origin},
    route::Outcome,
};
use tokio::sync::Semaphore;

use super::runtime::{ExecutionError, ModuleId, Request, Response, Runtime};

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

pub struct Handler {
    pub name: String,
    pub method: String,
    pub path: String,
    pub component: Vec<u8>,
}

struct State {
    runtime: Runtime,
    config: Config,
    slots: Arc<Semaphore>,
    stopping: AtomicBool,
}

#[derive(Clone)]
struct RegisteredHandler {
    name: String,
    module: Arc<ModuleId>,
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
    ensure!(
        config.max_concurrent_executions > 0
            && config.max_concurrent_executions <= u32::MAX as usize,
        "execution concurrency must be between 1 and {}",
        u32::MAX
    );
    ensure!(
        !config.request_timeout.is_zero(),
        "request timeout must be positive"
    );
    let mut runtime = Runtime::new(config.execution_timeout)?;
    let mut routes = Vec::new();
    for handler in handlers {
        // Route::new panics on invalid URIs; use Rocket's fallible parser first.
        Origin::parse_route(&handler.path).map_err(|error| {
            anyhow::anyhow!(
                "handler {}: invalid route {}: {error}",
                handler.name,
                handler.path
            )
        })?;
        let method = handler.method.parse().map_err(|()| {
            anyhow::anyhow!(
                "handler {}: invalid method {}",
                handler.name,
                handler.method
            )
        })?;
        let module = runtime
            .register(&handler.component)
            .with_context(|| format!("registering handler {} ({})", handler.name, handler.path))?;
        let mut route = Route::new(
            method,
            &handler.path,
            RegisteredHandler {
                name: handler.name.clone(),
                module: Arc::new(module),
            },
        );
        route.name = Some(handler.name.into());
        routes.push(route);
    }
    let state = Arc::new(State {
        runtime,
        slots: Arc::new(Semaphore::new(config.max_concurrent_executions)),
        stopping: AtomicBool::new(false),
        config: config.clone(),
    });
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
    .manage(state.clone())
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
            state.stopping.store(true, Ordering::SeqCst);
            handle.notify();
            server.await
        }
    };
    state.stopping.store(true, Ordering::SeqCst);
    let _drained = state
        .slots
        .acquire_many(state.config.max_concurrent_executions as u32)
        .await?;
    result.map(|_| ()).context("running HTTP server")
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
    let body = match tokio::time::timeout(
        state.config.request_timeout,
        data.open((state.config.max_request_body as u64).bytes())
            .into_bytes(),
    )
    .await
    {
        Err(_) => return empty_response(Status::RequestTimeout),
        Ok(Err(_)) => return empty_response(Status::BadRequest),
        Ok(Ok(body)) if !body.is_complete() => return empty_response(Status::PayloadTooLarge),
        Ok(Ok(body)) => body.into_inner(),
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
        state.runtime.execute(&handler.module, &guest_request)
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
