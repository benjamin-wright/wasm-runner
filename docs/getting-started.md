# Build your first application

This guide takes the checked-in example from Rust source to WebAssembly
components and a running HTTP server. You need a local checkout of this repository
and a shell. Run every command below from the **repository root**. Build and run are separate steps; restart the server after rebuilding.

## Prerequisites and installation

Install [rustup](https://rust-lang.github.io/rustup/installation/index.html), and
ensure `rustup` and `cargo` are on your `PATH`. You also need `make`. From this
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
that directory is on your `PATH`. Help should list `setup`, `build`, `clean`,
and `run`.

Repeat `make install` to refresh the installed command after changes. Use
`cargo uninstall wasm-runner` to remove it. Repository verification and the
development loop are described in [Contributing](../CONTRIBUTING.md#development-workflow).

For applications outside this checkout, configure your chosen Rust toolchain
with the `wasm32-wasip2` target before building. `wr` does not install build
prerequisites. The repository toolchain configuration applies to the example
because it lives beneath this checkout.

## Inspect the example

The [examples directory](../examples) is an application directory:

- [application.yaml](../examples/application.yaml) names the application `hello`
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

## Serve the application

```sh
wr run examples --listen 127.0.0.1:9000
```

After all components have been prepared and the listener bound, expect:

```text
Application: hello
Listening on http://127.0.0.1:9000
```

In another terminal:

```sh
curl http://127.0.0.1:9000/hello
curl -H 'Content-Type: application/json' -d '{"hello":"world"}' http://127.0.0.1:9000/echo
curl 'http://127.0.0.1:9000/users/42/orders?status=open&tag=a&tag=b'
```

Expect `Hello, world!`, the echoed JSON object, and a JSON request description
with `user_id` equal to `42` and the two `tag` values grouped together.
Press Ctrl-C to stop accepting work and drain admitted executions.
If components are missing, run the build step first. See the
[server reference](reference.md#run) for limits and errors.

## Make a small change

Change the greeting bytes in [greet.rs](../examples/functions/greet.rs), then run
`wr build examples` again. The component is rebuilt and published at the same
path. Stop and restart `wr run examples`, then call `/hello` to see the new response.

Continue with [Common tasks](guides.md) to add a function, configure dependencies,
prepare VS Code, make a locked build, or clean generated files.
