use std::sync::Arc;

use anyhow::{Context, Result};
use rocket::{Route, http::uri::Origin, local::asynchronous::Client};

use super::{
    metrics::HandlerMetrics,
    server::{Handler, RegisteredHandler, State},
};
use crate::runtime::{Capabilities, Runtime};

pub(super) struct Prepared {
    pub routes: Vec<Route>,
    pub metrics: HandlerMetrics,
}

pub(super) fn prepare(
    handlers: Vec<Handler>,
    timeout: std::time::Duration,
    capabilities: Capabilities,
) -> Result<Prepared> {
    let mut runtime = Runtime::with_capabilities(timeout, capabilities)?;
    let mut prepared = Vec::new();
    for handler in handlers {
        Origin::parse_route(&handler.path).map_err(|error| {
            anyhow::anyhow!(
                "handler {}: invalid route {}: {error}",
                handler.name,
                handler.path
            )
        })?;
        let method = handler
            .method
            .parse()
            .map_err(|()| anyhow::anyhow!("invalid method {}", handler.method))?;
        let module = runtime
            .register(&handler.component)
            .with_context(|| format!("registering handler {} ({})", handler.name, handler.path))?;
        prepared.push((handler, method, module));
    }
    let runtime = Arc::new(runtime);
    let mut metrics = HandlerMetrics::new();
    let routes = prepared
        .into_iter()
        .map(|(handler, method, module)| {
            let mut route = Route::new(
                method,
                &handler.path,
                RegisteredHandler {
                    name: handler.name.clone(),
                    metrics: metrics.entry(handler.name.clone()).or_default().clone(),
                    module: Arc::new(module),
                    runtime: runtime.clone(),
                },
            );
            route.name = Some(handler.name.into());
            route
        })
        .collect();
    Ok(Prepared { routes, metrics })
}

pub(super) async fn dispatcher(routes: Vec<Route>, state: Arc<State>) -> Result<Client> {
    let rocket = rocket::custom(rocket::Config {
        log_level: rocket::config::LogLevel::Off,
        shutdown: rocket::config::Shutdown {
            ctrlc: false,
            #[cfg(unix)]
            signals: Default::default(),
            ..Default::default()
        },
        ..rocket::Config::default()
    })
    .manage(state)
    .mount("/", routes);
    Client::untracked(rocket)
        .await
        .map_err(|error| anyhow::anyhow!("preparing HTTP routes: {error}: {:?}", error.kind()))
}
