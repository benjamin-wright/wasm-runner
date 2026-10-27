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

## Readability and scope

Use descriptive names, sensible whitespace, and straightforward control flow.
Keep comments sparse: explain non-obvious reasons or constraints without
narrating the code.

Give abstractions cohesive responsibilities and separate orchestration from
implementation details. Choose the smallest coherent solution: fewer concepts
and clearer reasoning, without compressed code or speculative generalization.
Prefer existing tools and documented prerequisites for responsibilities outside
the project. When complexity is challenged, reconsider the design before adding
another special case.

## Testing principles

Test meaningful behavior through public interfaces and major workflows.
Do not add a regression test automatically for every change. Preserve useful
tests across behavior-preserving refactors; avoid assertions coupled to private
implementation details. Prefer real collaborators and focused fixtures.

## Development and completion

Use the lifecycle commands in [Makefile](Makefile) and the prerequisites in
[README.md](README.md). Keep project-specific setup durable, idempotent, and
reviewable through `make setup` and version-controlled configuration. Report
environment blockers instead of requesting ad hoc installations or working
around execution restrictions.

After changes to Rust code, dependencies, or build configuration, run `make check`
before declaring completion. For documentation-only changes, check the relevant
links and instructions. Report the checks actually run, their results, and any
checks left unverified because of a blocker.
