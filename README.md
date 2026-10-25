# WASM RUNNER

A small, standalone WebAssembly application server: one manifest, one component
per function, and HTTP endpoints. The command is `wr`.

This repository contains a Rust CLI skeleton alongside the specification and
guest contract. The `run` command parses its arguments and exits with a clear
not-implemented error. It does not yet load manifests, start a server, or include
compiled example components.

## Local setup

Install [rustup](https://rust-lang.github.io/rustup/installation/index.html) and ensure its commands are on your `PATH`.

From the repository directory:

```sh
make setup
make check
make install
wr --help
wr run application.yaml --listen 127.0.0.1:9000
```

`make setup` installs the Rust version, Clippy, and rustfmt specified in
[rust-toolchain.toml](rust-toolchain.toml). `make install` builds a release binary
from this checkout using the existing [Cargo.lock](Cargo.lock) and installs `wr`
into Cargo's binary directory (normally `~/.cargo/bin`). Ensure that directory is
on your `PATH`. Repeat `make install` after changes to refresh the installed
command; use `cargo uninstall wasm-runner` to remove it.

Arguments after `wr` go directly to the application. `run` requires a manifest
path. The listener defaults to `127.0.0.1:8080` and accepts numeric IPv4 or
bracketed IPv6 addresses with a port. Hostnames are rejected. Configuration is
taken only from command-line arguments. Currently, `wr run` exits with the
expected not-implemented error; `wr --help` and `make check` work without guest
components.

## CLI development

| Target | Purpose |
| --- | --- |
| `make setup` | Install the pinned Rust toolchain and components |
| `make update-deps` | Create or update the lockfile to the latest compatible dependencies |
| `make build` (or `make`) | Compile a debug CLI for local development |
| `make install` | Build and install `wr` from this checkout |
| `make check` | Check formatting, lint, and run tests |

For a quick edit/build/run loop, invoke the newly built binary directly:

```sh
make build
./target/debug/wr --help
./target/debug/wr run application.yaml --listen 127.0.0.1:9000
make check
```

This uses the debug build in the checkout; the installed `wr` is updated only by
`make install`. There is no `make run` wrapper or `ARGS` variable.

Alternatively, build and run in one Cargo command:

```sh
cargo run --locked -- run application.yaml --listen 127.0.0.1:9000
```

Cargo options go before `--`; everything after it is passed to `wr`.
Use `cargo fmt` to apply formatting changes or `cargo test --locked` to run just
the tests. Other Cargo operations can be invoked directly as needed.

Dependency updates are separate from setup: run `make update-deps` only to
deliberately refresh dependency versions, then review and commit the result.
Build, install, and check use `--locked` and never update the lockfile.

Argument definitions live in [src/cli.rs](src/cli.rs); command dispatch and error reporting
live in [src/main.rs](src/main.rs). The [src/run.rs](src/run.rs) placeholder is the entry point for future
application preparation and serving.

## Intended experience

Build your functions as WebAssembly components implementing the
[`http-function` world](wit/world.wit), then run:

```sh
wr run application.yaml
```

Once implemented, the server will validate the whole application before listening,
print its routes, and stay in the foreground. Edit, rebuild, and restart to change
the application.

```text
hello listening on http://127.0.0.1:8080
GET   /hello  -> greet
POST  /echo   -> echo
Ctrl-C to stop
```

## Design

The host will use Rust, Wasmtime, and Tokio/Hyper. Each request gets a fresh
component instance with no host imports. Requests and responses cross the
component boundary as complete, buffered values through our custom WIT contract.

Read the [technology and architecture decisions](ARCHITECTURE.md), the
[first-iteration spec](SPEC.md), the [example manifest](application.yaml), and the
[guest contract notes](wit/README.md). The [WIT world](wit/world.wit) is
authoritative for the guest interface; the spec defines host behavior.
