# Common application tasks

These recipes assume `wr` and its build prerequisites are installed as described
in [Getting started](getting-started.md#prerequisites-and-installation). Commands
run from the **repository root** against `examples`; substitute your own
application directory when needed. Exact guarantees live in the
[command and manifest reference](reference.md).

## Add a function

Create `examples/functions/status.rs` with the minimal handler shown in the
[guest contract](../wit/README.md#write-a-handler). Its filename gives the
function its name. To configure a route, add this entry under the existing
`functions` map in [application.yaml](../examples/application.yaml):

```yaml
  status:
    http:
      method: GET
      path: /status
```

Run:

```sh
wr update examples
wr build examples
wr run examples
```

Build should report `.wr/components/status.wasm`. After the readiness message,
call `curl http://127.0.0.1:8080/demo/status` in another terminal. Stop and restart
the server after changing routes or rebuilding components.
You can also build an unlisted source without giving it a route. See
[source discovery](reference.md#source-discovery) for filename rules and the
requirement to keep at least one configured function in the manifest.

## Reuse code between handlers

Create `libs/mod.rs` beside the application's `functions` directory. Declare
shared modules there using ordinary Rust declarations:

```rust
pub mod responses;
```

Put the implementation in `libs/responses.rs`. The checked-in
[response helper](../examples/libs/responses.rs) returns `crate::Response`, so
handlers can call it without converting between different response types:

```rust
use crate::{Request, Response, libs::responses::text};

pub fn handle(_request: Request) -> Response {
    text(200, "Hello from shared code!")
}
```

The [counter](../examples/functions/counter.rs) and
[SQL greeting](../examples/functions/sql-greet.rs) both use this helper.
Run `wr build examples` and restart or redeploy to try the changed components.
No YAML entry is needed for a shared module. If shared code needs an external
crate, put it in the common dependencies and run `wr update examples` before
building. See [shared-code behavior](reference.md#shared-application-code) for
discovery, ownership, and compilation boundaries.

## Render HTML with Askama

The example application serves a SQL-backed HTML page at `/demo/`. Its
[home handler](../examples/functions/home.rs) calls the shared
[greeting query](../examples/libs/greetings.rs), then passes the results to the
[view](../examples/libs/views.rs). The query selects up to 100 greetings in ID
order. An empty database shows an empty state; an unavailable query returns
HTTP 503. Template rendering errors return HTTP 500.

Askama is an ordinary application dependency. The example declares it in the
common `dependencies` because every handler includes the shared view module:

```yaml
dependencies:
  askama: {version: "0.16", default-features: false, features: [derive, alloc, config]}
```

Keep HTML in an application-owned `templates` directory. The example's
[home template](../examples/templates/home.html) extends its
[base layout](../examples/templates/base.html). Askama compiles both into the
component and escapes interpolated HTML data by default. No template files or
filesystem access are needed at runtime, and the page needs no browser JavaScript.

Setup generates Askama configuration inside each handler crate, pointing to
the application's `templates` directory. Each derived view needs only the
template name:

```rust
#[derive(askama::Template)]
#[template(path = "home.html")]
struct Home<'a> {
    messages: &'a [String],
}
```

There is no application-root Askama configuration to maintain. Keep the HTML
templates outside `.wr` so they survive cleanup; the generated configuration
is recreated by setup or build. See the [generated-file contract](reference.md#generated-files).
Handlers call `views::home(&messages)`. The
[view module](../examples/libs/views.rs) owns template construction and renders
it into the existing response type with
`Content-Type: text/html; charset=utf-8`.

To try the checked-in page:

```sh
wr build examples
wr run examples --listen 127.0.0.1:9000
```

Open `http://127.0.0.1:9000/demo/`. With a fresh example database the page shows
`Hello from SQL!`, seeded by the existing migration. For a new application or
changed dependency declarations, run `wr update examples` before building.
Edit a template, rebuild, and restart the foreground server to see the change;
in production, rebuild and publish an authorized new version. There is no template hot reload.
Invalid templates fail compilation without replacing the published components.

## Configure dependencies

Edit the application manifest, not generated Cargo manifests. To make
`serde_json` available to every function in the checked-in example, remove its
declarations from both `functions.echo.dependencies` and
`functions.inspect.dependencies`, then add it to the existing top-level map,
keeping the Askama dependency used by the shared views:

```yaml
dependencies:
  askama: {version: "0.16", default-features: false, features: [derive, alloc, config]}
  serde_json: "1.0"
```

Do not leave it at both levels: that is an error. To keep dependencies local,
retain the example's separate function-level declarations instead. For `echo`:

```yaml
    dependencies:
      serde_json: {version: "1.0", default-features: true}
```

Preview, update the lockfile, and build after editing:

```sh
wr setup examples --dry-run
wr update examples
wr build examples
```

The preview should show a shared dependency in the generated workspace and
inherited by each function. A local dependency appears in its owning function's
generated package.
Setup checks declaration conflicts; update asks Cargo to resolve dependency versions.
Preserve the resulting application `Cargo.lock` in version control. See
[dependency constraints](reference.md#dependencies), including the reserved
`wit-bindgen` dependency.

## Prepare VS Code

Run `wr setup examples`, then open the **examples directory itself** as the
VS Code workspace. With rust-analyzer installed, its root `Cargo.toml` is
discoverable. Setup merges this setting into the application's
[editor settings](../examples/.vscode/settings.json):

```json
{
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/functions", "${workspaceFolder}/libs"]
}
```

The source handlers and shared modules live outside the generated Cargo package
directories, so rust-analyzer needs the extra include paths to resolve them. Cargo needs no such
setting. Use absolute paths for extra includes; the VS Code extension expands
`${workspaceFolder}`. A bare relative path such as `functions` is not suitable.
See the upstream [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html)
for `vfs.extraIncludes` and `linkedProjects`.

Remove an old `rust-analyzer.linkedProjects` entry pointing at `.wr/Cargo.toml`.
The workspace root is now the application's `Cargo.toml`.

If you prefer opening the parent repository, merge these settings in the
**parent workspace's** settings instead:

```json
{
  "rust-analyzer.linkedProjects": ["Cargo.toml", "examples/Cargo.toml"],
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/examples/functions", "${workspaceFolder}/examples/libs"]
}
```

This explicitly loads both host and guest projects. Nested application settings
do not apply to the parent window. Preserve any other projects or include paths
you already have. If hover information is missing after setup, run
**rust-analyzer: Restart server** from the Command Palette. You should then be
able to inspect the generated `Request` and `Response` types in a handler.

## Make a locked or release build

Builds are locked by default. Create the application's lockfile with `wr update`
before its first build, or update it deliberately after changing dependencies or
adding/removing functions. For an application with an up-to-date lockfile, use:

```sh
wr build examples
wr build examples --release
```

Keep `examples/Cargo.lock` in version control. A normal build fails if the lockfile
is missing or dependency resolution would require changing it. Source-only edits
do not need a dependency update.

To pull newer versions allowed by your manifest requirements:

```sh
wr update examples
wr build examples
```

Review the lockfile changes after update. Update does not build or change published
components, so rebuild and restart the server to use the new dependencies.
For an explicit one-off build that allows necessary lockfile changes, use
`wr build examples --unlocked`. Unlike update, this does not force upgrades of
dependencies already satisfied by the lockfile.

Both profiles publish to the same `.wr/components/<name>.wasm` paths, so the
last successful build supplies those files. Cargo failure preserves previously
published components; a failure while copying can leave a partially updated
set. See [build behavior](reference.md#build) for the exact boundary.

## Clean and recover generated state

```sh
wr clean examples
```

Expect `Workspace cleaned in <application-path>`. Cleanup removes the generated
workspace and build outputs, including published components. It retains source
files, the manifest, lockfile, ignore rules, and editor settings. It works even
if the manifest is missing or invalid; repeated cleanup succeeds.

If a handwritten root Cargo manifest blocks cleanup or setup, preserve it and
choose a separate application directory, or move it aside yourself after
reviewing it. Do not add the generated-file header to user-authored content to
bypass protection. Cleanup failures can leave partial results; inspect the
reported path and correct the filesystem problem before retrying.

Regenerate with `wr setup examples` for editor preparation or
`wr build examples` for components. See [cleanup behavior](reference.md#clean)
for the exact removal and preservation set.

## Use a persistent counter

The example manifest enables `kv: true`. Its [counter](../examples/functions/counter.rs)
uses `crate::kv::increment("visits")` and returns the committed count:

```sh
wr update examples
wr build examples
wr run examples --listen 127.0.0.1:9001
# In another terminal:
curl http://127.0.0.1:9001/demo/counter
curl http://127.0.0.1:9001/demo/counter
```

The second request returns one more than the first. Stop gracefully and run
again: the count continues. `wr clean examples` preserves the data; rebuild
before serving again. Production uses its selected state directory. Changing the
application name starts a different counter and retains the old keyspace.
See [KV reference](reference.md#persistent-application-kv) before designing
retries or backing up data.

## Observe KV storage

Start with `--admin-listen 127.0.0.1:9002` and open that address to inspect
shared database statistics, retained storage owners, and keyspace sizes. The
library's `Storage::inspect` API additionally exposes operation totals and entry
estimates. Successful graceful shutdown logs the durable-sync result.

For an alpha latency check, run the counter above on a representative filesystem
and collect request durations (for example `curl -w '%{time_total}\n' -o /dev/null`).
Repeat sequentially, then with concurrent clients and several applications.
Compare median, p95, p99, and maxima with the operation totals/maxima; HTTP
durations include guest instantiation and transport. Repeat on a disposable
slow or pressured filesystem, recording hardware, workload, and engine version.
Watch for stalls and shared effects rather than treating namespaces as I/O
isolation. No production latency target is established by the automated suite.

On disposable data only, verify that killing the process after an acknowledged
write and redeploying recovers the value under a healthy OS. Graceful stop must
log successful sync before a whole-directory offline backup. OS/power-loss
durability requires a separate controlled environment and explicit successful
sync; a normal process restart does not test it. Read-only/corrupt/locked
storage must reject preparation while preserving any current routes. Never
corrupt or stress production storage to perform these checks.

## Manually exercise failures and concurrency

The example application includes [misbehave](../examples/functions/misbehave.rs),
a deliberately failing handler for local manual testing. Its source and manifest
entry explicitly mark that purpose:

```sh
wr build examples
wr run examples --listen 127.0.0.1:9001
```

In another terminal, `/demo/misbehave/ok` should always return `1`, demonstrating
fresh guest state. `/demo/misbehave/trap` and `/demo/misbehave/invalid` return 500;
`/demo/misbehave/loop` returns 504 after the execution allowance. Subsequent
`/demo/misbehave/ok` requests should still succeed:

```sh
curl -i http://127.0.0.1:9001/demo/misbehave/ok
curl -i http://127.0.0.1:9001/demo/misbehave/trap
curl -i http://127.0.0.1:9001/demo/misbehave/invalid
curl -i http://127.0.0.1:9001/demo/misbehave/loop
curl -i http://127.0.0.1:9001/demo/misbehave/ok
seq 20 | xargs -P20 -I{} curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9001/demo/misbehave/loop
```

With the default 16 execution slots, simultaneous looping requests should show
both 503 (overload) and 504 (admitted execution timeout). While loops are active,
Ctrl-C should drain admitted jobs before exiting. Also try cancelling a looping
client: its execution slot remains occupied until the guest times out. These
concurrency and shutdown checks are manual rather than timing-sensitive tests.

To check request-body timeouts, use a client that pauses during a body upload to
`/demo/echo`, leaving bytes outstanding after the handler starts reading. Expect 408
when the [body-reading allowance](reference.md#run) expires, then verify a normal
`/demo/hello` request still succeeds. Header handling uses Rocket's transport defaults
and is separate from this timeout.

## Manage multiple applications

For runner installation, service supervision, upgrades, and state recovery, use
the [operations guide](operations.md). This recipe covers app registration and
artifact publication.

Production uses `wr start` under your process manager. Keep a deployment file
in a public repository:

```yaml
applications:
  greetings:
    repo: https://example.com/team/greetings.git
    branch: main
    path: application.yaml
```

Run the foreground host:

```sh
wr start --config-repo https://example.com/team/deploy.git \
  --config-path config.yaml --config-branch main --state-dir /var/lib/wr \
  --listen 127.0.0.1:9000 --admin-listen 127.0.0.1:9001
```

Each application's name must match its registration and its `path-prefix` must
not overlap another active application. Its manifest declares `publish.registry`
and `publish.version` as shown in the [release reference](reference.md#package-and-release-format).
Merging that manifest to the registered branch authorizes deployment; the host
waits for its complete release. Remove a registration to withdraw its routes.
Existing data is retained. The process manager owns restarts; WR restores accepted
state and resumes polling.

CI builds from the intended commit, then exports OCI layouts:

```sh
wr build ./greetings --release
wr package ./greetings --source-commit <FULL_COMMIT_SHA> --output ./release-output
```

CI owns registry write credentials. Copy each function layout, the optional
migration layout, and finally the release layout to the deterministic registry
paths. For a `greetings` application with one `hello` function and SQL enabled,
using ORAS:

```sh
oras cp --from-oci-layout ./release-output/functions/hello:1.4.0 ghcr.io/example/greetings/functions/hello:1.4.0
oras cp --from-oci-layout ./release-output/migrations:1.4.0 ghcr.io/example/greetings/migrations:1.4.0
oras cp --from-oci-layout ./release-output/release:1.4.0 ghcr.io/example/greetings/release:1.4.0
```

Use a CI step that stops on any failed copy, so the release is never published
after a failed child upload. Copy every configured function. Preserve manifests
and digests; use registry immutability and serialize conflicting publishers.
[ORAS copy](https://github.com/oras-project/oras/blob/main/cmd/oras/root/cp.go)
accepts OCI layouts directly. ORAS is a CI choice, not a WR runtime prerequisite.

## Inspect the host in a browser

Add `--admin-listen 127.0.0.1:9001` to `wr run` or `wr start`. The host prints
the bound admin URL. Its overview shows applications, prefixes, handler links,
KV/SQL state, and production desired/active versions and errors. Follow a handler
for rolling call counts, latency, and response codes. Pages refresh automatically.

The admin listener is separate from application routing, including root mounts.
Only loopback binding is supported. `/status` returns reconciliation JSON,
`/live` reports process liveness, and `/ready` reports application readiness.
An upstream error can coexist with healthy cached applications. Ctrl-C stops the
whole foreground host; there is no separate admin process.

## Use SQL and deploy schema changes

Enable SQL in the application manifest:

```yaml
sql:
  enabled: true
```

Put schema SQL in `migrations/0001-greetings.apply.sql` and, if reversible,
`migrations/0001-greetings.rollback.sql`. The [example apply script](../examples/migrations/0001-greetings.apply.sql)
and [rollback script](../examples/migrations/0001-greetings.rollback.sql) create
and remove a greeting table. The [SQL handler](../examples/functions/sql-greet.rs)
uses the generated `crate::sql` bindings and has no migration machinery.
`sql::query` returns a cursor; `cursor.next()` returns one optional row at a time.
Drop it early if you stop iterating. Read cursors retain their original snapshots
while other readers and the writer proceed. The full result is never buffered
by the host. Rebuild old SQL 0.1.0 handlers for the cursor interface.

```sh
wr update examples
wr build examples
wr run examples --port 9000 --admin-listen 127.0.0.1:9001
# In another terminal:
curl http://127.0.0.1:9000/demo/sql/hello
```

Local run applies migrations to its application data directory. Production
applies the migration records in its authorized OCI release. Build stays
separate from serving; packaging reads current SQL files and existing components. No SQLite CLI or SQL server installation is required.

For the next schema change, add a new numbered apply file rather than editing
an applied file. Use additive changes while older handlers may still execute.
A deployment can change only the schema, only the code, or both. To target an
older schema, retain all migration files and set an explicit target:

```yaml
sql:
  enabled: true
  migration-target: 1
```

Deploy code compatible with that schema and with the transition. Every migration
above the target must have its original rollback file. Rollbacks can destroy
data and do not guarantee that an old application version can run. If deployment
fails after a migration commit, inspect the database migration ID in status/admin;
retries skip already committed work. See the [SQL reference](reference.md#persistent-application-sql)
for immutable hashes, transaction restrictions, contention deadlines, and backup rules.
