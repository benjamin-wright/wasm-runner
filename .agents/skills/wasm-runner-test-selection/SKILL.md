---
name: wasm-runner-test-selection
description: >-
  Choose verification for wasm-runner behavior changes and non-trivial refactors,
  or when adding, changing, or removing tests. Select meaningful public-interface
  and workflow coverage without redundant regression tests. Skip documentation-only
  and mechanical edits.
---

# Select meaningful verification

Identify the observable behavior affected by the change and inspect existing
coverage before deciding whether to add a test. Use the relevant acceptance
criteria and coverage map in [requirements](../../../SPEC.md#implemented-requirements),
the exact contracts in [reference](../../../docs/reference.md), and existing
cases in [tests/cli.rs](../../../tests/cli.rs). Limit verification to the requested
work and its effects; planned runtime features may not exist yet.

## Choose the boundary and cases

- Exercise non-trivial algorithms through the public interface of the responsible
  object or module. Do not expose private helpers solely to test their steps.
- Exercise major workflows through their real boundary. Follow the existing
  process-level pattern in [tests/cli.rs](../../../tests/cli.rs) for CLI behavior.
  When runtime or transport behavior is implemented, use small real components
  or real sockets where those interactions determine correctness.
- Add a regression test when it protects meaningful behavior, fills a coverage
  gap, and will remain useful if the implementation changes. Reuse existing
  coverage for behavior-preserving refactors; do not rewrite tests merely to
  match a new internal structure.
- Select a small set of cases that distinguish correct behavior from plausible
  failures. Keep assertions on outcomes and documented contracts; assert exact
  wording or ordering only when those are part of the contract.
- Prefer real collaborators and focused, reusable fixtures. If setup requires
  more stubs and mocks than test cases, reconsider the boundary before adding
  infrastructure. Keep total suite runtime suitable for frequent local use.

## Run and report

Run focused checks during iteration as useful. For changes to Rust code,
dependencies, or build configuration, finish with `make check`, which runs
formatting checks, Clippy, and tests through the project
[Makefile](../../../Makefile).

Follow the setup and blocker rules in [AGENTS.md](../../../AGENTS.md) if a check
cannot run. In the completion summary, name the behavior verified, the checks
actually run and their results, and any remaining gaps. Briefly explain a
material decision to rely on existing coverage instead of adding tests.
