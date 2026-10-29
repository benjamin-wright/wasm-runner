# Engineering instructions

Apply these instructions when planning, implementing, testing, and reviewing
changes. Treat preferences as defaults; explain material tradeoffs that require
a departure. Keep the work within the user's requested scope.

## Working Process

Avoid interacting with Git directly, trust the user to have set up the environment
to disambiguate your changes from any existing work. If you do need some information
from Git, ask the user, you won't have access to those tools yourself.

## Decision points

Read and apply the linked skill when its condition holds. Both skills may apply
to one task; keep the reasoning and reporting proportional to the change.

- Before implementing a feature or refactor that adds abstractions, dependencies,
  configuration, or changes component responsibilities, use
  [wasm-runner-change-design](.agents/skills/wasm-runner-change-design/SKILL.md).
  Use it again when feedback identifies unnecessary complexity. Mechanical
  edits and straightforward local fixes do not need this design workflow.
- Before choosing verification for a behavior change or non-trivial refactor,
  or adding, changing, or removing tests, use
  [wasm-runner-test-selection](.agents/skills/wasm-runner-test-selection/SKILL.md).
  Documentation-only and mechanical edits do not need this testing workflow.

## Shared engineering policy

Follow the [engineering principles](CONTRIBUTING.md#engineering-principles),
[verification policy](CONTRIBUTING.md#verification), and
[documentation ownership rules](CONTRIBUTING.md#documentation-ownership).
These include readability, scoped design, embedding bundled text assets, and
testing through public interfaces. Keep shared policy in the contributor guide
rather than maintaining a second copy here.

Use [product requirements](SPEC.md) for scope and acceptance conditions,
[reference](docs/reference.md) for exact behavior, and
[architecture](ARCHITECTURE.md) for implementation responsibilities and rationale.

## Development and completion

Use the lifecycle commands in [Makefile](Makefile) and the
[development workflow](CONTRIBUTING.md#development-workflow), including its linked
prerequisites. Keep project-specific setup durable, idempotent, and
reviewable through `make setup` and version-controlled configuration. Report
environment blockers instead of requesting ad hoc installations or working
around execution restrictions.

After changes to Rust code, dependencies, or build configuration, run `make check`
before declaring completion. For documentation-only changes, check the relevant
links and instructions. Report the checks actually run, their results, and any
checks left unverified because of a blocker.
