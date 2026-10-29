# Build your first application

This guide takes the checked-in example from Rust source to WebAssembly
components and validated metadata. You need a local checkout of this repository
and a shell. Run every command below from the **repository root**. No HTTP
endpoint will be available at the end: execution and serving are not implemented.

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
  and configures `GET /hello` for `greet` and `POST /echo` for `echo`.
- [functions/greet.rs](../examples/functions/greet.rs) returns a greeting.
- [functions/echo.rs](../examples/functions/echo.rs) parses and returns JSON,
  using its function-specific `serde_json` dependency.

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

The directory listing should contain `echo.wasm` and `greet.wasm`. Build also
generates a root Cargo manifest inside the application, wrappers under `.wr`,
and artifacts under `target`. It merges ignore rules and editor settings.
A handwritten application-root Cargo manifest is protected from overwrite.
See [generated files](reference.md#generated-files) for ownership details.

## Inspect the application metadata

```sh
wr run examples --listen 127.0.0.1:9000
```

Expected output, with your checkout path substituted:

```text
Application: hello
Manifest: <checkout>/examples/application.yaml
Configured listener: 127.0.0.1:9000
Functions: 2
POST    /echo -> echo (<checkout>/examples/.wr/components/echo.wasm)
GET     /hello -> greet (<checkout>/examples/.wr/components/greet.wasm)
Metadata only; component execution and HTTP serving are not implemented yet.
```

Success means the two component files exist and the manifest can be inspected.
The `run` command validates metadata without opening components or binding the
listener; it can also run before a build. There is no endpoint to call yet.

## Make a small change

Change the greeting bytes in [greet.rs](../examples/functions/greet.rs), then run
`wr build examples` again. The component is rebuilt and published at the same
path. Metadata will still show the same route; it does not display the response
body or execute your handler.

Continue with [Common tasks](guides.md) to add a function, configure dependencies,
prepare VS Code, make a locked build, or clean generated files.
