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
call `curl http://127.0.0.1:8080/status` in another terminal. Stop and restart
the server after changing routes or rebuilding components.
You can also build an unlisted source without giving it a route. See
[source discovery](reference.md#source-discovery) for filename rules and the
requirement to keep at least one configured function in the manifest.

## Configure dependencies

Edit the application manifest, not generated Cargo manifests. To make
`serde_json` available to every function in the checked-in example, remove its
declarations from both `functions.echo.dependencies` and
`functions.inspect.dependencies`, then add this top-level map:

```yaml
dependencies:
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
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/functions"]
}
```

The source handlers live outside the generated Cargo package directories, so
rust-analyzer needs the extra include path to resolve them. Cargo needs no such
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
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/examples/functions"]
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

## Manually exercise failures and concurrency

The example application includes [misbehave](../examples/functions/misbehave.rs),
a deliberately failing handler for local manual testing. Its source and manifest
entry explicitly mark that purpose:

```sh
wr build examples
wr run examples --listen 127.0.0.1:9001
```

In another terminal, `/misbehave/ok` should always return `1`, demonstrating
fresh guest state. `/misbehave/trap` and `/misbehave/invalid` return 500;
`/misbehave/loop` returns 504 after the execution allowance. Subsequent
`/misbehave/ok` requests should still succeed:

```sh
curl -i http://127.0.0.1:9001/misbehave/ok
curl -i http://127.0.0.1:9001/misbehave/trap
curl -i http://127.0.0.1:9001/misbehave/invalid
curl -i http://127.0.0.1:9001/misbehave/loop
curl -i http://127.0.0.1:9001/misbehave/ok
seq 20 | xargs -P20 -I{} curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9001/misbehave/loop
```

With the default 16 execution slots, simultaneous looping requests should show
both 503 (overload) and 504 (admitted execution timeout). While loops are active,
Ctrl-C should drain admitted jobs before exiting. Also try cancelling a looping
client: its execution slot remains occupied until the guest times out. These
concurrency and shutdown checks are manual rather than timing-sensitive tests.

To check request-body timeouts, use a client that pauses during a body upload to
`/echo`, leaving bytes outstanding after the handler starts reading. Expect 408
when the [body-reading allowance](reference.md#run) expires, then verify a normal
`/hello` request still succeeds. Header handling uses Rocket's transport defaults
and is separate from this timeout.

## Manage multiple applications

On Unix, start one daemon and deploy each built application to it:

```sh
wr daemon start --listen 127.0.0.1:9000
wr deploy examples
wr apps list
wr daemon status
```

Build another application in its own directory, give it a distinct manifest
`name` and nonconflicting HTTP routes, then run `wr deploy <DIRECTORY>`. Both
applications use the same hosting address. Names identify applications; deploying
the same name replaces its complete set of routes and component bytes.

After changing an application's source or routes:

```sh
wr build examples
wr deploy examples
curl http://127.0.0.1:9000/hello
```

If deployment fails, correct the reported component or route error and redeploy;
the old application remains active. No daemon restart is needed. To remove the
example application (whose name is `hello`) or shut down the entire host:

```sh
wr remove hello
wr daemon stop
```

Each new daemon process begins empty. Redeploy the desired applications after a
restart. Repeating `wr daemon start` against a running daemon preserves its
applications and reports its current address. `wr clean` affects local build
outputs, not the running deployment.
For separate development hosts use a private, short `--state-dir <DIRECTORY>`
consistently on every command and a different hosting port. See the
[daemon reference](reference.md#daemon-and-deployment) for state paths, logs,
limits, and failure behavior.

To manually verify generation lifetime, deploy the example, start a request to
`/misbehave/loop`, then redeploy or remove `hello` while it executes. The admitted
request should still end with 504 at its deadline; new requests use the new
routes (or return 404 after removal). To check shutdown draining, start the same
loop and run `wr daemon stop` while it executes. Stop should wait for that
execution to finish before reporting success. Repeat with a disconnected client
to check that its execution still holds capacity until the deadline.
