# Current functionality

`wr` is a command-line tool that loads an application manifest, validates it,
prints its metadata, and exits. It does not load WebAssembly components or serve
HTTP requests. It can also generate a Cargo workspace for source functions.

## Command line

```sh
wr run examples/application.yaml
wr run examples/application.yaml --listen 127.0.0.1:9000
wr setup examples/application.yaml
wr setup examples/application.yaml --dry-run
```

The manifest path is required. The optional listener address accepts a numeric
IP address and port and defaults to `127.0.0.1:8080`. It is reported as
configuration only; no socket is opened. Environment variables do not alter
application configuration.

## Manifest

An application has a name and a nonempty collection of named functions. Each
function declares an HTTP method/path pair and optionally a module path. See the
[example manifest](examples/application.yaml) for the format and
[application.schema.json](application.schema.json) for field constraints.

The tool accepts one YAML document, including anchors, aliases, and merge keys.
It validates the resolved document against its embedded schema and rejects
duplicate mapping keys and duplicate method/path pairs.

Relative module paths resolve against the manifest's directory. Absolute paths
are also accepted. Omitting `module` selects a source function in
`functions/<name>.rs`, with an inferred component path of
`.wr/components/<name>.wasm`, both relative to the manifest. Module and source
files do not need to exist for metadata output or setup.

Optional top-level `dependencies` apply to all source functions; optional
function-level `dependencies` add dependencies to that function only. Values
are version strings or objects containing a required `version`, optional string
array `features`, and optional boolean `default-features`. Other dependency
fields are rejected. Cargo interprets version requirements during a future
build; setup does not resolve dependencies.

## Setup

`wr setup <MANIFEST>` validates the manifest and writes a generated Cargo
workspace rooted beside the application manifest, with member manifests under
`.wr/functions/<name>`. It creates parent directories and overwrites the
generated files. A root `Cargo.toml` without the setup-generated header is
rejected before any writes. Setup also creates or updates `.gitignore` beside
the application manifest. It preserves existing contents and appends missing
entries from `.wr/`, `/Cargo.toml`, and `/target/`, checking for exact matching
lines. It adds a separating newline if needed. Repeated runs do not duplicate
these entries. Existing patterns, including negations, are not interpreted or
reordered. No ignore entry is added for `Cargo.lock`.
Existing files that are not
part of the generated output are left alone, including obsolete function
directories, build artifacts, and lockfiles. Success reports the workspace path.
It includes a root Cargo manifest and a `cdylib` package manifest for each source
function. Each member explicitly selects the application workspace and sets
`lib.path` to `../../../functions/<name>.rs`; no wrapper files are generated.
Packages are named `wr-function-<name>`. Common dependencies are declared in
`workspace.dependencies` and explicitly inherited by each function crate.
Each function's additional dependencies appear only in its own package.

Functions with explicit module paths are skipped. Setup rejects an application
with no source functions and rejects a dependency declared both globally and
locally for a source function. Validation and generation finish before any
filesystem changes. Filesystem errors include the failing path; writes are not
transactional, so earlier files can have been written when a later write fails.

With `--dry-run`, setup only writes stdout: it prints file paths in sorted
order, followed by their contents in the same order, including the merged
`.gitignore`, without creating files or directories. Validation and generation
errors produce no preview output.

Neither mode reads source files, invokes Cargo, generates or synchronizes
lockfiles, or watches changes.
The source files contain ordinary Rust functions without WIT bindings or WASM
exports; compiling and packaging runnable components is outside this iteration.

## Output and errors

On successful `run`, print the application name, manifest path, configured listener,
function count, and routes with resolved module paths in function-name order.
The output states that component execution and HTTP serving are not implemented.

Invalid arguments or a manifest loading or validation failure produce an error
and a nonzero exit status. Manifest failures produce no metadata output.
Diagnostics use the detail provided by the parsing and validation libraries,
with basic operation context; no particular error-location format is required.
