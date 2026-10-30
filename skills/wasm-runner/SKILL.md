---
name: wasm-runner
description: Create, build, and run Rust HTTP applications using an installed wr CLI. Use for application manifests, handlers, routes, dependencies, and application troubleshooting. Assumes wr is installed; excludes framework installation and contributor workflows.
---

# Build applications with WASM Runner

Help the user take an application from Rust handler sources and a YAML manifest
to built WebAssembly components and verified HTTP responses. Adapt the examples
to the user's application directory and preserve existing application inputs.

Assume `wr` is installed and ready to use. Work in the user's application;
no framework checkout is needed. Keep advice focused on creating and running
the application. Use `wr <command> --help` when checking available CLI options.

For routing, dependencies, application capabilities, and troubleshooting, consult
[Application features](references/features.md) as needed.

## Create or extend an application

Use a dedicated application directory containing `application.yaml` and
`functions/`. Application filesystem commands take this directory, not the manifest
filename; daemon commands manage the shared host. Create these application files directly; `wr` manages the Cargo
workspace. If an existing handwritten `Cargo.toml` blocks setup, preserve it
and use a separate application directory.

For a minimal application, create the following two files under `my-app/`.
This path is an example; substitute the actual application path in commands.

`application.yaml`:

```yaml
name: my-app
path-prefix: /my-app
functions:
  greet:
    http:
      method: GET
      path: /hello
```

`functions/greet.rs`:

```rust
use crate::{Request, Response};

pub fn handle(_request: Request) -> Response {
    Response {
        status: 200,
        headers: vec![("content-type".into(), "text/plain".into())],
        body: b"Hello, world!".to_vec(),
    }
}
```

The filename supplies the function name. Application and function names begin
with a lowercase letter and contain only lowercase letters, digits, or hyphens,
up to 64 characters. The manifest must configure at least one function, and each
configured function needs a matching source. Every direct `functions/*.rs` file
is built, even if unlisted; unlisted sources have no HTTP route. Discovery is
not recursive, so do not put unrelated Rust modules directly in this directory.

Handlers import `Request` and `Response` from `crate`; `wr` supplies the HTTP
server. Return expected application errors as `Response` values. Configure
dependencies in the YAML manifest rather than generated Cargo files.

## Preview, build, and serve

Run these commands from the application's parent directory, or use an absolute
application path:

```sh
wr setup my-app --dry-run
wr update my-app
wr build my-app
wr daemon start --listen 127.0.0.1:9000
wr deploy my-app
wr daemon status
```

The optional preview displays the proposed workspace without writing or compiling.
For a new application, `wr update` creates its dependency lockfile without building.
`wr build` performs setup automatically and uses the locked versions. Expect a
`Built` line for `.wr/components/greet.wasm`. Wait for `Listening on http://127.0.0.1:9000`
and `Deployed my-app` before sending requests. Deploy does not build missing or changed sources.
The daemon workflow requires Unix. For foreground serving, `wr run my-app`
remains available and stops with Ctrl-C.

After readiness, use another terminal to verify the endpoint:

```sh
curl -i http://127.0.0.1:9000/my-app/hello
```

Expect status 200 and `Hello, world!`. Stop with `wr daemon stop`.
After source or manifest changes, rebuild and redeploy the application.
Use `wr apps list` to inspect deployed names and `wr remove my-app` to remove one.
Use `wr admin` for a read-only browser overview of applications and their deployed
functions, HTTP methods, and relative route paths. Open its printed local URL and
refresh after deployment changes; Ctrl-C stops the UI while leaving the daemon
running. Use the same `--state-dir` selection as your other daemon commands.
Every manifest requires `path-prefix`: use `/my-app` to mount this application
or `/` for sole-application root hosting. Equal or nested prefixes conflict;
different applications may use identical relative function routes. Conflicting
deployments fail atomically. Prefixes are applied by both run and deploy;
handlers receive application-relative paths, and there is no CLI prefix override.
Daemon deployments are in memory, so redeploy after every daemon restart.
Use a consistent `--state-dir <DIRECTORY>` when targeting an isolated daemon.
There is no file watcher or automatic redeployment. The default listener is `127.0.0.1:8080`;
`--listen` requires a numeric IP and port, not a hostname.

Builds are locked by default: a missing or outdated application lockfile is an
error. Source-only edits need only a rebuild. To deliberately update dependencies
to newer versions allowed by the manifest, or after adding/removing functions, use:

```sh
wr update my-app
wr build my-app --release
```

Keep the application's `Cargo.lock` in version control and review changes after
update. Update leaves existing built components unchanged; rebuild and redeploy
to use new dependencies. `wr build my-app --unlocked` explicitly permits Cargo
to create or adjust the lockfile during a build, without forcing upgrades of
already-satisfied dependencies. Use `--release` for optimized builds. Debug and
release builds publish to the same paths; the last successful build supplies
those files.

## Editor setup and cleanup

Use `wr setup my-app` when only preparing an editor. Open the application
directory itself in VS Code with rust-analyzer. Setup prepares the workspace
and editor settings. Preserve unrelated settings. If opening a parent workspace
instead, its editor settings must include the application; nested application
settings do not apply to the parent window.

Make application changes in `application.yaml` and `functions/`. Leave generated
Cargo files and `.wr/` contents to `wr`.

When cleanup is requested or needed to remove obsolete generated outputs:

```sh
wr clean my-app
```

This removes `.wr/`, `target/`, and the generated root `Cargo.toml`, including
published components. It preserves the manifest, handler sources, lockfile,
ignore rules, and editor settings. It can work with a missing or invalid manifest.
Rebuild before serving again. Do not clean routinely to fix every build failure;
it discards incremental artifacts and existing components.

## Verify the requested outcome

Build and start the application, then check a representative HTTP request as the
task requires. A successful build alone does not guarantee the application will
start. Include an expected error response when changing input validation.
Report commands actually run, observed results, and any blocker.
