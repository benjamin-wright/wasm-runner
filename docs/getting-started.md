# Build your first application

This guide takes the checked-in example from Rust source to WebAssembly
components and a running HTTP server. You need a local checkout of this repository
and a shell. Run every command below from the **repository root**. Build and serving are separate steps; restart local serving after rebuilding.

## Prerequisites and installation

Install [rustup](https://rust-lang.github.io/rustup/installation/index.html), and
ensure `rustup` and `cargo` are on your `PATH`. You also need `make`. Production Git reads require Git on `PATH`; Git is not needed by `wr run`. From this
checkout, run:

```sh
make setup
make install
wr --help
```

`make setup` uses [rust-toolchain.toml](../rust-toolchain.toml) to install the
pinned Rust toolchain, Clippy, rustfmt, and `wasm32-wasip2` target. `make install`
builds a release binary using the repository's [Cargo.lock](../Cargo.lock) and
installs `wr` into Cargo's binary directory (normally `~/.cargo/bin`). Ensure
that directory is on your `PATH`. Help should list `setup`, `build`, `update`, `clean`, `run`, `package`, and `start`.

Repeat `make install` to refresh the installed command after changes. Use
`cargo uninstall wasm-runner` to remove it. Repository verification and the
development loop are described in [Contributing](../CONTRIBUTING.md#development-workflow).

For applications outside this checkout, configure your chosen Rust toolchain
with the `wasm32-wasip2` target before building. `wr` does not install build
prerequisites. The repository toolchain configuration applies to the example
because it lives beneath this checkout.

## Inspect the example

The [examples directory](../examples) is an application directory:

- [application.yaml](../examples/application.yaml) names the application `hello`, mounts it at `/demo`,
  and configures greeting, JSON echo, request-inspection, and deliberately
  misbehaving routes.
- [functions/greet.rs](../examples/functions/greet.rs) returns a greeting.
- [functions/echo.rs](../examples/functions/echo.rs) parses and returns JSON.
- [functions/inspect.rs](../examples/functions/inspect.rs) returns decoded path
  captures and grouped query values.
- [functions/misbehave.rs](../examples/functions/misbehave.rs) deliberately fails
  for [manual server checks](guides.md#manually-exercise-failures-and-concurrency).

Each source defines `handle`, taking a generated `Request` and returning a
generated `Response`. The [guest contract](../wit/README.md) explains the types.
The runner generates the Cargo workspace and binding code around these sources.

## Build the components

You can inspect the proposed workspace without writing it:

```sh
wr setup examples --dry-run
```

Expect `Workspace preview for hello`, a sorted file list, and the proposed
contents. Then build:

```sh
wr build examples
ls examples/.wr/components
```

Build performs setup automatically, so a separate `wr setup examples` is
optional. Cargo prints compilation diagnostics; successful publication ends
with lines shaped like these, where `<checkout>` is your repository path:

```text
Built <checkout>/examples/.wr/components/echo.wasm
Built <checkout>/examples/.wr/components/greet.wasm
```

The directory listing should contain `echo.wasm`, `greet.wasm`,
`inspect.wasm`, and `misbehave.wasm`. Build also
generates a root Cargo manifest inside the application, wrappers under `.wr`,
and artifacts under `target`. It merges ignore rules and editor settings.
A handwritten application-root Cargo manifest is protected from overwrite.
See [generated files](reference.md#generated-files) for ownership details.

Builds use the application's locked dependency versions by default. The example
includes `Cargo.lock`; for a new application, run `wr update <DIRECTORY>` before
its first build to create the lockfile. Use update again when deliberately
refreshing dependency versions or adding/removing functions, then review the
lockfile changes and rebuild. See [dependency updates](reference.md#update).

## Serve the application

```sh
wr run examples --port 9000 --admin-listen 127.0.0.1:9001
```

Wait for `Listening on http://127.0.0.1:9000`, then use another terminal:

```sh
curl -i http://127.0.0.1:9000/demo/hello
```

Expect status 200 and `Hello, world!`. Open <http://127.0.0.1:9001> for live
application, storage, and handler information. Stop the process with Ctrl-C.
Local storage remains beneath `examples/.wr-data/` across restarts.

## Make a small change

Edit a handler, stop local serving, run `wr build examples`, and start the same
`wr run` command again. Build failures leave previously built components intact.
There is no local file watcher or automatic build on run.

For automatic production updates from public Git repositories and OCI registries,
follow [Manage multiple applications](guides.md#manage-multiple-applications).
To provision and supervise the runner itself, use the [operations guide](operations.md).
