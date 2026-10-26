# WASM RUNNER

A small, standalone WebAssembly application server: one manifest, one component
per function, and HTTP endpoints. The command is `wr`.

This repository contains a Rust CLI alongside the specification and guest
contract. The `run` command loads and validates an application YAML manifest,
prints its metadata, and exits successfully. It does not yet load WebAssembly
components or start an HTTP server.

## Local setup

Install [rustup](https://rust-lang.github.io/rustup/installation/index.html) and ensure its commands are on your `PATH`.

From the repository directory:

```sh
make setup
make check
make install
wr --help
wr run examples/application.yaml --listen 127.0.0.1:9000
```

`make setup` installs the Rust version, Clippy, and rustfmt specified in
[rust-toolchain.toml](rust-toolchain.toml). `make install` builds a release binary
from this checkout using the existing [Cargo.lock](Cargo.lock) and installs `wr`
into Cargo's binary directory (normally `~/.cargo/bin`). Ensure that directory is
on your `PATH`. Repeat `make install` after changes to refresh the installed
command; use `cargo uninstall wasm-runner` to remove it.

Arguments after `wr` go directly to the application. `run` requires a manifest
path. The listener defaults to `127.0.0.1:8080` and accepts numeric IPv4 or
bracketed IPv6 addresses with a port. Hostnames are rejected. Application settings
come from the manifest; the listener setting comes from the command line.
Environment variables do not alter configuration.

`wr run` reports the application name, manifest path, configured listener,
function count, routes in function-name order, and resolved module paths.
Relative module paths resolve against the manifest's directory. Invalid manifests
exit nonzero with an error before printing metadata. The listener is not bound and module files are not
opened, so the [example manifest](examples/application.yaml) works without compiled guests.

The manifest's data contract is [application.schema.json](application.schema.json),
a JSON Schema draft-07 document that can also be used by editor tooling. The
schema is embedded in `wr`; it does not need to accompany the manifest at runtime.
YAML anchors, aliases, and merge keys are resolved before schema validation.
Duplicate routes are checked after schema validation. Diagnostics use the detail
provided by the libraries, with basic context, rather than a prescribed
error-location format.

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
./target/debug/wr run examples/application.yaml --listen 127.0.0.1:9000
make check
```

This uses the debug build in the checkout; the installed `wr` is updated only by
`make install`. There is no `make run` wrapper or `ARGS` variable.

Alternatively, build and run in one Cargo command:

```sh
cargo run --locked -- run examples/application.yaml --listen 127.0.0.1:9000
```

Cargo options go before `--`; everything after it is passed to `wr`.
Use `cargo fmt` to apply formatting changes or `cargo test --locked` to run just
the tests. Other Cargo operations can be invoked directly as needed.

Dependency updates are separate from setup: run `make update-deps` only to
deliberately refresh dependency versions, then review and commit the result.
Build, install, and check use `--locked` and never update the lockfile.

Argument definitions live in [src/cli.rs](src/cli.rs); command dispatch and error reporting
live in [src/main.rs](src/main.rs). Manifest loading and validation live in
[src/manifest.rs](src/manifest.rs), which parses YAML with `yaml_serde`, validates
the resolved data with `jsonschema`, and deserializes directly into the returned
application types. Its `load` function resolves module paths and checks route
uniqueness before returning the application.
[src/run.rs](src/run.rs) prints metadata and is the entry point for future
application preparation and serving.

## Design

Read the [architecture](ARCHITECTURE.md), the
[current functionality spec](SPEC.md), and the
[example manifest](examples/application.yaml). The [guest interface notes](wit/README.md)
describe a WIT sketch that is not yet used by the executable. Runtime and HTTP
design details will be documented as those features are implemented.
