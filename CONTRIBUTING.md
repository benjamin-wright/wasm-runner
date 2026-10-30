# Contributing to WASM Runner

This guide covers changing and verifying the runner and maintaining its
documentation. Read the [product requirements](SPEC.md) for scope, the
[shared reference](docs/reference.md) for exact behavior, and the
[architecture](ARCHITECTURE.md) for code responsibilities and design decisions.

## Development workflow

Follow the shared [prerequisites](docs/getting-started.md#prerequisites-and-installation)
first. Run lifecycle commands from the **repository root**. Installing `wr` is
optional for contributor work; the debug binary below uses the current checkout.

| Target | Purpose |
| --- | --- |
| `make setup` | Install the pinned Rust toolchain, components, and target |
| `make update-deps` | Deliberately update the runner lockfile to compatible dependencies |
| `make build` (or `make`) | Compile a debug CLI |
| `make install` | Build and install `wr` from this checkout |
| `make check` | Check formatting, lint, and test the workspace |

For an edit/build/run loop:

```sh
make build
./target/debug/wr --help
./target/debug/wr build examples
./target/debug/wr run examples --listen 127.0.0.1:9000
make check
```

The installed `wr` changes only when you run `make install`. There is no
`make run` wrapper or `ARGS` variable. Alternatively, build and run in one step:

```sh
cargo run --locked -- run examples --listen 127.0.0.1:9000
```

Cargo options go before `--`; arguments after it go to `wr`. Use `cargo fmt`
to apply formatting, or `cargo test --locked` to run just the tests. Other
Cargo operations can be invoked directly as needed.

Dependency updates are separate from setup: run `make update-deps` only when
intentionally refreshing versions, then review the lockfile changes before
committing them. Build, install, and check use `--locked` and do not update the
runner lockfile. Guest builds have their own [lockfile behavior](docs/reference.md#build).

Keep project-specific setup durable, idempotent, and reviewable through
`make setup` and version-controlled configuration. Report environment blockers
instead of requesting ad hoc installations or bypassing execution restrictions.

### Local development MCP server

[scripts/dev_mcp.py](scripts/dev_mcp.py) exposes fixed development commands over
[MCP stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).
It needs Python 3.9 or newer on `PATH`, with no Python packages or Rust build
required to start. Cargo, rustup, and make use the same prerequisites and
`make setup` as ordinary development.

The checked-in [VS Code configuration](.vscode/mcp.json) registers
`wasm-runner-dev`. In a trusted checkout, use **MCP: List Servers** to start it,
then **Chat: Manage Tool Approval** to pre-approve its tools if desired. These
are [host-managed approvals](https://code.visualstudio.com/docs/agents/run/approvals);
the server does not change approval or sandbox policies. Other MCP clients can
launch `python3 /absolute/path/to/wasm-runner/scripts/dev_mcp.py` as a stdio server
and configure tool approval in that client.

| Tool | Operation |
| --- | --- |
| `cargo_fmt` | `cargo fmt --all` (writes formatting changes) |
| `cargo_deps` | `cargo fetch --locked` (downloads locked dependencies; does not update versions) |
| `cargo_build` | `make build` |
| `cargo_clippy` | `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| `cargo_test` | `cargo test --locked --workspace` |
| `cargo_check` | `make check` |

Every tool takes an empty argument object and runs in the checkout containing
the script, regardless of the client's working directory. Commands inherit
the server's environment and permissions. Builds and tests execute repository
and dependency code, so pre-approval means trusting that code as well as the
wrapper. No arbitrary command, argument, or directory is accepted.

Calls run one at a time and return the exit status and combined stdout/stderr
after completion; a nonzero exit or launch failure is reported as a tool error.
There is no streaming or cancellation support. Set the client's tool timeout
long enough for a cold build. Dependency version updates remain a deliberate
`make update-deps` operation.

Verify the MCP transport, fixed commands, working directory, and error reporting
without invoking Cargo:

```sh
python3 -B -m unittest discover -s tests -p 'test_dev_mcp.py'
```

## Engineering principles

Use descriptive names, sensible whitespace, and straightforward control flow.
Keep comments sparse: explain non-obvious reasons or constraints rather than
narrating the code.

Give abstractions cohesive responsibilities and separate orchestration from
implementation details. Choose the smallest coherent solution with fewer
concepts and clearer reasoning, without compressed code or speculative
generalization. Prefer existing tools and documented prerequisites for work
outside the project. When complexity is challenged, reconsider the design
before adding a special case.

Embed bundled text assets with `include_str!` rather than reading them from disk
at runtime. Declare embeddings as descriptively named module-level `const`
values near the top of the file, after imports, rather than inline. Apply the
same convention to test fixtures.

## Verification

Test meaningful behavior through public interfaces and major workflows. Do not
add a regression test automatically for every change. Preserve useful tests
across behavior-preserving refactors, avoid assertions coupled to private
implementation details, and prefer real collaborators with focused fixtures.
The [requirements](SPEC.md#implemented-requirements) link acceptance conditions
to existing CLI coverage in [tests/cli.rs](tests/cli.rs).

After changes to Rust code, dependencies, or build configuration, run
`make check` before declaring completion. For documentation-only changes, check
relevant links, examples, and instructions; walk through affected user tasks
when the environment permits. Report checks actually run, results, and any
checks left unverified because of a blocker. No new tooling is required solely
to enforce word counts or document formatting.

## Documentation ownership

Each detailed fact has one maintained home. Other pages can briefly summarize
it and link to the owner. The schema and WIT define their machine-readable
contracts; prose explains their use and limitations rather than reproducing them.

| Page or definition | Owns |
| --- | --- |
| [README](README.md) | Current capability summary and reader navigation |
| [Getting started](docs/getting-started.md) | Installation and a guided first build with observable outcomes |
| [Common tasks](docs/guides.md) | Recipes for authoring, dependencies, editor use, builds, and recovery |
| [Reference](docs/reference.md) | Exact commands, manifest model, validation stages, generated files, preservation and failure behavior |
| [Guest notes](wit/README.md) and [WIT](wit/world.wit) | Handler authoring and the authoritative guest interface |
| [Schema](application.schema.json) | Manifest field shapes and constraints |
| [Examples](examples) | Concrete applications and handlers; generated files are not a separate specification |
| [Requirements](SPEC.md) | Product outcomes, acceptance conditions, exclusions, and open decisions |
| [Architecture](ARCHITECTURE.md) | Responsibilities, important flows, constraints, and implementation rationale |
| [Contributing](CONTRIBUTING.md) | Development, engineering, verification, and documentation practices |
| [Makefile](Makefile) and [toolchain configuration](rust-toolchain.toml) | Executable lifecycle commands and toolchain selection |
| [Agent instructions](AGENTS.md) and [skills](.agents/skills) | Agent operating rules and conditional procedures that use these shared documents |

### Maintain documentation with each change

- A behavior change updates the applicable requirement, reference section, and
  affected examples or guides together.
- An internal refactor updates architecture only where responsibilities, flows,
  or significant rationale change. It does not require restating unchanged
  contracts elsewhere.
- A workflow change updates this guide and the affected agent procedure.
- A schema or WIT change updates its authoritative definition and adjacent
  explanation and examples.
- A capability-status change updates the short entry-point summaries, including
  package metadata when it affects CLI help.

During review, check that detailed facts have clear owners, summaries agree,
links and referenced assets resolve, and a reader can finish the documented
task. Preserve validation, ownership, and failure guarantees when moving
content. Keep accepted requirements distinct from future possibilities.
The dated [documentation review](DOCUMENTATION_REVIEW.md) is migration history,
not a maintained source of product behavior.
