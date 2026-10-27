# Current functionality

`wr` is a command-line tool that loads an application manifest, validates it,
prints its metadata, and exits. It does not load WebAssembly components or serve
HTTP requests. It also generates a Cargo workspace and builds HTTP function components.

## Command line

```sh
wr run examples
wr run examples --listen 127.0.0.1:9000
wr setup examples
wr setup examples --dry-run
wr build examples
wr build examples --release --locked
```

The application directory is required. All commands read `application.yaml`
inside it; alternate filenames are not supported. The optional listener address accepts a numeric
IP address and port and defaults to `127.0.0.1:8080`. It is reported as
configuration only; no socket is opened. Environment variables do not alter
application configuration.

## Manifest

An application has a name and a nonempty collection of named functions. Each
configured function declares an HTTP method/path pair. See the
[example manifest](examples/application.yaml) for the format and
[application.schema.json](application.schema.json) for field constraints.

The tool accepts one YAML document, including anchors, aliases, and merge keys.
It validates the resolved document against its embedded schema and rejects
duplicate mapping keys and duplicate method/path pairs.

Every configured function must have a source file at `functions/<name>.rs`.
Component paths are inferred as `.wr/components/<name>.wasm`, relative to the
application directory. Prebuilt `module` declarations are rejected. Components
do not need to exist for metadata output or setup.

Every direct `functions/*.rs` file is a build target, even without YAML
configuration. Discovery is non-recursive and ignores directories and other
extensions. Discovered names must satisfy the schema's identifier constraints.
Unconfigured functions inherit shared dependencies and have no HTTP route.

Optional top-level `dependencies` apply to all source functions; optional
function-level `dependencies` add dependencies to that function only. Values
are version strings or objects containing a required `version`, optional string
array `features`, and optional boolean `default-features`. Other dependency
fields are rejected. `wit-bindgen` is reserved for generated bindings and may
not be declared as an application dependency. Cargo interprets version requirements during a
build; setup does not resolve dependencies.

## Setup

`wr setup <DIRECTORY>` validates the manifest and sources and writes a generated Cargo
workspace rooted beside the application manifest, with member manifests under
`.wr/functions/<name>`. It creates parent directories and replaces changed
generated files, leaving identical files untouched for incremental builds.
A root `Cargo.toml` without the setup-generated header is
rejected before any writes. Setup also creates or updates `.gitignore` beside
the application manifest. It preserves existing contents and appends missing
entries from `.wr/`, `/Cargo.toml`, and `/target/`, checking for exact matching
lines. It adds a separating newline if needed. Repeated runs do not duplicate
these entries. Existing patterns, including negations, are not interpreted or
reordered. No ignore entry is added for `Cargo.lock`.
Setup also creates or merges `.vscode/settings.json`, adding
`${workspaceFolder}/functions` to `rust-analyzer.vfs.extraIncludes` if absent.
Existing include paths and unrelated settings are retained, along with comments
and formatting. JSON comments and trailing commas are supported. Invalid editor
settings or an include setting that is not an array of strings are rejected
before any writes. Repeated runs do not duplicate the include path.
Existing files that are not
part of the generated output are left alone, including obsolete function
directories, build artifacts, and lockfiles. Success reports the workspace path.
It includes a root Cargo manifest and a `cdylib` package manifest for each source
function. Each member explicitly selects the application workspace and sets
`lib.path` to a generated `lib.rs` entry point. This entry point includes
`functions/<name>.rs`, generates bindings from `.wr/wit/world.wit`, re-exports
the WIT `Request` and `Response` types, implements the exported handler trait,
and exports the implementation. Packages are named `wr-function-<name>`;
explicit library names are `wr_function_<name>` with hyphens replaced by
underscores. Common dependencies are declared in
`workspace.dependencies` and explicitly inherited by each function crate.
Each function's additional dependencies appear only in its own package.

Setup rejects an application with no source functions and rejects a dependency declared both globally and
locally for a source function. Validation and generation finish before any
filesystem changes. Filesystem errors include the failing path; writes are not
transactional, so earlier files can have been written when a later write fails.

With `--dry-run`, setup only writes stdout: it prints file paths in sorted
order, followed by their contents in the same order, including the merged
`.gitignore` and editor settings, without creating files or directories. Validation and generation
errors produce no preview output.

Neither mode reads source contents, invokes Cargo, generates or synchronizes
lockfiles, or watches changes.

## Build

`wr build <DIRECTORY>` refreshes the generated workspace and invokes Cargo once
to build all workspace libraries for `wasm32-wasip2`. It supplies an explicit
application-local `target` directory, overriding Cargo's target-directory
configuration. Debug is the default profile; `--release` enables optimization.
`--locked` requires an existing up-to-date guest lockfile; otherwise Cargo may
create or update it. The guest lockfile is separate from the CLI lockfile.

Handlers have the signature `pub fn handle(request: Request) -> Response`, using
the WIT-generated Rust types re-exported at the crate root. Expected failures
are represented as HTTP responses, not automatically translated Rust errors.
Generated components export `local:http/handler@0.1.0` from the `http-function`
world. This is a custom interface, not WASI HTTP. The build target may introduce
additional WASI imports that a future host must satisfy.

Cargo's output is inherited directly. Nonzero Cargo status fails the command;
old artifacts are never used as evidence of success. After Cargo succeeds,
all expected artifacts at `target/wasm32-wasip2/<profile>/wr_function_<name>.wasm`
are checked for existence, then copied to `.wr/components/<name>.wasm`.
Hyphens in artifact library names become underscores. Each copied component is
reported. Cargo failure does not alter previously published components; copying
is not transactional if a later filesystem operation fails. Obsolete outputs
are not removed. No automatic prerequisite installation, runtime execution,
HTTP serving, SQL/KV support, or event handling is included.

## Output and errors

On successful `run`, print the application name, manifest path, configured listener,
function count, and routes with resolved module paths in function-name order.
The output states that component execution and HTTP serving are not implemented.

Invalid arguments or a manifest loading or validation failure produce an error
and a nonzero exit status. Manifest failures produce no metadata output.
Diagnostics use the detail provided by the parsing and validation libraries,
with basic operation context; no particular error-location format is required.
