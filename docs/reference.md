# Command and manifest reference

This page defines the current public behavior of `wr`. For a first build, use
[Getting started](getting-started.md); for recipes, use [Common tasks](guides.md).
[Product requirements](../SPEC.md) incorporate this reference for exact behavior.
The [schema](../application.schema.json) and [WIT world](../wit/world.wit) remain
the authoritative field and guest-interface definitions.

## Commands

Application filesystem commands require an application directory, supplied as a
relative or absolute path. Setup, build, update, run, and deploy read
`application.yaml` inside it; alternate manifest filenames are not supported.
Clean does not load a manifest.

| Invocation | Behavior and options |
| --- | --- |
| `wr setup <DIRECTORY> [--dry-run]` | Generate the guest workspace, or preview paths and contents without writing. |
| `wr build <DIRECTORY> [--release] [--unlocked]` | Refresh the workspace and compile components using an up-to-date guest lockfile. Debug is the default; `--release` selects optimization; `--unlocked` explicitly permits lockfile changes. |
| `wr update <DIRECTORY>` | Refresh the workspace and create or update the guest lockfile to the newest compatible dependency versions, without building components. |
| `wr clean <DIRECTORY>` | Remove generated workspace files and build outputs. |
| `wr run <DIRECTORY> [--listen <IP:PORT>]` | Serve previously built handlers. The listener defaults to `127.0.0.1:8080`. |
| `wr daemon start [--listen <IP:PORT>]` | Report an already running daemon, or start a Unix background daemon with an empty application registry. New instances default to `127.0.0.1:8080`. |
| `wr daemon status` | Query the live daemon's PID, hosting address, and deployed applications. |
| `wr daemon stop` | Stop accepting work, drain executions, and wait for the daemon to release its listeners and process lock. |
| `wr deploy <DIRECTORY>` | Upload existing built components and atomically create or replace the named application. |
| `wr apps list` | Print deployed application names in sorted order, one per line. |
| `wr remove <NAME>` | Remove the application's routes; already selected generations remain alive until their requests finish. |

Daemon operations accept the global `--state-dir <DIRECTORY>` option. See
[daemon and deployment](#daemon-and-deployment) for lifecycle and transport details.

Use `wr --help`, `wr --version`, or `wr <COMMAND> --help` to inspect the CLI.
Arguments after `wr` go directly to the application.

## Manifest

An application has a name and a nonempty map of named functions. Every configured
function declares an HTTP method/path pair. See the complete
[example manifest](../examples/application.yaml). Application and function names
are lowercase identifiers of 1 to 64 letters, digits, or hyphens, starting with
a letter.

The [JSON Schema draft-07 document](../application.schema.json) defines field
constraints and supported methods. Unknown fields are rejected. `http.path`
is a [Rocket route URI](https://api.rocket.rs/v0.5/rocket/struct.Route.html),
parsed by Rocket itself. Use `/users/<user_id>/orders` for a single-segment
capture or `/files/<path..>` for a trailing capture. Query constraints and
percent-encoded literals follow Rocket's rules. Use `<name>` for captures;
the old `{name}` form is not capture syntax.

Routes use Rocket's default ranks and collision rules. For routes without
query constraints, fully static paths rank before partially dynamic paths,
which rank before fully dynamic paths. This is a category ranking, not a
per-segment specificity calculation. Overlapping routes with the same method
and rank fail foreground startup or deployment, including `/a/<x>` and `/<y>/b`.
Route conflicts are checked when running or deploying, not when generating or
building the guest workspace.
There is no custom rank setting.

Method matching participates in route selection: with `GET /users/me` and
`POST /users/<id>`, a POST to `/users/me` invokes the latter with `id=me`.
Rocket normalizes route slashes and ignores empty path segments when matching;
trailing slashes do not distinguish routes. Manifest iteration order does not
resolve collisions.

One YAML document is accepted, including anchors, aliases, and merge keys.
Duplicate mapping keys are rejected. The schema is embedded in `wr` and does
not need to accompany the application at runtime; editors can use the checked-in
schema directly.

Every configured function must have a matching `functions/<name>.rs` file.
Component paths are inferred as `.wr/components/<name>.wasm` relative to the
application directory. Prebuilt `module` declarations are rejected. Components
need not exist for setup, but run requires compatible built components.

Application settings come from the manifest, and the listener setting comes
from the command line. Environment variables do not override those settings.
Cargo has its own configuration and environment, described under [Build](#build).

## Source discovery

Setup, build, and update discover every direct `functions/*.rs` file, including sources
without a manifest entry. Discovery is non-recursive, ignores directories and
other extensions, validates filenames using the function identifier rules, and
sorts names. At least one source function is required.

Manifest entries configure routes and function-specific dependencies, not build
membership. Unlisted functions inherit common dependencies and have no route.
The manifest must still contain at least one configured function: support for
extra unlisted sources does not imply support for a completely route-free
application. Run serves configured functions only.

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
| Manifest loading | setup, build, update, run, deploy | Parse YAML, resolve merges, validate against the embedded schema, deserialize, parse route URIs with Rocket, and check configured source files exist. |
| Discovery and generation | setup, build, update | Validate discovered source names and require sources; reject reserved or conflicting dependencies; merge ignore rules and validate editor settings. |
| Workspace writing | setup without `--dry-run`, build, update | Refuse a handwritten root Cargo manifest before any writes; then create or update generated files. |
| Server preparation | run | Read every configured component, validate imports and the guest contract, compile native code, let Rocket reject route collisions, then bind the listener. |
| Deployment preparation | deploy | Read configured components, upload routes and bytes, then let the daemon validate and compile components and reject combined route collisions before publishing. |
| Compilation and publication | build | Cargo checks Rust and dependencies; after successful compilation, require every expected artifact before copying components. |
| Dependency update | update | Cargo resolves dependency versions allowed by the generated manifests and writes the application lockfile. |
| Cleanup | clean | Require an existing directory and check root Cargo manifest ownership before any removals. |

Run does not discover sources, generate a workspace, or compile guest Rust code.
It does compile the existing components to native code during runtime registration.
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
| `Cargo.lock` | Managed by Cargo during update or explicitly unlocked builds, not by setup; keep it in version control. |
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

Debug builds are the default. `--release` selects optimized builds. Builds pass
`--locked` to Cargo by default: the application's `Cargo.lock` must exist and be
up to date, and a missing lockfile or resolution requiring changes fails the build.
Run `wr update <DIRECTORY>` before the first build and when deliberately refreshing
dependencies or changing workspace membership. Source-only edits do not need an
update. `wr build --unlocked` opts out and lets Cargo create or adjust the lockfile
as needed; it does not force all already-locked versions to be upgraded. The old
`wr build --locked` flag is replaced by this locked default.

Guest dependencies, including managed `wit-bindgen`, are separate from the
CLI's lockfile. Building requires Cargo and
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

## Update

`wr update <DIRECTORY>` validates inputs and refreshes the generated workspace
using the same ownership checks as build, then runs `cargo update` against that
workspace. It creates a missing application `Cargo.lock`, or updates dependencies
to the newest versions compatible with the manifest requirements and Cargo's
resolver. It does not rewrite version requirements in `application.yaml`.
See [Cargo's update command](https://doc.rust-lang.org/cargo/commands/cargo-update.html).

Update invokes Cargo but does not compile handlers, publish components, or start
the server. Existing components stay unchanged; rebuild and redeploy (or restart foreground serving) to use updated
dependencies. Review and retain the application lockfile in version control.
This command updates application dependencies, not the `wr` installation.

Cargo configuration and environment remain in effect. Cargo diagnostics pass
through, and a failed update exits nonzero without a success message. Generated
workspace files may already have changed; the operation is not transactional.
Success reports `Updated dependencies in <PATH>`, naming the application lockfile.

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

Run loads the manifest and existing `.wr/components/<name>.wasm` files. It never
invokes Cargo. Missing components report their function and path with a
build-first instruction. All routes and components must pass preparation before
binding. `Listening on http://<address>` is printed only after preparation and
binding succeed; startup failure prints no readiness message. Port zero lets
the OS choose a port and the message reports the actual address.

`--listen` accepts a numeric IPv4 address or bracketed IPv6 address with a port,
such as `127.0.0.1:9000` or `[::1]:9000`. Hostnames are rejected. There is no
implicit exposure beyond the default loopback address.

The library's async `server::serve(config, handlers, shutdown_future, on_ready)`
accepts component bytes without a CLI or manifest, inside a Tokio runtime.
The readiness callback receives the bound address; the future returns after
shutdown and draining admitted executions.
Only the CLI installs process signal handlers (Ctrl-C and SIGTERM on Unix).

| Setting | CLI default (configurable through library `server::Config`) |
| --- | --- |
| Execution timeout | 5 seconds |
| Maximum concurrent executions | 16, shared across all workers |
| Maximum request body | 1 MiB |
| Request-body timeout | 10 seconds; HTTP header handling follows Rocket's transport defaults |

Synchronous invocations run on blocking threads. Admission has no application
waiting queue: excess invocations receive 503. A slot is held until execution
actually ends, including after a client disconnects. The execution allowance
starts inside the blocking job and includes instantiation and handling, not
registration or time waiting for a blocking thread. Epoch interruption traps
nonterminating guest code with approximately 5 ms tick/checkpoint granularity.
Shutdown stops execution admission, gracefully stops HTTP workers, and drains
admitted jobs before releasing the runtime and its ticker.

The guest receives Rocket's encoded request path, decoded named captures, and
form-decoded query pairs. Each decoded key appears once, in first-occurrence
order, with values in encounter order. Empty/absent queries yield no pairs;
`?flag` and `?flag=` both yield an empty string value. `+` decodes to a space;
`%2B` decodes to a literal plus. Malformed UTF-8 in captures and query fields
uses replacement characters. Headers retain repeated values; request header
values follow Rocket's HTTP parsing. A trailing capture joins its decoded
segments with `/`; ignored `<_>` captures are omitted from `path-params`.
Route query constraints affect matching, while the guest still receives all
query pairs.

| Condition | HTTP result |
| --- | --- |
| No matching route for the path and method | 404 (except Rocket's HEAD fallback) |
| Execution capacity exhausted or shutdown admission closed | 503 |
| Request body over the size limit | 413 |
| Request-body reading expires | 408 |
| Guest execution timeout | 504 |
| Guest trap or invalid guest response | 500 |

Rocket tries an explicit HEAD route first, then falls back to GET and strips
the response body. On fallback the guest sees method GET. OPTIONS requires an
explicit route; no CORS handling is installed. Guest HTTP error responses
are successful invocations and retain their status. Final guest statuses must
be 200–599 and header names/values must be HTTP-valid. Hop-by-hop headers,
headers named by `Connection`, and guest `Content-Length` are removed; transport
framing is computed by the server. Bodies are suppressed where HTTP requires
it, including HEAD and statuses 204, 205, and 304. Execution diagnostics go to
host stderr. Errors produced by the component dispatch layer have empty bodies;
Rocket's own responses, such as an unmatched route's 404, use its default error
responders and may include a body.

### Runtime and WASI policy

`runtime::Runtime::new(timeout)` creates the engine and its owned epoch ticker.
`register(bytes)` compiles and validates a component, returning an opaque
`ModuleId`; `execute(&id, &Request)` synchronously returns `Response` or
`ExecutionError`. IDs belong to their original runtime. Registration uses a
mutable runtime; subsequent invocations may share it concurrently. Each call
gets a new instance, store, resource table, and WASI context. The runtime does
no routing, parsing, socket handling, or job scheduling.

Supported imports are the WASI 0.2 interfaces for CLI environment, exit,
stdin/stdout/stderr and terminals; I/O error, streams, and poll; and the monotonic
clock. These are the imports introduced by the current Rust target in the
examples. Import versions/types must be compatible with the host. No environment,
arguments, preopened directories, or network access is inherited. Stdin is
closed; stdout and stderr are sinks; no terminals are available. Monotonic clock
reads report time since invocation; timer subscriptions trap because native
WASI waits cannot be interrupted by guest epochs. Filesystem, random, socket,
wall-clock, and custom host interfaces are not supported in this increment.
Unsupported imports and incompatible guest interfaces fail registration.
The [guest contract](../wit/README.md#execution-and-compatibility) defines interface
versioning and rebuild requirements.

Epoch interruption cannot forcibly terminate arbitrary host code or undo
external effects. Any future host I/O must honor the remaining execution
deadline. Memory quotas, streaming, hot reload, SQL/KV bindings, and custom
scheduling are outside this increment.

## Output and errors

Successful commands exit zero. Invalid arguments, manifest loading or validation
failures, and failed operations exit nonzero. Manifest failures produce no
readiness output. Diagnostics use parsing and validation library detail with
basic operation context; no particular error-location format is guaranteed.

## Daemon and deployment

These commands currently require Unix (including macOS and Linux). The daemon is
one background process with two listeners: an HTTP hosting listener and a private
Unix control socket. It does not install a login service or restart itself after
failure. `wr run` remains available for foreground, single-application serving.

State-directory selection is `--state-dir`, then `WR_STATE_DIR`, then
`$HOME/.wr/daemon`. Relative paths resolve against the invoking working directory.
Use the same directory for all commands targeting a daemon. The directory is
created with mode 0700; an existing directory with group/other permissions or a
symlink is rejected. It contains `control.sock` (mode 0600), `daemon.lock`, and
an append-only `daemon.log`. The full socket path must fit the platform's Unix
socket path limit (less than 104 bytes on macOS). Use a shorter state directory
if binding reports a path-length error. A different state directory selects a
separate daemon, whose hosting address must also be available.

`start` first queries the selected control socket. If a daemon responds, it
succeeds and reports that instance's PID and actual hosting address without
spawning a process or changing its applications. `--listen` only applies to new
instances; stop the existing daemon first to change its address. A missing or
refused socket connection proceeds with startup; other control errors fail the
command. The process lock still prevents duplicate instances if starts race;
a concurrent start whose daemon is not yet ready may need retrying.

For a new instance, `start` waits up to 30 seconds for both listeners to bind. It prints the hosting
address, control socket, and log location. Port zero selects an available hosting
port. Startup failure prints no successful readiness and terminates/reaps the
child; diagnostics are in the log. A process-held file lock prevents duplicate
instances. A stale socket is removed only after acquiring that lock; a non-socket
file at that path is rejected. The lock file itself remains after shutdown.
The daemon uses a separate process group and has no terminal input.

`status` queries the live control server, rather than trusting a PID file. An
unreachable daemon is a command failure. `stop` requests graceful shutdown and
waits up to 120 seconds for release of the process lock. New deployments and
executions are rejected during shutdown. Already admitted executions and pending
control work are drained; control reads/writes have 30-second deadlines. SIGINT
and SIGTERM use the same shutdown path. Stopping an absent daemon fails.

`deploy` loads the manifest and configured sources using the usual validation,
then reads existing component files. It neither runs Cargo nor generates files.
The control payload contains the manifest's application name, named method/path
routes, and component bytes; source paths and build dependencies are not sent.
The server independently validates names, methods, route syntax, component
compatibility, and combined route collisions. Application and handler names use
the manifest identifier constraints. At least one handler is required.

Application names are deployment identities: deploying an existing name replaces
its complete route/component set, and renaming it creates another application.
All applications share one route namespace with the existing Rocket ranking and
collision rules. Prepare/compile and route validation finish before atomic
publication. Failure leaves every active application unchanged. Concurrent
changes are serialized at publication and validated against the latest set.
An acknowledged deployment is active for subsequent requests; a lost connection
can leave its outcome unknown, so inspect/redeploy before assuming failure.

Each request retains its selected routing generation until completion. Old
components are released once no active registry or request references them.
Removal returns after route publication, without waiting for prior requests.
`remove` of an unknown name fails. The execution limits in [Run](#run) apply
globally across applications; there is no per-application fairness guarantee.
Deleting local build outputs does not affect deployed component bytes.

The control transport uses one request and response per Unix connection. Each
message is a four-byte big-endian byte length followed by UTF-8 JSON, capped at
128 MiB including JSON encoding. The request's `command` is `deploy`, `remove`,
`list`, `status`, or `stop`. Deploy includes `application: {name, handlers}`, where
each handler contains `name`, `method`, `path`, and `component` (an array of byte
values). Remove includes `name`. Responses use `result: ok`, `applications`
(with `names`), `status` (with `pid`, `listen`, and `applications`), or `error`
(with `message`). Four control requests may run concurrently; excess connections
are closed. Client reads time out after 120 seconds; a timeout does not cancel a
server-side deployment. The control socket is local administration, not a remote
or multi-user authentication API.

Deployments are held only in memory. Each new daemon process begins empty; redeploy
the desired applications after a stop, crash, or restart. The state directory
contains lifecycle files and logs, not persisted application configuration.
