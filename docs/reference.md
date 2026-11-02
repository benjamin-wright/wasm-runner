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
| `wr apps list` | Print name and path prefix separated by a tab, one application per line, sorted by name. |
| `wr remove <NAME>` | Remove the application's routes; already selected generations remain alive until their requests finish. |
| `wr admin [--listen <IP:PORT>]` | Serve a local, read-only daemon overview in the foreground. Defaults to an available port on `127.0.0.1`. See [Admin UI](#admin-ui). |

Daemon operations and the admin UI accept the global `--state-dir <DIRECTORY>` option. See
[daemon and deployment](#daemon-and-deployment) for lifecycle and transport details.

Use `wr --help`, `wr --version`, or `wr <COMMAND> --help` to inspect the CLI.
Arguments after `wr` go directly to the application.

## Manifest

An application has a name, a required `path-prefix`, and a nonempty map of named
functions. Optional `kv: true` enables [persistent application storage](#persistent-application-kv);
`sql: {enabled: true}` enables [SQL and deployment migrations](#persistent-application-sql);
it defaults to false. Every configured function declares an HTTP method/path pair. See the complete
[example manifest](../examples/application.yaml). Application and function names
are lowercase identifiers of 1 to 64 letters, digits, or hyphens, starting with
a letter.

`path-prefix` is the application's mount point for both `wr run` and `wr deploy`.
Use `/` for root hosting or a slash followed by nonempty literal segments, such
as `/demo` or `/api/orders`. Segments may contain ASCII letters, digits, `-`, `_`,
`.`, and `~`, but cannot be `.` or `..`. Trailing or repeated slashes, percent
encoding, whitespace, query strings, fragments, and dynamic captures are rejected.
Missing prefixes are errors: existing manifests must add an explicit prefix
(use `/` to preserve their original URLs). There is no CLI prefix override or
implicit default, and every deployment takes its prefix from the submitted manifest.

Prefixes match the encoded request path literally and case-sensitively on whole
segment boundaries, before function routing. `/demo` matches `/demo` and
`/demo/hello`, but not `/demolition/hello` or `/%64emo/hello`. The mount is removed
before Rocket routes the request; an exact mount request becomes `/`. The
remaining encoded path and original query are preserved. Function paths in the
manifest and the path seen by handlers are application-relative. For example,
`path-prefix: /demo` plus `http.path: /hello` serves `/demo/hello`, and the handler
receives `/hello`. Rocket's slash handling below applies after mount selection.

Distinct applications cannot own equal or nested prefixes, even if their
function routes differ. `/api` conflicts with `/api/orders`, while `/api/orders`
and `/api/users` may coexist. `/` overlaps every prefix, so it can only be used
by the sole deployed application. Identical function routes in nonoverlapping
applications are valid. Prefix conflicts fail deployment atomically, preserving
all existing applications.

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

### Shared application code

An optional `libs/mod.rs`, beside the `functions` directory, is included as
`pub mod libs` at the root of every generated handler crate. Handlers import
shared items through `crate::libs`. Declare child modules with ordinary Rust
`mod` declarations; `wr` does not discover individual files beneath `libs`.
Shared modules can use the same `crate::Request`, `crate::Response`, `crate::kv`,
and `crate::sql` bindings as handlers.

Setup, build, and update check for the entry point. If it is absent, no shared
module is generated, even when the `libs` directory contains other files. An
entry point that exists but is not a file, or cannot be inspected, fails
generation before writing. Cargo validates the module contents during build.
Adding or removing the entry point is reflected at the next generation.

Shared code is compiled separately into each handler, not into a separate
component or a shared runtime instance. Dependencies used by shared modules
must be available in every handler that compiles them; declare these in the
manifest's top-level `dependencies`. Source-only changes require a rebuild but
no lockfile update. The library sources remain author-owned and are preserved
by setup and clean. Deployed components do not need the source directory.
See [reuse code between handlers](guides.md#reuse-code-between-handlers).

### HTML responses and templates

Handlers can render HTML through an application dependency and return its UTF-8
bytes with a `Content-Type: text/html; charset=utf-8` header. The runner uses the
ordinary HTTP response contract; it has no template-engine setting or rendering
host import. The [Askama example](guides.md#render-html-with-askama) embeds its
templates at compile time. Setup generates Askama's default configuration in
each handler crate, pointing to the application's `templates` directory. This
file is unused by handlers without Askama; no manifest flag or directory
discovery is needed. Application-owned HTML templates are preserved by setup
and clean, and are not sent separately at deploy.
Template changes require rebuilding components and restarting or redeploying.
The guest's filesystem restrictions and complete-response buffering still apply.

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
| Deployment preparation | deploy | Read configured components, upload routes and bytes, then let the daemon validate and compile components and reject internal route collisions and overlapping application prefixes before publishing. |
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
| `.wr/functions/<name>/askama.toml` | Generated Askama configuration with `general.dirs = ["../../../templates"]`, relative to each handler crate. |
| `.wr/wit/world.wit`, `.wr/wit/deps/kv/store.wit`, `.wr/wit/deps/sql/database.wit` | Embedded HTTP, KV, and SQL guest contracts. |
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
`/Cargo.toml`, `/target/`, and `/.wr-data/`, adding a separating newline if necessary.
Existing patterns, including negations, are not interpreted or reordered.
Repeated setup does not duplicate these entries. No ignore rule is added for
`Cargo.lock`.

Setup adds `${workspaceFolder}/functions` to the
`rust-analyzer.vfs.extraIncludes` array in `.vscode/settings.json`, and adds
`${workspaceFolder}/libs` when `libs/mod.rs` is present, without duplicating
either path. Previously added paths are retained if the shared entry point is
removed. It preserves other include paths, unrelated settings, comments,
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
components or serve HTTP. KV and SQL capability enforcement belongs to the runtime.
Non-HTTP event handlers are unsupported.

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
This embedding entry point hosts at `/`; the CLI honors the manifest prefix.
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

The guest receives the encoded application-relative path after prefix removal, decoded named captures, and
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
wall-clock, and custom host interfaces other than the versioned KV and SQL imports are unsupported.
Unsupported imports and incompatible guest interfaces fail registration.
The [guest contract](../wit/README.md#execution-and-compatibility) defines interface
versioning and rebuild requirements.

Epoch interruption cannot forcibly terminate arbitrary host code or undo
external effects. Any future host I/O must honor the remaining execution
deadline. Memory quotas, streaming, hot reload, and custom
scheduling are outside this increment.

## Persistent application KV

Top-level `kv` is an optional boolean, default `false`. An enabled application's
functions share the keyspace named by its validated manifest `name`. Guests
receive only that capability, never a database path or a keyspace selector.
The versioned [KV WIT](../wit/deps/kv/store.wit) is exported to Rust handlers as
`crate::kv`; the HTTP export remains `local:http/handler@0.2.0`. Previously built
HTTP-only components continue to work. Upgrade the CLI and daemon together:
older daemons reject deployment messages containing the new capability flag.

### Operations and errors

`get` returns an optional typed value; empty bytes/text differ from absence.
`set` replaces both value and type. `delete` atomically returns whether a value
was removed. `increment` atomically adds one to a signed 64-bit integer and
returns that operation's committed result; a missing key becomes integer `1`.
Text or bytes produce `type-mismatch` and the maximum integer produces
`overflow`; neither changes the value. No implicit coercion, wrapping, or
saturation occurs. Get followed by set is not an atomic counter update.

Keys are case-sensitive UTF-8, 1–1,024 encoded bytes, without normalization.
Bytes and text allow 0–1,048,576 payload bytes, excluding the internal type tag.
Integers allow the full signed 64-bit range. Typed errors are `disabled`,
`invalid-key`, `value-too-large`, `type-mismatch`, `overflow`, and `storage`.
Errors contain no paths, keys, or values. The guest chooses their HTTP mapping.
The capability is checked before every operation, including invalid arguments.

### Storage ownership and preservation

| Host | Database directory | Keyspace |
| --- | --- | --- |
| Daemon | `<state-dir>/kv/` | Manifest name |
| `wr run` | `<application-directory>/.wr-data/kv/` | Manifest name |
| `server::serve_with_storage` | Explicit root argument | `application` |

The database opens lazily when preparing the first enabled application, before
route publication. Setup, build, update, previews, and disabled generations do
not open it. Managed paths reject symlinks and special files; Unix database
directories must be private (0700). Concurrent ownership is rejected, including
when capabilities remain alive after their owner is dropped. Storage errors
fail preparation and leave published routes unchanged. A failed preparation
may leave a new database or empty keyspace; it never clears existing values.

Changing routes, prefixes, components, or deployment source location preserves
the keyspace. Renaming selects another keyspace and retains the old one.
Removal and disabling retain data and handles. Old in-flight generations may
still write. Clean preserves `.wr-data/` and does not touch daemon data. There
is no reset/delete-database command, multi-process sharing, or automatic data
deletion. For offline backup, stop the owner and copy the entire database
directory. Deployments themselves still start empty after daemon restart.

### Durability and deadlines

Fjall uses default OS-buffer persistence, without a durable sync per mutation.
Successful writes are intended to survive a process crash under a healthy OS;
they are not guaranteed against OS crash or power loss. There is no bounded
loss window or runner-managed periodic sync. Graceful shutdown closes
admission, drains preparation and executions, explicitly syncs the shared
database with `SyncAll`, and reports failures before releasing ownership.

Accepted writes survive a later guest trap, timeout, invalid HTTP response, or
disconnect. A failed or unacknowledged call can have an unknown outcome;
retrying increment can increment twice. Calls are never automatically replayed.
Database calls can wait for I/O, locks, or maintenance. Runtime deadline checks
run before and after each call, but cannot interrupt native I/O. The execution
slot remains held until the call returns; timeout does not roll back mutations.

`runtime::Runtime::new` and `server::serve` remain storage-disabled.
`runtime::Runtime::with_store(timeout, Arc<KvStore>)` accepts an application
capability. Embedders obtain it from `persistence::Storage::application(name)`;
creating `Storage::new(root)` does no I/O. Drain all users, call `Storage::sync`,
and release capabilities before opening the same root again. The storage-enabled
server sibling accepts `(config, handlers, root, shutdown, on_ready)` and owns
this sync boundary.

### Inspection and alpha limits

`Storage::inspect` and daemon status expose retained
keyspaces, operation counts/failures, total and maximum elapsed microseconds,
estimated entry counts, database disk bytes, journal count, and storage errors.
The storage API and daemon status also expose explicit sync count, failures,
last duration, and result for the current host lifetime. These reset on restart;
they do not describe the previous daemon's shutdown. Actual shutdown outcomes and durations
are recorded in the daemon log. Foreground hosts log metadata every 30 seconds
and sync outcomes at shutdown. All counters reset on restart.
Existing keyspaces are discovered when the
database first opens after restart; inspection alone does not open storage.

Estimated entry counts may diverge substantially after updates/deletes.
Keyspace disk bytes measure its on-disk storage, excluding shared journals and
caches. A keyspace can report zero while writes are still buffered and recorded
in the shared journal. Database disk bytes include shared engine storage and
journal preallocation; they do not measure live payload size or necessarily
physical filesystem allocation. Shared journals, caches, I/O, and health are not
attributable to one application. There are no independent performance/failure
domains or runner-enforced application quotas. Engine buffering and backpressure
remain enabled. No scans or mirror maps are used for exact live-size metrics.

Scans, TTL, compare-and-set, guest transactions, named stores, remote
backends, replication, online backup, and a data browser are outside this API.

## Persistent application SQL

The optional `sql` object requires a boolean `enabled` field. The only other
field is `migration-target`, an optional integer from zero through 2^63 − 1.
There is no boolean shorthand. Omitting `sql` disables SQL; setting a target
while disabled is an error. Disabled applications neither load migration files
nor open their SQL database. SQL and KV are independent capabilities.

```yaml
sql:
  enabled: true
  migration-target: 3
```

### Migration files and history

Put UTF-8 SQL files directly in the application's `migrations/` directory.
Names are `0001-description.apply.sql` and optionally
`0001-description.rollback.sql`: at least four decimal digits, a hyphen, a
normal lowercase identifier, then the suffix. IDs are positive, fit a signed
64-bit integer, and are sorted numerically; gaps are permitted. Apply and rollback
names must agree. Duplicate IDs for either direction, non-regular files, unknown
filenames, empty files, and rollback files without an apply file are rejected.
A missing directory means an empty migration list. Each file is limited to
1 MiB; at most 1,000 migrations and 8 MiB of combined SQL text are accepted.

Run and deploy load these files alongside existing components. Deploy sends
structured migration records in its bounded control message, without source
paths or an archive. Setup, build, and update do not execute migrations.

The runner records each applied migration's ID, name, and separate SHA-256 hashes
of exact apply/rollback bytes in a protected database table. Once applied, names
and both files are immutable, including whitespace and line endings. Absence of
a rollback file is also recorded. A rollback retains the history record and its
hashes. Every deployment must include all recorded migrations, even ones already
rolled back; missing or changed history fails before any migration SQL runs.
New migrations cannot be inserted below the highest recorded ID.

With no target, migrate forward to the highest supplied ID; omission never
implicitly rolls back. An explicit target must name a supplied migration, or be
zero for the empty schema. Apply in ascending order; roll back in descending
order. A lower target requires rollback files for every step being undone.
Missing scripts fail before the transition begins. Deploying an older checkout
without newer history is not a rollback workflow: retain the migration files
and explicitly lower the target.

### SQL deployment and failures

The host validates configuration, components, routes, prefix ownership, and
migration history before executing user migration SQL. It may initialize an
empty database and its internal history during preparation. The complete schema
transition and history changes run in one transaction; failure rolls both back.
Files must contain transaction-compatible SQL and must not manage transactions.
The candidate generation is published only after a successful commit.

Migrations and competing publications/removals are serialized. Existing HTTP
requests continue against their retained generation. Authors must keep both
forward and rollback changes compatible with every generation still executing.
The runner cannot prove query/schema compatibility. Rollback SQL does not promise
to recover deleted data. Migration direction and code rollout order remain the
author's responsibility.

A committed migration is not undone by a later publication failure, shutdown,
lost response, or process failure. Foreground run can also commit migrations and
then fail to bind its listener. Retrying validates history and skips committed
steps. The migration allowance is 30 seconds, including waiting for the local
connection; SQLite progress checks interrupt query work but cannot bound native
filesystem stalls. Shutdown drains admitted preparation and execution work.

### SQL guest operations and limits

The [versioned SQL import](../wit/deps/sql/database.wit) is exposed as `crate::sql`.
`query` prepares and starts one read-only statement, returning a cursor resource.
The cursor's `columns` method returns column names; `next` returns one typed row
or `none` at exhaustion. The host retains at most one prefetched row and never
collects the whole result. `execute` runs one statement without returned columns
and returns its affected row count.
`batch` runs up to 100 such statements atomically and returns their individual
counts. `RETURNING` is not supported by these initial operations. Each statement
has positional parameters represented by null, signed integer, real, text, or
bytes. Nonfinite real parameters are rejected. No string interpolation is needed.

Schema changes are permitted only in migrations. Both guest and migration SQL
are denied access to runner history, other databases, extension loading, virtual
and temporary tables, PRAGMA settings, and explicit transaction/savepoint control.
The `_wr_` name prefix is reserved. This is an application capability, not a
filesystem path or connection-string API.

SQL text is limited to 1 MiB per statement and across a guest batch. A statement
accepts at most 256 parameters and 1 MiB of parameter payload. A cursor has at
most 128 columns and 1 MiB of column-name metadata; each individual row is limited
to 1 MiB of value payload, counting numeric and null values as eight bytes.
There is no total row-count or accumulated result-size limit. Exceeding a row
limit returns an error and releases the reader, never a truncated row. SQLite expression, statement-program, compound
query, and trigger-depth limits also apply. These limits are not a total memory
quota or a hard native-I/O deadline.

Each application owns one queued writer connection and a lazy pool of up to 16
read-only connections. Writers (including migrations) serialize; readers can run
concurrently with each other and the writer through SQLite WAL mode. Each cursor
retains a reader and a stable snapshot established during `query`. Writes made
by the same handler or other requests do not change an open cursor's snapshot.
A later query sees changes committed before its own snapshot starts.

A guest may hold at most four cursor resources. Dropping a cursor releases it
early; exhaustion or error releases the reader immediately, though the resource
handle still counts toward the four-handle limit until dropped. All remaining
cursors are destroyed on invocation return, trap, or timeout. Cursors cannot
survive into another invocation. Call `drop(cursor)` when a handler no longer
needs one. Repeated `next` calls on an exhausted cursor return `none`.

The first query waits for a reader when the pool is full. If the guest already
holds any cursor resource, a further query requires an immediately available
reader or returns `limit`; it cannot wait while retaining resources that might
be needed to unblock other nested queries. Writers and first-query reader waits
consume the invocation's existing deadline. An expired waiter performs no SQL.
Strict FIFO ordering is not guaranteed. Migration writer waits use their existing
30-second allowance. Native embedders use the public reader-pool limit when
planning nested queries; native `query` always waits within its supplied deadline.

The host's execution slots remain held while waiting, so existing admission
limits still apply. No guest call is replayed. `busy` remains available for
SQLite-level lock conflicts outside this connection coordination.
SQL progress checks use the invocation's remaining deadline and outer checks
reject late returns. Accepted commits survive a later trap, timeout,
or disconnect. A commit interrupted at the transport boundary can have an unknown
outcome; retries require application-level idempotency.

Typed errors are `disabled`, `invalid-statement`, `denied`, `constraint`, `busy`,
`limit`, `timeout`, and `storage`. Guest errors do not expose host paths or SQL
contents. The application chooses their HTTP mapping. Previously built HTTP-only
and KV components remain compatible. The cursor contract is
`local:sql/database@0.2.0`; components importing SQL 0.1.0 must be rebuilt and use
cursor iteration. Upgrade CLI and daemon together.

### SQL storage and inspection

SQLite is bundled with the runner. Each application has a separate database at
`<state-dir>/sql/<name>/data.sqlite` for the daemon, or
`<application-directory>/.wr-data/sql/<name>/data.sqlite` for foreground run.
The validated application name determines identity. Replacement, prefix changes,
removal, disabling, and cleanup preserve data; renaming selects another database.
Capabilities retain an exclusive ownership lock. Managed directories reject
symlinks/special files and require private Unix permissions. Invalid or locked
storage fails preparation rather than substituting empty data.

Connections use WAL journaling, FULL synchronous writer commits, enabled foreign
keys, and memory-backed temporary work. SQLite performs automatic checkpoints;
long-lived read snapshots can delay checkpoint progress and retain WAL data.
The invocation deadline and prompt cursor cleanup bound normal reader lifetimes;
there is no hard disk-stall bound. Durability depends on the filesystem
honoring SQLite's sync requests. There is no shared transaction with KV, online
backup service, or distributed coordination. For offline backup, stop all users
and copy the application's whole SQL directory.

The first enabled SQL application discovers retained databases under its host
root. Status inspection does not initialize unopened storage. Records show the
current migration ID, optional combined database/WAL file size, and in-memory
counts, failures, total time, and maximum time for each operation. `query` measures
preparation, acquisition, and first-row prefetch; `next` measures each row advance.
File size excludes the shared-memory index and does not estimate live row data. Statistics restart with the host;
migration history persists. Admin shows active and retained owners together.

Embedders use `persistence::sql::SqlStorage::new(root)`, obtain a capability through
`application(name)`, and explicitly call `SqlStore::migrate` before exposing new
code. `SqlStore::query` returns `SqlCursor`, whose `columns`, `next_row`, and
`close` methods expose iteration and early release. The cursor keeps its database
owner alive, and its supplied deadline covers the entire scan. `MAX_READ_CONNECTIONS`
exports the pool capacity. `runtime::Capabilities` carries optional KV and SQL
handles into `Runtime::with_capabilities`. Hosts own draining and capability lifetimes.

## Output and errors

Successful commands exit zero. Invalid arguments, manifest loading or validation
failures, and failed operations exit nonzero. Manifest failures produce no
readiness output. Diagnostics use parsing and validation library detail with
basic operation context; no particular error-location format is guaranteed.

## Admin UI

`wr admin` requires Unix and serves a read-only browser overview of the selected
daemon. It runs in the foreground on a separate HTTP listener. `--listen` takes
a numeric loopback IP and port, defaults to `127.0.0.1:0`, and accepts port zero
to select an available port. Non-loopback addresses are rejected. Rocket
configuration files and environment variables do not override these settings.

After binding, the command prints `Admin UI listening on http://<IP:PORT>`.
Open that URL in a browser; the command does not launch one automatically. A
bind failure exits with an error and no readiness message. The daemon need not
be running to start the UI. Selection follows the same `--state-dir`,
`WR_STATE_DIR`, and default-directory rules as
[daemon commands](#daemon-and-deployment). The UI does not create daemon state
or start the daemon.

`GET /` queries the private control socket for live status and returns HTML
showing the daemon PID, hosting address, and application names and path prefixes
sorted by name. Each application includes its deployed functions sorted by name,
with their HTTP methods and application-relative route paths (before the
application's path prefix is applied). Dynamic captures are displayed literally.
Each function shows its rolling minimum and maximum latency and links to
`GET /applications/<application>/handlers/<handler>`. This detail page shows
completed call count, minimum/maximum/mean latency, and counts by HTTP response
code. A handler that is no longer deployed returns an HTTP 404 page. Both views
use the [handler metrics](#handler-metrics) below.
The Daemon section includes database-wide KV statistics. Each application's
row groups its routes and a Keyspace file bytes column, which shows file size
for KV-enabled applications and a dash for deployed applications with KV off.
Other per-keyspace metrics are not displayed in the admin UI.
Names present only in retained
keyspaces also appear with their retained file size, marked Disabled with no active prefix or functions.
Deployed applications are marked Enabled, independently of their KV capability.
The application count includes both groups, each name appearing once.
The SQL column shows capability status, the database migration ID, database file
bytes, and operation failures, including retained databases. SQL schema state is
independent of the active generation. Metric definitions are in
[KV inspection](#inspection-and-alpha-limits) and [SQL inspection](#sql-storage-and-inspection).
Only deployed functions appear; source files without configured routes are not
deployed. An empty registry is shown explicitly. The browser reloads the page
five seconds after each load using an HTTP `Refresh` header, querying current metadata
without a Refresh button. Error pages use the same automatic refresh, allowing
recovery after a daemon restart. This refresh reloads the whole page.
The refresh URL explicitly preserves the page path and query, omitting any
fragment such as `#applications` so browsers fetch fresh metadata instead of
performing an in-page navigation. The first refresh after following a fragment
link therefore returns to the top of the page. `GET /style.css`
serves the bundled stylesheet. Rocket supplies HEAD fallback. No browser
JavaScript, external assets, application source directories, or local build
outputs are required.

The control exchange has a five-second deadline. Unavailable, malformed,
unexpected, or timed-out replies produce an HTTP 503 error page; a timeout
closes the control connection. Each later page request retries independently,
so the UI can recover after a daemon restart. Responses use `Cache-Control:
no-store`, and errors never display an earlier successful snapshot as current.
Metadata and diagnostic text are HTML-escaped.

The UI has no deployment, removal, or daemon lifecycle actions. SIGINT (Ctrl-C)
or SIGTERM closes its listener and allows in-flight metadata requests to finish
within their deadline. This leaves the daemon and its applications running.
The UI never occupies application routes, even when an application owns `/`.
It provides local inspection only, with no remote or multi-user administration
contract. See the [browser inspection recipe](guides.md#inspect-the-daemon-in-a-browser).

### Handler metrics

Each active handler keeps an in-memory five-minute window, aggregated into
one-second buckets using a monotonic clock. The window includes the current
second and the previous 299 seconds; calls may expire up to one second before
their exact five-minute age. Recording and inspection discard expired buckets,
including when a handler becomes idle. Memory is bounded by 300 buckets per
handler, independently of traffic volume. Only aggregate counts and timings
are retained; request paths, bodies, headers, and individual samples are not.

One call is counted when a matched handler finishes dispatch and produces an
HTTP response. Latency is measured from entry into the matched handler through
request adaptation, execution admission, component execution, and response
validation. It excludes the gateway's initial request upload and network
response delivery. Guest HTTP errors retain their status; host errors use their
mapped status (for example, 500 for traps/invalid responses, 504 for execution
timeouts, and 503 for rejected admission). HEAD fallback counts against the
selected GET handler. Unmatched requests, gateway rejections before handler
selection, and cancelled dispatches do not contribute. A completed dispatch
does not guarantee the client received the response.

Minimum, maximum, and arithmetic mean latency are reported in milliseconds.
An empty window has zero calls, no response code counts, and `null` latencies
(shown as a dash in the UI). These are window counts, not lifetime totals.
Metrics belong to the deployed generation: successful replacement resets them,
failed replacement preserves them, and removal or daemon restart discards them.
In-flight old-generation requests cannot contribute to the replacement's window.
The admin detail page refreshes and handles metadata failures exactly like the
overview; it does not maintain its own metrics cache.

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
The control payload contains the manifest's application name, required
`path-prefix`, the optional boolean `kv` flag (absent means false), the SQL configuration
and migration records, named method/path routes, and component bytes; source paths and
build dependencies are not sent.
The server independently validates names, methods, route syntax, component
compatibility, internal route collisions, and prefix ownership. Application and
handler names use the manifest identifier constraints. At least one handler is required.

Application names are deployment identities: deploying an existing name replaces
its complete prefix/route/component set, and renaming it creates another application.
Function routes use Rocket ranking and collision rules within each application.
Preparation, compilation, route validation, and prefix ownership checks finish
before SQL migrations and atomic publication. Failure leaves every active application
generation unchanged; a committed SQL migration can survive a later publication
failure. Publication and removal are serialized across migrations, with registry
locks held only for metadata access. See [SQL deployment](#sql-deployment-and-failures).
An acknowledged deployment is active for subsequent requests; a lost connection
can leave its outcome unknown, so inspect/redeploy before assuming failure.

Each request retains its selected application generation until completion. Old
components are released once no active registry or request references them.
Removal returns after route publication, without waiting for prior requests.
`remove` of an unknown name fails. The execution limits in [Run](#run) apply
globally across applications; there is no per-application fairness guarantee.
Deleting local build outputs does not affect deployed component bytes.

The control transport uses one request and response per Unix connection. Each
message is a four-byte big-endian byte length followed by UTF-8 JSON, capped at
128 MiB including JSON encoding. The request's `command` is `deploy`, `remove`,
`list`, `status`, or `stop`. Deploy includes `application: {name, path-prefix, kv, sql, migrations, handlers}`, where
each handler contains `name`, `method`, `path`, and `component` (an array of byte
values). Remove includes `name`. Responses use `result: ok`, `applications`
(with `applications`), `status` (with `pid`, `listen`, `applications`, `storage`, and `sql`), or `error`
(with `message`). Both application lists contain objects with `name`,
`path-prefix`, `kv`, `sql` (a boolean capability flag), and `functions` fields. `functions` is sorted by name and contains
objects with `name`, `method`, and application-relative `path` from the active
routes, plus `metrics`. The metrics object contains `window_seconds` (300),
`calls`, `latency_min_ms`, `latency_max_ms`, `latency_mean_ms`, and
`response_codes` (an object mapping numeric HTTP status strings to counts).
SQL status is a separate `sql` list of retained application database records,
with `name`, current `migration` ID, optional `disk_bytes`, and per-operation
statistics. Deployment `sql` is the manifest object; each migration record contains
`id`, `name`, `apply` text, and optional `rollback` text.
See [handler metrics](#handler-metrics) for measurement and lifecycle semantics.
Component bytes are not included in these metadata responses. CLI list
displays application names and prefixes; status also displays storage metadata.
Four control requests may run concurrently; excess connections
are closed. Client reads time out after 120 seconds; a timeout does not cancel a
server-side deployment. The control socket is local administration, not a remote
or multi-user authentication API.

Deployments are held only in memory. Each new daemon process begins empty; redeploy
the desired applications after a stop, crash, or restart. The state directory
contains lifecycle files and logs, not persisted application configuration.

After upgrading from a version without path prefixes, restart the daemon so
the CLI and control server use the same deployment and listing formats. Add an
explicit `path-prefix` to each manifest before redeploying.

After upgrading from a version without function metadata, restart the daemon
and redeploy applications before using the updated client. List and status
responses require `functions`; an older daemon's response produces a metadata
error rather than an empty function list.

After upgrading from a version without handler metrics, restart the daemon and
redeploy before using the updated admin client. Missing `metrics` is a metadata
error, rather than being presented as a zero-call window.
