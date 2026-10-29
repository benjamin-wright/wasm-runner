# Product requirements

This document states product scope, intended outcomes, and observable acceptance
conditions. The [shared reference](docs/reference.md) is part of these
requirements: it defines the exact command, validation, filesystem, and failure
contracts. The [schema](application.schema.json) and [WIT world](wit/world.wit)
remain the authoritative field and interface definitions. Implementation
rationale belongs in [Architecture](ARCHITECTURE.md).

## Audience and problem

The current increment serves Rust application authors who need to turn function
sources and route configuration into WebAssembly components. The working product
rationale is to remove the repetitive work of assembling Cargo packages,
guest bindings, and component outputs while keeping source and configuration
under the author's control.

The longer-term ambition is to run those functions as HTTP endpoints. This
increment prepares and builds them; it does not provide a working application
server. Product priorities beyond this increment remain open decisions.

## Current scope

The supported workflow is to describe an application in YAML, author Rust
handlers against the guest contract, preview or write a generated workspace,
build components, inspect application metadata, and clean generated outputs.
Source discovery is independent of route configuration, within the
[manifest and discovery constraints](docs/reference.md#source-discovery).

## Implemented requirements

The following requirements preserve current public guarantees. Acceptance
conditions describe observable results, not private implementation details.
Existing evidence is identified in [tests/cli.rs](tests/cli.rs); that mapping
is not a claim that a particular test run has passed or exhaustive coverage of
every filesystem failure.

| ID | Requirement and reason | Observable acceptance condition and existing coverage |
| --- | --- | --- |
| R1 | Reject invalid application inputs before reporting successful application results, so authors can correct configuration before use. | Setup, build, and run reject invalid manifests and missing configured sources; run emits no metadata on those failures. Validation follows the [documented stages](docs/reference.md#validation-stages). Covered by `invalid_manifests_fail_before_printing_metadata`, `commands_require_a_directory_and_report_missing_function_sources`, and the YAML/schema cases. |
| R2 | Let authors inspect generated content before changing their application, and protect handwritten root Cargo manifests when writing. | Dry-run creates no files or directories and previews the same generated content used by setup. Writing refuses a handwritten root manifest before any writes. Covered by `setup_previews_workspace_without_writing_files`, `setup_writes_the_previewed_files_beside_the_manifest_and_can_be_repeated`, and `setup_preserves_an_existing_handwritten_cargo_manifest`. |
| R3 | Build a guest-contract component for every valid directly discovered source, including sources without route entries. | Successful builds publish every expected component with the [guest interface](wit/README.md); invalid discovered names fail setup. Covered by `setup_discovers_all_direct_rust_sources_and_rejects_invalid_names` and `build_produces_http_components_and_preserves_outputs_on_cargo_failure`. |
| R4 | Preserve previously published components when Cargo compilation fails, so a failed build cannot appear to publish a new successful result. | The command fails and leaves published components unchanged. Publication-copy failures are separately [nontransactional](docs/reference.md#build). Covered by `build_produces_http_components_and_preserves_outputs_on_cargo_failure`. |
| R5 | Remove only the documented generated workspace and build outputs during cleanup, preserving application inputs and user configuration. | The [cleanup set and ownership checks](docs/reference.md#clean) apply even with a missing or invalid application manifest. Missing outputs are tolerated, and removed directory symlinks are not followed. Covered by `clean_removes_generated_files_and_preserves_user_files`, `clean_refuses_a_handwritten_cargo_manifest_before_removing_anything`, `clean_does_not_require_a_valid_manifest_or_function_sources`, and `clean_removes_directory_symlinks_without_following_them`. |
| R6 | Make run's current metadata-only behavior explicit, so authors do not mistake it for a serving process. | Run reports validated application metadata and the execution/serving limitation without requiring component files or opening a listener. Covered by `run_prints_metadata_without_requiring_components_or_announcing_readiness`. |
| R7 | Preserve user configuration and support repeated setup without unnecessary churn, making the generated workspace usable during development. | Ignore rules and editor settings are merged as [documented](docs/reference.md#ignore-rules-and-editor-settings); repeated setup avoids duplicates and leaves identical generated files untouched. Invalid editor settings fail before writing. Covered by the setup repeatability, ignore-rule, editor-merge, and invalid-settings cases; incremental build reuse is checked in the component build case. |
| R8 | Let authors configure supported shared and per-function dependencies and repeat locked builds, while keeping guest dependency resolution separate from the runner. | Invalid declaration conflicts fail generation; debug/release builds use the [documented lockfile behavior](docs/reference.md#build), and `--locked` refuses a missing or outdated lockfile. Covered by `setup_rejects_invalid_dependencies_without_partial_output` and the component build case. |

Failure behavior, output paths, and preservation rules in the reference are
part of these commitments even where the table summarizes them. Filesystem
writes, removal, and publication are not promised to be transactional.

## Deliberate exclusions

This increment does not execute components, serve HTTP requests, implement a
host capability policy, provide SQL/KV host services, or handle event triggers.
The custom guest interface is not WASI HTTP. Prebuilt module declarations,
automatic prerequisite installation, file watching, and unsupported dependency
forms are outside the current contract. See the
[reference](docs/reference.md) for exact input constraints.

## Open decisions

No additional delivery dates, performance targets, supported-platform promises,
or accepted future implementation requirements are established by this migration.
HTTP serving is an ambition, not a completed capability or scheduled release.

- Confirm the working product rationale and the priority of the next user outcome.
- Define component execution, HTTP translation, resource limits, and the policy
  for WASI and other host imports before implementing a runtime.
- Decide whether and when non-HTTP event triggers or SQL/KV capabilities belong
  in scope. Separate event-specific worlds and shared capability imports are
  design possibilities, not approved features or binding architectural choices.

New requirements should state their rationale and observable acceptance
conditions, distinguish accepted work from ideas, and link to the relevant
contract and design decision. Follow the
[documentation maintenance rules](CONTRIBUTING.md#maintain-documentation-with-each-change).
