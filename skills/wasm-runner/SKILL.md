---
name: wasm-runner
description: Create, build, and run Rust HTTP applications using an installed wr CLI. Use for application manifests, handlers, routes, dependencies, and application troubleshooting. Assumes wr is installed; excludes runner installation, production host administration, and contributor workflows.
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
filename for authoring commands; run and package also accept a manifest path. Create these application files directly; `wr` manages the Cargo
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

For shared application code, create an optional `libs/mod.rs` beside `functions`.
Declare child modules there and import their public items as `crate::libs::...`
from handlers. Shared modules can use the generated HTTP, KV, and SQL bindings.
They compile within every handler crate; put their dependencies in the common
manifest dependencies. Rebuild after source changes. These are ordinary Rust
modules, not separate Cargo packages or deployed components.
For simple HTML frontends, Askama can compile application-owned templates into
the component. See [HTML templates](references/features.md#html-templates) for
the generated configuration and dependency setup.

Handlers import `Request` and `Response` from `crate`; `wr` supplies the HTTP
server. Return expected application errors as `Response` values. Configure
dependencies in the YAML manifest rather than generated Cargo files.

For persistent values, set top-level `kv: true` and use generated `crate::kv`
bindings for typed get/set, delete, and atomic integer increment. Storage is
shared by all functions of the same application identity and survives removal
and cleanup. See [Application features](references/features.md#application-capabilities-and-limits).

For SQL, set `sql: {enabled: true}` and use generated `crate::sql` parameterized
query/execute/batch operations. Query returns a cursor: read its column names,
then call `next()` until it returns `None`, or drop it early. Readers use a bounded
pool and retain a snapshot while writes proceed on a separate connection.
SQL 0.1.0 handlers must rebuild for this API. Put schema changes in `migrations/NNNN-name.apply.sql`
and optional paired `.rollback.sql` files. The host runs them after candidate
validation; handlers contain no migration runner. Keep applied files unchanged
and preserve compatibility with overlapping generations. Optional nested
`sql.migration-target` selects an exact forward/rollback target; omission only
moves forward. See the project's SQL reference for limits and history rules.

## Preview, build, and serve

Run these commands from the application's parent directory, or use an absolute
application path:

```sh
wr setup my-app --dry-run
wr update my-app
wr build my-app
wr run my-app --port 9000 --admin-listen 127.0.0.1:9001
```

Preview displays proposed files; update creates or refreshes the dependency
lockfile. Build performs setup automatically and compiles locked components.
Run uses existing outputs and stops with Ctrl-C. In another terminal:

```sh
curl -i http://127.0.0.1:9000/my-app/hello
```

Expect status 200 and `Hello, world!`. After local changes, rebuild and restart.
The optional admin listener shows live metadata and rolling handler metrics.
Every application needs an explicit `path-prefix`; equal or nested prefixes
conflict in a multi-application host.

Production uses foreground `wr start --config-repo <PUBLIC_GIT_URL>
--config-branch main --config-path config.yaml --state-dir <DIRECTORY>` under a
process manager. The deployment file registers application repositories,
branches, and manifest paths. Changes to registered application manifests
authorize deployment. Each production manifest needs `publish.registry` and
`publish.version`; CI runs `wr package <PATH> --source-commit <SHA> --output <NEW_DIRECTORY>`
and uploads the resulting OCI layouts, release last. WR only reads public Git
and OCI endpoints, verifies digest-pinned releases, and retains the previous
application on failure. Removal preserves data. State directories are
exclusively locked; verified releases support recovery without upstream access.
See the project's production and packaging reference for the release contract.

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
