# Build your first application

This guide takes the checked-in example from Rust source to WebAssembly
components and a running HTTP server. You need a local checkout of this repository
and a shell. Run every command below from the **repository root**. Build and deployment are separate steps; redeploy after rebuilding. The daemon workflow requires Unix.

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
that directory is on your `PATH`. Help should list `setup`, `build`, `update`, `clean`, `run`,
`daemon`, `deploy`, `apps`, and `remove`.

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

Builds use the application's locked dependency versions by default. The example
includes `Cargo.lock`; for a new application, run `wr update <DIRECTORY>` before
its first build to create the lockfile. Use update again when deliberately
refreshing dependency versions or adding/removing functions, then review the
lockfile changes and rebuild. See [dependency updates](reference.md#update).

## Serve the application

```sh
wr daemon start --listen 127.0.0.1:9000
wr deploy examples
wr daemon status
```

Start returns after both listeners are bound. Deploy returns after all routes and
components are prepared and active. The output includes:

```text
Daemon started
Listening on http://127.0.0.1:9000
...
Deployed hello
```

Then call the hosting listener:

```sh
curl http://127.0.0.1:9000/hello
curl -H 'Content-Type: application/json' -d '{"hello":"world"}' http://127.0.0.1:9000/echo
curl 'http://127.0.0.1:9000/users/42/orders?status=open&tag=a&tag=b'
```

Expect `Hello, world!`, the echoed JSON object, and a JSON request description
with `user_id` equal to `42` and the two `tag` values grouped together.
Run `wr daemon stop` to stop accepting work and drain admitted executions.
Starting again begins with no applications; run `wr deploy examples` again.
For foreground serving, use `wr run examples --listen 127.0.0.1:9000` instead
of the daemon commands and stop it with Ctrl-C.
If components are missing, run the build step first. See the
[server reference](reference.md#run) for limits and errors.

## Make a small change

Change the greeting bytes in [greet.rs](../examples/functions/greet.rs), then run
`wr build examples` again. The component is rebuilt and published at the same
path. With the daemon running, run `wr deploy examples`, then call `/hello`
to see the new response. A failed deployment leaves the previous application active.
See [managing multiple applications](guides.md#manage-multiple-applications).

Continue with [Common tasks](guides.md) to add a function, configure dependencies,
prepare VS Code, make a locked build, or clean generated files.
