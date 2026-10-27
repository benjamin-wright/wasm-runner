# WASM RUNNER

A small, standalone WebAssembly application server: one manifest, one component
per function, and HTTP endpoints. The command is `wr`.

This repository contains a Rust CLI alongside the specification and guest
contract. The `run` command loads and validates an application YAML manifest,
prints its metadata, and exits successfully. It does not yet load WebAssembly
components or start an HTTP server. The `setup` command writes a generated
Cargo workspace for HTTP functions. Use `--dry-run` to preview file paths
and contents instead. The `build` command generates that workspace and uses
Cargo to compile WebAssembly components implementing the checked-in WIT world.

## Local setup

Install [rustup](https://rust-lang.github.io/rustup/installation/index.html) and ensure its commands are on your `PATH`.

From the repository directory:

```sh
make setup
make check
make install
wr --help
wr setup examples
wr build examples
wr run examples --listen 127.0.0.1:9000
```

`make setup` installs the Rust version, Clippy, rustfmt, and `wasm32-wasip2` target specified in
[rust-toolchain.toml](rust-toolchain.toml). `make install` builds a release binary
from this checkout using the existing [Cargo.lock](Cargo.lock) and installs `wr`
into Cargo's binary directory (normally `~/.cargo/bin`). Ensure that directory is
on your `PATH`. Repeat `make install` after changes to refresh the installed
command; use `cargo uninstall wasm-runner` to remove it.

Arguments after `wr` go directly to the application. All commands require an
application directory and read its `application.yaml`. The directory may be
relative or absolute; custom manifest filenames are not supported.
The listener defaults to `127.0.0.1:8080` and accepts numeric IPv4 or
bracketed IPv6 addresses with a port. Hostnames are rejected. Application settings
come from the manifest; the listener setting comes from the command line.
Environment variables do not alter configuration.

`wr run` reports the application name, manifest path, configured listener,
function count, routes in function-name order, and resolved module paths.
Component paths are `.wr/components/<name>.wasm` inside the application directory.
Every configured function must have a matching `functions/<name>.rs` source file;
prebuilt `module` declarations are not supported. Invalid manifests or missing
sources exit nonzero before printing metadata. The listener is not bound and
components are not opened, so metadata output does not require a build.

The manifest's data contract is [application.schema.json](application.schema.json),
a JSON Schema draft-07 document that can also be used by editor tooling. The
schema is embedded in `wr`; it does not need to accompany the manifest at runtime.
YAML anchors, aliases, and merge keys are resolved before schema validation.
Duplicate routes are checked after schema validation. Diagnostics use the detail
provided by the libraries, with basic context, rather than a prescribed
error-location format.

## Guest workspace setup

```sh
cargo run --locked -- setup examples
cargo run --locked -- setup examples --dry-run
```

Every direct `functions/*.rs` file is a build target, including files without
an entry in `application.yaml`. Discovery is non-recursive and ignores other
extensions and directories. Filenames use the same lowercase identifiers as
manifest functions: 1 to 64 letters, digits, or hyphens, starting with a letter.
YAML entries configure HTTP routes and function-specific dependencies, not build
membership. Unlisted functions inherit common dependencies and have no route.

Top-level `dependencies` are available to every source function. Each function
can add its own `dependencies`. A dependency is a version string or an object
with `version`, optional `features`, and optional `default-features`, matching
the corresponding Cargo fields. Declaring the same dependency globally and
locally is an error during setup. Path, Git, and optional dependencies are not
supported in this first pass. Cargo version requirements are passed through;
they are not resolved or checked by setup. `wit-bindgen` is managed by `wr` and
must not be declared in application dependencies.

The generated `Cargo.toml` sits beside the application manifest and defines a
workspace with one `cdylib` package per source function under `.wr/functions/<name>`.
Packages are named `wr-function-<name>`, with explicit library names
`wr_function_<name>` (hyphens become underscores). Each package's generated
`lib.rs` includes the original source file, generates bindings from the copied
`.wr/wit/world.wit`, and exports the HTTP handler. Common dependencies are
inherited from the workspace. Handlers use the generated WIT types directly:

```rust
use crate::{Request, Response};

pub fn handle(_request: Request) -> Response {
    Response {
        status: 200,
        headers: vec![],
        body: b"Hello, world!".to_vec(),
    }
}
```

See the [example handlers](examples/functions) and [guest contract](wit/README.md).

Setup creates the generated files' parent directories and replaces changed
files when run again. Identical files are left untouched so repeated builds
can reuse Cargo artifacts. It refuses to overwrite a root `Cargo.toml` unless it has
the generated-file header written by setup. Other files are left alone, including
old generated wrappers and manifests. Member manifests explicitly select the
application workspace, so an old `.wr/Cargo.toml` is not used.
Setup creates or updates `.gitignore` beside the application manifest, preserving
existing contents and appending any missing entries from `.wr/`, `/Cargo.toml`,
and `/target/`. Entries are compared literally; existing patterns are not
interpreted or reordered. Cargo maintains `Cargo.lock` beside the application
manifest; keep that lockfile in version control.

Setup also creates or updates `.vscode/settings.json`, adding
`${workspaceFolder}/functions` to `rust-analyzer.vfs.extraIncludes` without
duplicating it. Existing include paths, other settings, comments, and formatting
are preserved. JSON with comments and trailing commas is supported. Invalid
settings or an include setting that is not an array of strings cause setup to
fail before writing files.

With `--dry-run`, setup only prints the proposed file layout and contents,
including the merged `.gitignore` and editor settings, and does not write files
or create directories.
Neither mode runs Cargo, installs
prerequisites, generates lockfiles, builds WASM, packages components, or watches
for changes. The preview is human-readable output, not an executable script.
`make setup` remains the separate command for setting up this repository's Rust
toolchain.

## Building components

```sh
wr build examples
wr build examples --release
wr build examples --locked
```

Build refreshes the generated workspace automatically; running setup first is
optional. It invokes Cargo once for all discovered function libraries, using
`wasm32-wasip2` and an explicit application-local `target` directory. Cargo
diagnostics are inherited directly. Debug builds are the default; `--release`
selects optimized builds. The explicit library name, target, profile, and output
directory determine each artifact path without parsing Cargo JSON messages.
After a successful build, components are copied to `.wr/components/<name>.wasm`.
Failed Cargo builds do not publish components; previous outputs remain on disk.
Publication is not transactional if a filesystem operation fails during copying.

Cargo creates or updates the application's `Cargo.lock` normally. Commit that
file and use `--locked` to require it to be up to date without changes. Guest
dependencies, including the managed `wit-bindgen` version, are separate from
the CLI's lockfile. Building requires Cargo and the WASM target; `wr` never
installs prerequisites. For applications outside this checkout, configure the
chosen Rust toolchain with the `wasm32-wasip2` target before building.

These are real components exporting our custom HTTP interface, not WASI HTTP.
The target may also introduce WASI imports. Component execution, host capability
policy, HTTP serving, and event handlers are not implemented.

### VS Code

Run setup, then open `examples` as the VS Code workspace. Its root `Cargo.toml`
is discoverable by rust-analyzer. Setup creates or updates the
[editor settings](examples/.vscode/settings.json), adding `${workspaceFolder}/functions` to
`rust-analyzer.vfs.extraIncludes`: the original handlers live outside the
generated Cargo package directories, so rust-analyzer needs them explicitly
included to resolve the generated `#[path]` modules. Cargo itself resolves these
paths without extra configuration. `extraIncludes` requires absolute paths;
the VS Code extension expands `${workspaceFolder}` before sending the setting
to rust-analyzer. Bare relative paths such as `functions` are ignored.
Remove any old `linkedProjects` setting
pointing at `.wr/Cargo.toml`.

Opening the parent repository still discovers the host workspace; to load both
in that window, explicitly link `Cargo.toml` and `examples/Cargo.toml` using
`rust-analyzer.linkedProjects`, or open `examples` in a separate window.
When opening the parent repository, add `${workspaceFolder}/examples/functions` to
`rust-analyzer.vfs.extraIncludes` in that window's settings instead; the nested
application's editor settings do not apply to the parent workspace.
If hover information is missing after generating the workspace, run
**rust-analyzer: Restart server** from the Command Palette.

## CLI development

| Target | Purpose |
| --- | --- |
| `make setup` | Install the pinned Rust toolchain and components |
| `make update-deps` | Create or update the lockfile to the latest compatible dependencies |
| `make build` (or `make`) | Compile a debug CLI for local development |
| `make install` | Build and install `wr` from this checkout |
| `make check` | Check formatting, lint, and run tests across the workspace |

For a quick edit/build/run loop, invoke the newly built binary directly:

```sh
make build
./target/debug/wr --help
./target/debug/wr build examples
./target/debug/wr run examples --listen 127.0.0.1:9000
make check
```

This uses the debug build in the checkout; the installed `wr` is updated only by
`make install`. There is no `make run` wrapper or `ARGS` variable.

Alternatively, build and run in one Cargo command:

```sh
cargo run --locked -- run examples --listen 127.0.0.1:9000
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
uniqueness and source-file existence before returning the application.
[src/run.rs](src/run.rs) prints metadata and is the entry point for future
application preparation and serving. [src/setup.rs](src/setup.rs) coordinates
generation and selects writing or previewing. The separate
[wasm-runner-setup crate](crates/wasm-runner-setup) renders workspace and member
manifests, WIT definitions, and entry points and writes them through a separate
filesystem function. [src/build.rs](src/build.rs) invokes Cargo and publishes
components at deterministic paths. Command handlers receive the already-loaded
application and its directory.

## Design

Read the [architecture](ARCHITECTURE.md), the
[current functionality spec](SPEC.md), and the
[example manifest](examples/application.yaml). The [guest interface notes](wit/README.md)
describe the HTTP component contract used by generated bindings. Runtime and HTTP
design details will be documented as those features are implemented.
