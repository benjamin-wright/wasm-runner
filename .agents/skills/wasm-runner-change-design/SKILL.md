---
name: wasm-runner-change-design
description: >-
  Design wasm-runner features and refactors that add abstractions, dependencies,
  configuration, or change component responsibilities. Reassess designs when
  feedback identifies unnecessary complexity. Skip mechanical edits and
  straightforward local fixes.
---

# Design a scoped change

Before implementing, identify the user's workflow, the requested behavior, and
the smallest change that can deliver it. Keep the rationale brief and
proportional to the decisions involved.

## Establish the contract and current state

Read the relevant implementation and tests. Use the
[contributor guide](../../../CONTRIBUTING.md) for development commands and
documentation ownership, [requirements](../../../SPEC.md) for scope and
acceptance conditions, [reference](../../../docs/reference.md) for exact
behavior, and [architecture](../../../ARCHITECTURE.md) for implementation
decisions. For guest-interface changes, also read the
authoritative [WIT world](../../../wit/world.wit) and its
[contract notes](../../../wit/README.md).

Distinguish planned behavior from implemented behavior. Implement the requested
slice; do not expand it to complete unrelated acceptance criteria. If the user
requests a contract change, update the affected documents alongside the code
according to the contributor guide's ownership rules.

## Choose responsibilities and the simplest approach

- Identify what `wr` must own and what existing tools or documented prerequisites
  already provide. Consult the [requirements](../../../SPEC.md) and
  [reference](../../../docs/reference.md) before treating a
  capability as new. Additional host capabilities require an explicit scope
  change rather than being incidental setup conveniences.
- Compare the proposed approach with reusing or simplifying the current code.
  Justify new abstractions and dependencies by the concrete responsibility or
  complexity they remove. Avoid generic providers and configuration reserved
  for hypothetical features.
- Keep orchestration separate from parsing, execution, and transport details as
  those responsibilities are implemented. Treat proposed module boundaries in
  the architecture as guidance; do not scaffold unused modules.
- Look for duplicated responsibilities or special cases that can follow the
  ordinary flow. Prefer removal or consolidation when it clarifies ownership.

Before editing, briefly state the chosen approach and any material tradeoff.
An existing progress update is sufficient; no separate design artifact is
required. If feedback exposes unnecessary complexity, return to the workflow
and responsibilities before patching the latest symptom.
