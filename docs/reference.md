# Command and manifest reference

This page defines the current public behavior of `wr`. For a first build, use
[Getting started](getting-started.md); for recipes, use [Common tasks](guides.md).
[Product requirements](../SPEC.md) incorporate this reference for exact behavior.
The [schema](../application.schema.json) and [WIT world](../wit/world.wit) remain
the authoritative field and guest-interface definitions.

## Commands

All commands require an application directory, supplied as a relative or absolute
path. Setup, build, and run read `application.yaml` inside it; alternate manifest
filenames are not supported. Clean does not load a manifest.

| Invocation | Behavior and options |
| --- | --- |
| `wr setup <DIRECTORY> [--dry-run]` | Generate the guest workspace, or preview paths and contents without writing. |
| `wr build <DIRECTORY> [--release] [--locked]` | Refresh the workspace and compile components. Debug is the default; `--release` selects optimization; `--locked` requires an up-to-date guest lockfile. |
| `wr clean <DIRECTORY>` | Remove generated workspace files and build outputs. |
| `wr run <DIRECTORY> [--listen <IP:PORT>]` | Validate and print metadata. The listener defaults to `127.0.0.1:8080`; no socket is opened. |

Use `wr --help`, `wr --version`, or `wr <COMMAND> --help` to inspect the CLI.
Arguments after `wr` go directly to the application.

## Manifest

An application has a name and a nonempty map of named functions. Every configured
function declares an HTTP method/path pair. See the complete
[example manifest](../examples/application.yaml). Application and function names
are lowercase identifiers of 1 to 64 letters, digits, or hyphens, starting with
a letter.

The [JSON Schema draft-07 document](../application.schema.json) defines field
constraints, supported methods, and literal route paths. Unknown fields are
rejected. Routes are exact paths, not wildcard patterns; the schema excludes
whitespace, control characters, query strings, fragments, percent escapes, and
wildcard syntax. A method/path pair may appear only once; distinct methods may
use the same path.

One YAML document is accepted, including anchors, aliases, and merge keys.
Duplicate mapping keys are rejected. The schema is embedded in `wr` and does
not need to accompany the application at runtime; editors can use the checked-in
schema directly.

Every configured function must have a matching `functions/<name>.rs` file.
Component paths are inferred as `.wr/components/<name>.wasm` relative to the
application directory. Prebuilt `module` declarations are rejected. Components
need not exist for setup or metadata output.

Application settings come from the manifest, and the listener setting comes
from the command line. Environment variables do not override those settings.
Cargo has its own configuration and environment, described under [Build](#build).

## Source discovery

Setup and build discover every direct `functions/*.rs` file, including sources
without a manifest entry. Discovery is non-recursive, ignores directories and
other extensions, validates filenames using the function identifier rules, and
sorts names. At least one source function is required.

Manifest entries configure routes and function-specific dependencies, not build
membership. Unlisted functions inherit common dependencies and have no route.
The manifest must still contain at least one configured function: support for
extra unlisted sources does not imply support for a completely route-free
application. Run reports configured functions only.

## Dependencies

Top-level `dependencies` apply to all discovered functions. Each configured
function can add its own `dependencies`. A value is a version string or an
object containing `version`, optional `features`, and optional
`default-features`, using the corresponding Cargo meanings. Other fields,
including path, Git, and optional dependencies, are not supported.

Setup rejects a dependency declared both globally and locally for a function.
`wit-bindgen` is managed by `wr` and must not be declared in application
dependencies. Version requirements are passed through to Cargo; setup does not
resolve or check them. See [Configure dependencies](guides.md#configure-dependencies)
for a recipe.

## Validation stages

A schema-valid manifest alone does not guarantee that setup or build succeeds.

| Stage | Commands | Checks |
| --- | --- | --- |
| Manifest loading | setup, build, run | Parse YAML, resolve merges, validate against the embedded schema, deserialize, reject duplicate routes, and check configured source files exist. |
| Discovery and generation | setup, build | Validate discovered source names and require sources; reject reserved or conflicting dependencies; merge ignore rules and validate editor settings. |
| Workspace writing | setup without `--dry-run`, build | Refuse a handwritten root Cargo manifest before any writes; then create or update generated files. |
| Compilation and publication | build | Cargo checks Rust and dependencies; after successful compilation, require every expected artifact before copying components. |
| Cleanup | clean | Require an existing directory and check root Cargo manifest ownership before any removals. |

Run does not perform discovery, workspace generation, or compilation.
Dry-run performs discovery and generation but does not perform the writer's
root-manifest ownership check. Neither setup mode reads function source contents.

## Setup

`wr setup <DIRECTORY>` finishes validation and generation before changing the
filesystem. It creates parent directories and replaces changed generated files.
Identical files are left untouched so repeated builds can reuse Cargo artifacts.
Success reports `Workspace written to <DIRECTORY>`.

A root `Cargo.toml` without the setup-generated header is rejected before any
writes. Existing files outside the generated output are left alone, including
obsolete function directories, old wrappers/manifests, build artifacts, and
lockfiles. Filesystem errors identify the affected path. Writes are not
transactional: earlier files can have been written when a later write fails.

### Generated files

All paths below are relative to the application directory.

| Path | Purpose and ownership |
| --- | --- |
| `Cargo.toml` | Generated guest workspace root; protected against overwriting a handwritten manifest. |
| `.wr/functions/<name>/Cargo.toml` | Generated `cdylib` package for each discovered source. |
| `.wr/functions/<name>/lib.rs` | Generated entry point that includes the original source and exports the guest handler. |
| `.wr/wit/world.wit` | Copy of the embedded guest contract. |
| `.gitignore` | Existing contents preserved; required literal entries appended if missing. |
| `.vscode/settings.json` | Existing settings merged with the source include path. |
| `Cargo.lock` | Managed by Cargo during builds, not by setup; keep it in version control. |
| `target/` | Application-local Cargo build artifacts. |
| `.wr/components/<name>.wasm` | Components published after successful compilation. |

Packages are named `wr-function-<name>` with explicit library names
`wr_function_<name>` (hyphens in function names become underscores). Each member
explicitly selects the application workspace, so an old `.wr/Cargo.toml` is not
used. Its `lib.path` selects the generated `lib.rs`, which includes the source,
generates WIT bindings, re-exports `Request` and `Response`, implements the
exported handler trait, and exports that implementation. Common dependencies
use `workspace.dependencies` and explicit inheritance in each member; local
dependencies appear only in that function's package. The
[architecture](../ARCHITECTURE.md#guest-workspace-boundary) explains these choices.

### Ignore rules and editor settings

Setup preserves `.gitignore` and appends missing literal lines from `.wr/`,
`/Cargo.toml`, and `/target/`, adding a separating newline if necessary.
Existing patterns, including negations, are not interpreted or reordered.
Repeated setup does not duplicate these entries. No ignore rule is added for
`Cargo.lock`.

Setup adds `${workspaceFolder}/functions` to the
`rust-analyzer.vfs.extraIncludes` array in `.vscode/settings.json` without
duplicating it. It preserves other include paths, unrelated settings, comments,
and formatting. JSON comments and trailing commas are supported. Invalid
settings or an include setting that is not an array of strings cause generation
to fail before any files are written. See the [VS Code recipe](guides.md#prepare-vs-code)
for workspace selection and troubleshooting.

### Preview

`wr setup <DIRECTORY> --dry-run` prints a human-readable preview to stdout:
file paths in sorted order, followed by their contents in the same order,
including merged ignore rules and editor settings. It creates no files or
directories. Validation and generation errors produce no preview output.
The preview is not an executable script.

Neither setup mode invokes Cargo, installs prerequisites, generates or
synchronizes lockfiles, builds/packages components, or watches for changes.
`make setup` separately prepares this repository's Rust toolchain.

## Build

Build automatically refreshes the generated workspace; explicit setup is
optional. It invokes Cargo once for all discovered function libraries, targeting
`wasm32-wasip2` with an explicit application-local `target` directory. This
overrides Cargo's target-directory configuration. Other Cargo configuration and
environment remain in effect; `wr` does not clear the environment.

Debug builds are the default. `--release` selects optimized builds. Cargo creates
or updates the application's `Cargo.lock` normally; `--locked` requires it to
exist and be up to date without changes. Guest dependencies, including managed
`wit-bindgen`, are separate from the CLI's lockfile. Building requires Cargo and
the WASM target; `wr` never installs them. See [prerequisites](getting-started.md#prerequisites-and-installation).

Cargo diagnostics are inherited directly. A nonzero Cargo status fails the
command; old artifacts are never treated as evidence of success. After Cargo
succeeds, every expected artifact at
`target/wasm32-wasip2/<profile>/wr_function_<name>.wasm` must exist before copying
begins. Hyphens in artifact library names become underscores. Components are
copied to `.wr/components/<name>.wasm`, and each published path is reported as
`Built <PATH>`.

Cargo failure leaves previously published components unchanged. Publication
is not transactional if a filesystem operation fails during copying. Obsolete
outputs are not removed. Setup may already have updated workspace files before
a compilation failure.

These are components exporting the [custom HTTP guest interface](../wit/README.md),
not WASI HTTP. The target may introduce WASI imports. Build does not execute
components, serve HTTP, provide host capability policy or SQL/KV support, or
implement event handlers.

## Clean

`wr clean <DIRECTORY>` removes `.wr/`, `target/`, and the root `Cargo.toml`
generated by setup. This includes obsolete generated files, published components,
and all build profiles. It preserves the application manifest, function sources,
`Cargo.lock`, `.gitignore`, editor settings, and other application files.

A handwritten root `Cargo.toml` causes cleanup to fail before anything is
removed. The application directory must exist and be a directory, but its
manifest and sources can be missing or invalid. Clean does not run Cargo.
Missing generated files are ignored, so repeated cleanup succeeds. Directory
symlinks at the removed paths are unlinked rather than followed. Removal errors
identify the path; removal is not transactional. Success reports
`Workspace cleaned in <DIRECTORY>`. Setup or build can regenerate the workspace.

## Run

Run reports the application name, manifest path, configured listener, configured
function count, and routes with resolved component paths in function-name order.
It explicitly states that component execution and HTTP serving are not
implemented. Components are not opened, and no listener is bound; a build is not
required for metadata output.

`--listen` accepts a numeric IPv4 address or bracketed IPv6 address with a port,
such as `127.0.0.1:9000` or `[::1]:9000`. Hostnames are rejected.

## Output and errors

Successful commands exit zero. Invalid arguments, manifest loading or validation
failures, and failed operations exit nonzero. Manifest failures produce no
metadata output. Diagnostics use parsing and validation library detail with
basic operation context; no particular error-location format is guaranteed.
