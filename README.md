# WASM Runner

WASM Runner is a Rust CLI for building and serving WebAssembly HTTP function
components from an application manifest. The command is `wr`. It is for
application authors working with Rust handlers and HTTP route configuration.
Handlers can [reuse application code](docs/guides.md#reuse-code-between-handlers)
through an optional shared Rust module tree.
The example includes a [SQL-backed HTML page rendered with Askama](docs/guides.md#render-html-with-askama).

`wr setup` prepares a guest Cargo workspace, `wr update` creates or refreshes
the application's dependency lockfile, `wr build` compiles components using
locked dependencies by default, and `wr clean` removes generated files and build
outputs. `wr run <PATH> --port 8081` serves one local application in the
foreground. `wr start` follows a public Git deployment configuration and
application manifests, pulls complete OCI releases, and reconciles application
membership and versions without restarting the host. A process manager owns
startup and restarts. Verified releases and accepted configuration survive
restart in a locked state directory.

Both commands support a separate, read-only loopback admin listener via
`--admin-listen`, with application metadata, storage inspection, handler metrics,
and health endpoints. See [production deployment](docs/guides.md#manage-multiple-applications)
and [browser inspection](docs/guides.md#inspect-the-host-in-a-browser).
`wr package` produces local OCI layouts for CI to upload; WR does not push
artifacts or require private repository credentials. The library continues to
expose a reusable runtime and HTTP server.

## Get started

Follow [Build your first application](docs/getting-started.md) to install the
CLI, build the checked-in example, and call its HTTP endpoints. For build errors
involving Cargo or the WASM target, start with the
[prerequisites and installation](docs/getting-started.md#prerequisites-and-installation).

## Documentation

| I want to… | Read |
| --- | --- |
| Build my first application | [Getting started](docs/getting-started.md) |
| Add functions or dependencies, configure VS Code, or clean outputs | [Common tasks](docs/guides.md) |
| Look up commands, validation, generated files, or failure guarantees | [Command and manifest reference](docs/reference.md) |
| Write a Rust guest handler | [Guest contract](wit/README.md) |
| Change and verify the runner | [Contributing](CONTRIBUTING.md) |
| Understand scope and acceptance criteria | [Product requirements](SPEC.md) |
| Understand code boundaries and design choices | [Architecture](ARCHITECTURE.md) |

The [example application](examples/application.yaml),
[manifest schema](application.schema.json), and [WIT world](wit/world.wit) supply
concrete examples and exact interface definitions. The guest interface is custom,
not WASI HTTP. Applications can opt into [persistent typed KV storage](docs/reference.md#persistent-application-kv),
including atomic counters, and [embedded SQL with streaming cursors and deployment migrations](docs/reference.md#persistent-application-sql).
Non-HTTP event handling is not implemented.

## Shareable agent skill

The [wasm-runner skill](skills/wasm-runner/SKILL.md) guides application authors
through application setup, handlers, available features, building, and serving,
assuming `wr` is already installed.
Share or copy the entire [skill folder](skills/wasm-runner), including its
references and agent metadata, into your agent's skills directory. For Codex,
use `$CODEX_HOME/skills/wasm-runner` (normally `~/.codex/skills/wasm-runner`).
Invoke it with, for example, “Use $wasm-runner to create and run a greeting API.”
The skill is portable; installing it does not install the `wr` CLI or Rust tools.
