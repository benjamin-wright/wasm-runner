# Product requirements

This document states product scope, intended outcomes, and observable acceptance
conditions. The [shared reference](docs/reference.md) is part of these
requirements: it defines the exact command, validation, filesystem, and failure
contracts. The [schema](application.schema.json) and [WIT world](wit/world.wit)
remain the authoritative field and interface definitions. Implementation
rationale belongs in [Architecture](ARCHITECTURE.md).

## Audience and problem

The current increment serves Rust application authors who need to build and
serve WebAssembly HTTP components from function sources and route configuration.
The working product rationale is to remove the repetitive work of assembling Cargo packages,
guest bindings, and component outputs while keeping source and configuration
under the author's control.

## Current scope

The supported workflow is to describe an application in YAML, author Rust
handlers against the guest contract, preview or write a generated workspace,
explicitly update dependencies, build components with locked dependencies by
default, serve configured HTTP endpoints, and clean generated outputs.
A reusable synchronous runtime and a CLI-independent HTTP server support
embedding applications as well as the CLI. On Unix, a background daemon hosts
multiple applications on one HTTP listener, with a private control socket for
atomic deployment, listing, removal, status, and shutdown. Deployments are
in-memory and must be resubmitted after restarting the daemon.
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
| R1 | Reject invalid application inputs before reporting successful application results, so authors can correct configuration before use. | Setup, build, update, and run reject invalid manifests and missing configured sources; run announces no readiness on those failures. Validation follows the [documented stages](docs/reference.md#validation-stages). Covered by `invalid_manifests_fail_before_printing_metadata`, `commands_require_a_directory_and_report_missing_function_sources`, and the YAML/schema cases. |
| R2 | Let authors inspect generated content before changing their application, and protect handwritten root Cargo manifests when writing. | Dry-run creates no files or directories and previews the same generated content used by setup. Writing refuses a handwritten root manifest before any writes. Covered by `setup_previews_workspace_without_writing_files`, `setup_writes_the_previewed_files_beside_the_manifest_and_can_be_repeated`, and `setup_preserves_an_existing_handwritten_cargo_manifest`. |
| R3 | Build a guest-contract component for every valid directly discovered source, including sources without route entries. | Successful builds publish every expected component with the [guest interface](wit/README.md); invalid discovered names fail setup. Covered by `setup_discovers_all_direct_rust_sources_and_rejects_invalid_names` and `build_produces_http_components_and_preserves_outputs_on_cargo_failure`. |
| R4 | Preserve previously published components when Cargo compilation fails, so a failed build cannot appear to publish a new successful result. | The command fails and leaves published components unchanged. Publication-copy failures are separately [nontransactional](docs/reference.md#build). Covered by `build_produces_http_components_and_preserves_outputs_on_cargo_failure`. |
| R5 | Remove only the documented generated workspace and build outputs during cleanup, preserving application inputs and user configuration. | The [cleanup set and ownership checks](docs/reference.md#clean) apply even with a missing or invalid application manifest. Missing outputs are tolerated, and removed directory symlinks are not followed. Covered by `clean_removes_generated_files_and_preserves_user_files`, `clean_refuses_a_handwritten_cargo_manifest_before_removing_anything`, `clean_does_not_require_a_valid_manifest_or_function_sources`, and `clean_removes_directory_symlinks_without_following_them`. |
| R6 | Start serving only after all inputs and components are prepared and the listener is bound. | Run requires a separate build, reports function/path context for missing components, and never announces readiness on preparation failure. Covered by `run_requires_built_compatible_components_before_readiness` and the real-socket server check in the component build case. CLI startup and signal-driven shutdown require the [manual workflow](docs/guides.md#manually-exercise-failures-and-concurrency). |
| R7 | Preserve user configuration and support repeated setup without unnecessary churn, making the generated workspace usable during development. | Ignore rules and editor settings are merged as [documented](docs/reference.md#ignore-rules-and-editor-settings); repeated setup avoids duplicates and leaves identical generated files untouched. Invalid editor settings fail before writing. Covered by the setup repeatability, ignore-rule, editor-merge, and invalid-settings cases; incremental build reuse is checked in the component build case. |
| R8 | Let authors configure supported shared and per-function dependencies, build with stable locked versions by default, and explicitly update them, while keeping guest dependency resolution separate from the runner. | Invalid declaration conflicts fail generation; normal debug/release builds reject missing or outdated lockfiles without changing them, and `--unlocked` explicitly permits changes. `wr update` creates or refreshes the application lockfile to compatible versions without compiling or changing published components. See [build](docs/reference.md#build) and [update](docs/reference.md#update). Covered by `setup_rejects_invalid_dependencies_without_partial_output`, `update_refreshes_dependencies_without_building`, and the component build case. |
| R9 | Execute registered HTTP components through a reusable synchronous runtime. | Registration validates imports and the versioned contract and retains compiled code. Calls use fresh stores/instances and may run concurrently; invalid IDs, traps, and nontermination return errors without stopping the host. The engine-owned epoch ticker traps at the execution deadline and shuts down with the runtime. Server coverage exercises real components; the deliberately misbehaving example handler supports manual isolation, timeout, trap, and recovery checks. |
| R10 | Serve built applications with bounded HTTP execution. | Rocket owns route syntax, default ranking, collision detection, method matching, slash handling, and HEAD fallback. Query decoding and bounded request bodies follow the [server reference](docs/reference.md#run). The example-based HTTP smoke check in `build_produces_http_components_and_preserves_outputs_on_cargo_failure` covers routing and body handling through real sockets, plus rejection of collisions before readiness. Overload, disconnects, request-reading timeouts, and draining shutdown are manual checks using the [guide](docs/guides.md#manually-exercise-failures-and-concurrency). |

Failure behavior, output paths, and preservation rules in the reference are
part of these commitments even where the table summarizes them. Filesystem
writes, removal, and publication are not promised to be transactional.

### Daemon requirements

| ID | Requirement | Observable acceptance condition and coverage |
| --- | --- | --- |
| R11 | Separate daemon lifecycle from application deployment. | `daemon start` returns after both listeners are bound, or succeeds by reporting an already running instance without changing its address or applications. `status` queries the live process, and `stop` waits for listener closure and execution draining. Failed startup leaves no new live daemon. `daemon_lifecycle_owns_both_listeners_and_restarts_empty` covers process lifecycle, repeated starts, failed binding, socket permissions, singleton ownership, and stale-socket recovery; the component-build workflow checks preservation of deployments on repeated start. Draining active executions remains a manual scenario. |
| R12 | Host independently managed applications without interrupting unrelated routes. | A deployment contains identity, routes, and component bytes. Invalid components or conflicting routes leave all active applications unchanged. Replacement and removal affect future requests; requests already using a generation retain it. The component-build workflow covers two applications, failed and successful replacements, concurrent requests, removal, independence from source paths, and empty restart; replacing an executing generation remains a manual scenario. |

## Deliberate exclusions

This increment does not provide SQL/KV host services or non-HTTP event triggers.
The custom guest interface is not WASI HTTP. Prebuilt module declarations,
automatic builds on run, automatic prerequisite installation, file watching,
streaming, TLS configuration, rate limiting, custom schedulers, persistent
deployments, remote control administration, automatic startup at login/boot,
Windows daemon support, per-application resource quotas, and unsupported
dependency forms are outside the current contract. See the
[reference](docs/reference.md) for exact input and host capability constraints.

## Open decisions

No additional delivery dates, performance targets, or supported-platform
promises are established here. Future work may consider additional bounded
host capabilities, memory quotas, or non-HTTP triggers; those are not approved
features. New requirements should state observable acceptance conditions and
follow the [documentation maintenance rules](CONTRIBUTING.md#maintain-documentation-with-each-change).
