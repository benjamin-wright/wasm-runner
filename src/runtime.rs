//! Synchronous, isolated execution of HTTP components, independent of transport.
use std::{
    fmt,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use wasmtime::{
    Config, Engine, Store, Trap,
    component::{Component, Linker, Resource, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "http-function" });

pub use exports::local::http::handler::{Request, Response};

const EPOCH_INTERVAL: Duration = Duration::from_millis(5);

/// Identifies a registration in its originating runtime only.
#[derive(Clone, Debug)]
pub struct ModuleId {
    owner: Arc<()>,
    index: usize,
}

#[derive(Debug)]
pub enum ExecutionError {
    InvalidModule,
    Timeout,
    Guest(anyhow::Error),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModule => write!(f, "module ID does not belong to this runtime"),
            Self::Timeout => write!(f, "component execution timed out"),
            Self::Guest(error) => write!(f, "component execution failed: {error:#}"),
        }
    }
}

impl std::error::Error for ExecutionError {}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    started: Instant,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

pub struct Runtime {
    engine: Engine,
    modules: Vec<HttpFunctionPre<HostState>>,
    owner: Arc<()>,
    timeout: Duration,
    stop: mpsc::Sender<()>,
    ticker: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new(timeout: Duration) -> Result<Self> {
        ensure!(!timeout.is_zero(), "execution timeout must be positive");
        ensure!(
            timeout.as_millis() < u64::MAX as u128,
            "execution timeout is too large"
        );
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let ticker_engine = engine.clone();
        let (stop, stopped) = mpsc::channel();
        let ticker = thread::Builder::new()
            .name("wr-epoch".into())
            .spawn(move || {
                let mut next_tick = Instant::now() + EPOCH_INTERVAL;
                while matches!(
                    stopped.recv_timeout(next_tick.saturating_duration_since(Instant::now())),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    ticker_engine.increment_epoch();
                    next_tick += EPOCH_INTERVAL;
                }
            })
            .context("starting runtime epoch ticker")?;
        Ok(Self {
            engine,
            modules: Vec::new(),
            owner: Arc::new(()),
            timeout,
            stop,
            ticker: Some(ticker),
        })
    }

    /// Compile, resolve imports, and validate the complete versioned guest contract.
    pub fn register(&mut self, bytes: &[u8]) -> Result<ModuleId> {
        let component = Component::new(&self.engine, bytes).context("compiling component")?;
        for (name, _) in component.component_type().imports(&self.engine) {
            let interface = name.split('@').next().unwrap_or(name);
            ensure!(
                name.split_once('@').is_some_and(|(_, version)| version
                    .strip_prefix("0.2.")
                    .is_some_and(
                        |patch| !patch.is_empty() && patch.bytes().all(|c| c.is_ascii_digit())
                    )),
                "unsupported WASI import version: {name}"
            );
            if !matches!(
                interface,
                "wasi:cli/environment"
                    | "wasi:cli/exit"
                    | "wasi:cli/stdin"
                    | "wasi:cli/stdout"
                    | "wasi:cli/stderr"
                    | "wasi:cli/terminal-input"
                    | "wasi:cli/terminal-output"
                    | "wasi:cli/terminal-stdin"
                    | "wasi:cli/terminal-stdout"
                    | "wasi:cli/terminal-stderr"
                    | "wasi:io/error"
                    | "wasi:io/streams"
                    | "wasi:io/poll"
                    | "wasi:clocks/monotonic-clock"
            ) {
                bail!("unsupported component import: {name}");
            }
        }
        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        // Rust imports this interface even without explicit timers. Replacing it
        // keeps timer waits from escaping the guest's epoch deadline in host code.
        linker.allow_shadowing(true);
        for (name, _) in component
            .component_type()
            .imports(&self.engine)
            .filter(|(name, _)| name.starts_with("wasi:clocks/monotonic-clock@"))
        {
            let mut clock = linker.instance(name)?;
            clock.func_wrap(
                "now",
                |store: wasmtime::StoreContextMut<'_, HostState>, (): ()| {
                    Ok((store
                        .data()
                        .started
                        .elapsed()
                        .as_nanos()
                        .min(u64::MAX as u128) as u64,))
                },
            )?;
            clock.func_wrap("resolution", |_, (): ()| Ok((1_000_000u64,)))?;
            for name in ["subscribe-duration", "subscribe-instant"] {
                clock.func_wrap(
                    name,
                    |_, (_time,): (u64,)| -> Result<(Resource<wasmtime_wasi::p2::DynPollable>,)> {
                        bail!("WASI timer subscriptions are not supported")
                    },
                )?;
            }
        }
        let prepared = HttpFunctionPre::new(linker.instantiate_pre(&component)?)
            .context("component must export local:http/handler@0.2.0; rebuild with wr build")?;
        let id = ModuleId {
            owner: self.owner.clone(),
            index: self.modules.len(),
        };
        self.modules.push(prepared);
        Ok(id)
    }

    /// The allowance begins here, not when the caller queues a blocking job.
    pub fn execute(
        &self,
        module: &ModuleId,
        request: &Request,
    ) -> Result<Response, ExecutionError> {
        let started = Instant::now();
        if !Arc::ptr_eq(&self.owner, &module.owner) {
            return Err(ExecutionError::InvalidModule);
        }
        let prepared = self
            .modules
            .get(module.index)
            .ok_or(ExecutionError::InvalidModule)?;
        let state = HostState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            started,
        };
        let mut store = Store::new(&self.engine, state);
        let remaining = self
            .timeout
            .checked_sub(started.elapsed())
            .ok_or(ExecutionError::Timeout)?;
        // One extra tick prevents an invocation just before a tick from expiring early.
        store.set_epoch_deadline(
            remaining.as_millis().div_ceil(EPOCH_INTERVAL.as_millis()) as u64 + 1,
        );
        store.epoch_deadline_trap();
        let result = prepared.instantiate(&mut store).and_then(|instance| {
            instance
                .local_http_handler()
                .call_handle(&mut store, request)
        });
        if started.elapsed() >= self.timeout
            || result
                .as_ref()
                .err()
                .is_some_and(|error| error.downcast_ref::<Trap>() == Some(&Trap::Interrupt))
        {
            return Err(ExecutionError::Timeout);
        }
        result.map_err(ExecutionError::Guest)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}
