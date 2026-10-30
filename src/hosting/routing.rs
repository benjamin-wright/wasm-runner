use std::sync::Arc;

use anyhow::{Context, Result};
use rocket::{Route, http::uri::Origin, local::asynchronous::Client};

use super::server::{Handler, RegisteredHandler, State};
use crate::runtime::Runtime;

pub(super) fn prepare(handlers: Vec<Handler>, timeout: std::time::Duration) -> Result<Vec<Route>> {
    let mut runtime = Runtime::new(timeout)?;
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
    Ok(prepared
        .into_iter()
        .map(|(handler, method, module)| {
            let mut route = Route::new(
                method,
                &handler.path,
                RegisteredHandler {
                    name: handler.name.clone(),
                    module: Arc::new(module),
                    runtime: runtime.clone(),
                },
            );
            route.name = Some(handler.name.into());
            route
        })
        .collect())
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
