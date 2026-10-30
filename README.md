# WASM Runner

WASM Runner is a Rust CLI for building and serving WebAssembly HTTP function
components from an application manifest. The command is `wr`. It is for
application authors working with Rust handlers and HTTP route configuration.

`wr setup` prepares a guest Cargo workspace, `wr build` compiles components,
and `wr clean` removes generated files and build outputs. After building,
`wr run` serves configured handlers on `127.0.0.1:8080` by default. The library
also exposes a reusable component runtime and HTTP server.

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
not WASI HTTP. SQL/KV support and non-HTTP event handling are not implemented.
