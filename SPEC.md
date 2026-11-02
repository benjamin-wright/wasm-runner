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
multiple applications at declarative, nonoverlapping path prefixes on one HTTP listener, with a private control socket for
atomic deployment, listing, removal, status, and shutdown. Deployments are
in-memory and must be resubmitted after restarting the daemon. A foreground
`wr admin` client serves a local, read-only browser view of live daemon metadata.
Applications can opt into persistent typed KV values and atomic counters, or
application-scoped SQL databases with deployment migrations;
storage identity and inspection survive application removal and replacement.
Source discovery is independent of route configuration, within the
[manifest and discovery constraints](docs/reference.md#source-discovery).
Handlers can reuse application-owned Rust modules through an optional
[shared entry point](docs/reference.md#shared-application-code).
The example demonstrates [server-rendered HTML with Askama](docs/guides.md#render-html-with-askama)
using ordinary application dependencies and the existing HTTP contract.

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
| R6 | Start serving only after all inputs and components are prepared and the listener is bound. | Run requires a separate build, reports function/path context for missing components, and never announces readiness on preparation failure. Covered by `run_requires_built_compatible_components_before_readiness` and `http_serving_routes_requests_and_rejects_collisions`. `foreground_serving_honors_application_prefix` checks CLI startup and prefix handling, and `foreground_kv_survives_restart_and_clean` checks signal-driven shutdown and persistence. Draining active executions remains a [manual workflow](docs/guides.md#manually-exercise-failures-and-concurrency). |
| R7 | Preserve user configuration and support repeated setup without unnecessary churn, making the generated workspace usable during development. | Ignore rules and editor settings are merged as [documented](docs/reference.md#ignore-rules-and-editor-settings); repeated setup avoids duplicates and leaves identical generated files untouched. Invalid editor settings fail before writing. Covered by the setup repeatability, ignore-rule, editor-merge, and invalid-settings cases; incremental build reuse is checked in the component build case. |
| R8 | Let authors configure supported shared and per-function dependencies, build with stable locked versions by default, and explicitly update them, while keeping guest dependency resolution separate from the runner. | Invalid declaration conflicts fail generation; normal debug/release builds reject missing or outdated lockfiles without changing them, and `--unlocked` explicitly permits changes. `wr update` creates or refreshes the application lockfile to compatible versions without compiling or changing published components. See [build](docs/reference.md#build) and [update](docs/reference.md#update). Covered by `setup_rejects_invalid_dependencies_without_partial_output`, `update_refreshes_dependencies_without_building`, and the component build case. |
| R9 | Execute registered HTTP components through a reusable synchronous runtime. | Registration validates imports and the versioned contract and retains compiled code. Calls use fresh stores/instances and may run concurrently; invalid IDs, traps, and nontermination return errors without stopping the host. The engine-owned epoch ticker traps at the execution deadline and shuts down with the runtime. Server coverage exercises real components; the deliberately misbehaving example handler supports manual isolation, timeout, trap, and recovery checks. |
| R10 | Serve built applications with bounded HTTP execution. | Rocket owns route syntax, default ranking, collision detection, method matching, slash handling, and HEAD fallback. Query decoding and bounded request bodies follow the [server reference](docs/reference.md#run). `http_serving_routes_requests_and_rejects_collisions` covers routing and body handling through real sockets, plus rejection of collisions before readiness. Overload, disconnects, request-reading timeouts, and draining shutdown are manual checks using the [guide](docs/guides.md#manually-exercise-failures-and-concurrency). |

Shared-code acceptance: an optional `libs/mod.rs` is available as `crate::libs`
in every handler, with access to the existing guest types. Applications without
it retain their existing behavior. Generation follows entry-point addition and
removal, preserves source files, and supports editor discovery. Covered by
`shared_libraries_run_in_multiple_components`,
`setup_refreshes_optional_shared_library_entry_point`, the preview/repeatability
and cleanup cases, and the component build case, including shared-source
compilation failure without replacing published components.

Template-example acceptance: setup supplies Askama configuration within each
generated handler crate, so views need only the template filename and no
application-root configuration. The SQL-backed page renders an inherited HTML
layout, escapes database text, handles empty results and unavailable SQL, and
executes without source templates on disk. Covered by
`askama_renders_sql_data_without_template_files_at_runtime`. The component
build workflow also rejects invalid inherited templates without replacing
published components, and cleanup preserves the template inputs. This does not
add a host rendering API, static file hosting, or template hot reload.

Failure behavior, output paths, and preservation rules in the reference are
part of these commitments even where the table summarizes them. Filesystem
writes, removal, and publication are not promised to be transactional.

### Daemon requirements

| ID | Requirement | Observable acceptance condition and coverage |
| --- | --- | --- |
| R11 | Separate daemon lifecycle from application deployment. | `daemon start` returns after both listeners are bound, or succeeds by reporting an already running instance without changing its address or applications. `status` queries the live process, and `stop` waits for listener closure and execution draining. Failed startup leaves no new live daemon. `daemon_lifecycle_owns_both_listeners_and_restarts_empty` covers process lifecycle, repeated starts, failed binding, socket permissions, singleton ownership, and stale-socket recovery; the deployment workflow checks preservation of deployments on repeated start. Draining active executions remains a manual scenario. |
| R12 | Host independently managed applications without interrupting unrelated routes. | A deployment contains identity, an explicit path prefix, routes, and component bytes. Invalid components, internal route collisions, or overlapping prefixes leave all active applications unchanged. Replacement and removal affect future requests; requests already using a generation retain it. The deployment workflow covers two applications, failed and successful replacements, concurrent requests, removal, independence from source paths, and empty restart; replacing an executing generation remains a manual scenario. |
| R13 | Declare application mount points without deployment flags or retained defaults. | Every manifest requires a canonical `path-prefix`. Run and deploy serve application-relative routes beneath it and strip the prefix before guest execution. Different applications may share function paths but cannot have equal or nested prefixes. Listing/status show current prefixes. Prefix changes publish atomically with the application, and failures preserve old mounts. `manifests_require_literal_canonical_path_prefixes` covers input constraints; the deployment workflow covers prefix routing, capture/query preservation, boundaries, root mounts, conflicts, replacement, and removal. |

The deployment workflow is independently covered by
`deployment_replacement_preserves_routes_and_admin_metrics`. It shares compiled
component bytes with other hosting tests, but uses its own application files,
daemon, and admin listener.

### R14: Local administration UI through the client

Let authors inspect the running daemon and its deployed applications in a
browser through a lightweight, read-only UI started by the `wr` client. The
client queries the selected daemon over its existing private control socket
and renders simple HTML pages on the server. The daemon remains the source of
live metadata; the client owns presentation and the UI listener.

Observable acceptance conditions:

- `wr admin` starts the UI in the foreground on a loopback-only HTTP
  listener and reports its URL after binding succeeds. It uses the existing
  daemon selection rules described in the
  [reference](docs/reference.md#daemon-and-deployment).
- Pages show the daemon's PID and hosting address, and deployed application
  names and path prefixes in a stable order. Each application lists its deployed
  functions by name, with HTTP methods and application-relative route paths.
  Each function headlines rolling minimum/maximum latency and links to its
  own metrics page with completed call count, min/max/mean latency, and HTTP
  response code counts. Empty windows show no latency measurement. Windows
  expire during idle inspection and use bounded memory independent of traffic.
  Metrics belong to active deployment generations: successful replacement
  resets them, failed replacement preserves them, and removal/restart discards
  them. Old-generation completions cannot affect new-generation metrics.
  Database-wide KV metrics appear under Daemon. KV-enabled applications show
  their keyspace file size in a column; other per-keyspace metrics are omitted.
  Retained-only owners appear once, marked Disabled, with their file size.
  An empty daemon is explicitly shown as having no deployed applications.
- Loading or automatically refreshing a page queries live daemon metadata, so subsequent
  requests reflect deployments, replacements, and removals without restarting
  the UI. The browser refreshes every five seconds after a page load, including
  error pages, without a manual Refresh button. Inspection does not require application source directories or local
  build outputs.
  Refresh fetches fresh metadata even after following the Applications fragment
  link; fragment navigation must not substitute for a new page request.
- An unavailable daemon or failed metadata request produces a clear error
  within a bounded time, rather than displaying stale data as current. A later
  page request can recover when the daemon becomes available.
- Navigation and refresh work without browser JavaScript. Templates and styles
  are bundled with `wr`, with no separate asset installation or runtime CDN
  dependency. Metadata is escaped when rendered into HTML.
- The UI listener is separate from the daemon's application hosting listener,
  so deployed routes, including a root mount, cannot conflict with UI pages.
  Stopping the UI closes its listener without stopping or changing the daemon
  or its deployments.

The initial UI provides inspection only. Browser-initiated deployment, removal,
and daemon lifecycle changes, remote access, logs, and persistent metrics history
are outside this requirement. The [admin reference](docs/reference.md#admin-ui)
defines command options, page responses, and timeouts. Coverage in
`admin_ui_reports_live_status_and_recovers` checks startup, metadata, errors,
escaping, recovery, and independent shutdown; `admin_ui_cancels_stalled_metadata_requests`
checks the control deadline and connection cancellation. The existing
deployment workflow checks live deployment, replacement, removal, root
mounts, and independence from local build outputs. It also checks function
names, ordering, methods, escaped dynamic paths, and metadata preservation on
failed deployment and refresh after replacement or removal.
Handler metrics coverage extends that workflow with empty windows, GET/HEAD
counting, guest response codes, per-handler links, and generation resets.
`daemon_kv_preserves_data_and_reports_live_metrics` checks trap and timeout
response counts. The metrics window test checks aggregation, boundary expiry,
and idle recovery without sleeping.

## Persistent application KV

Applications opt into persistent typed values with `kv: true`. One embedded
Fjall database per host root isolates applications by their validated names.
Exact behavior belongs to the [storage reference](docs/reference.md#persistent-application-kv),
[schema](application.schema.json), and [guest interface](wit/deps/kv/store.wit).
The [storage architecture](ARCHITECTURE.md#kv-storage) explains responsibilities.
KV persistence does not persist deployments.

### Acceptance conditions

- Bytes, UTF-8 text, and signed integers retain their types; absence and empty
  values remain distinct. Keys and payloads obey the documented limits.
- Increment atomically adds one, initializes absence, and returns its own
  committed result. Type mismatch and overflow preserve the prior value.
  Delete atomically reports whether a value existed.
- Each application sees only its own keyspace. Functions and overlapping
  generations of one application share values. Replacement, removal,
  disabling/re-enabling, prefix changes, and cleanup preserve stored data.
- Storage opens lazily before publication; invalid or locked storage cannot
  replace published routes or silently substitute empty data. Authoring
  commands and disabled applications do not create databases.
- Default OS-buffered mutations survive process restart under a healthy OS.
  A later trap, timeout, or disconnect does not undo accepted writes.
  Graceful shutdown drains preparation and executions and explicitly syncs.
  No per-mutation power-loss durability or bounded I/O latency is promised.
- Previously built HTTP-only components work. YAML and control messages validate
  the capability flag; every guest call checks its bound capability.
- Inspection retains inactive keyspaces and reports operation counts/errors,
  timings, estimated entry counts, keyspace and database sizes, and sync outcomes. Unavailable
  engine measurements are explicitly unavailable, never fabricated as zero.

Coverage uses [public storage tests](tests/storage.rs) and real components and
process workflows in [CLI tests](tests/cli.rs). Operational slow-storage and
power-loss validation remain [manual procedures](docs/guides.md#observe-kv-storage).
`runtime_kv_isolates_calls_and_preserves_accepted_writes`,
`daemon_kv_preserves_data_and_reports_live_metrics`, and
`foreground_kv_survives_restart_and_clean` independently cover runtime,
daemon, and foreground behavior. `runtime_accepts_pre_sql_components` compiles frozen HTTP-only and HTTP/KV
interfaces and checks backward compatibility.
These tests do not establish production latency targets.

## Persistent application SQL

Applications enable embedded SQL with the nested `sql` configuration. Schema
changes are SQL files shipped with the deployment; the runner owns migration
execution and immutable history. The [SQL reference](docs/reference.md#persistent-application-sql),
[schema](application.schema.json), and [guest interface](wit/deps/sql/database.wit)
define the exact contracts.

Acceptance conditions:

- Each validated application identity has its own database, shared by functions
  and overlapping generations. Replacement, removal, disabling, and cleanup
  preserve stored data. Disabled applications do not open SQL storage.
- Prepared statements transport typed null/integer/real/text/byte parameters and
  return cursor resources for incremental row consumption. A scan can exceed the
  old accumulated row/byte limits while each returned row stays bounded.
  Mutation batches are atomic. Calls check capability
  availability and cannot access another database, filesystem paths, engine
  settings, transaction controls, or runner migration history.
- The host validates candidate components, routes, and prefix ownership before
  running migration SQL. Rejected candidates do not change application schemas.
- Migrations use increasing numeric IDs and optional paired rollback files.
  Recorded names and exact-content hashes remain immutable after rollback.
  Missing or edited recorded history fails before schema changes.
- Explicit targets support forward and reverse transitions; omitted targets
  never implicitly roll back. An entire transition and its history commit or
  roll back together. Missing rollback scripts fail before any transition.
- Publication follows migration commit. Authors own compatibility with all
  generations that may execute, including after a commit/publication failure.
  Already committed SQL survives later guest failures and is never replayed
  automatically. Query work and result sizes are bounded; native I/O stalls
  are not promised a hard upper bound.
- A bounded, lazy pool allows independent read snapshots to run concurrently
  with each other and the queued writer. A handler can mutate data while reading
  its original snapshot. Cursor exhaustion, errors, explicit drop, and invocation
  teardown release readers. Cursors retain database ownership until dropped.
- Acquisition waits use existing execution deadlines; a call that expires
  while waiting executes no SQL. Cursor/resource limits prevent unbounded leases,
  and nested queries do not wait while holding cursors against an exhausted pool.
  Waiting retains the host's bounded execution permits. SQL 0.1.0 callers must
  rebuild for the cursor API; existing HTTP-only and KV components remain valid.
- SQL state and the current migration ID are visible through daemon status and
  admin inspection, independently of generation metadata. Retained databases
  are discovered on the first SQL-enabled opening after restart.

Public persistence coverage lives in [SQL tests](tests/sql.rs). Real components
and process workflows in [CLI tests](tests/cli.rs) cover
`runtime_sql_checks_capabilities_and_preserves_committed_writes`,
`sql_deployment_validates_before_migrating_and_preserves_history`, and
`foreground_sql_migrates_and_preserves_data_across_restart_and_clean`.
Existing compatibility and deployment tests continue to protect HTTP/KV clients.
Disk-stall, power-loss, and application-specific schema compatibility remain
operational validation responsibilities, not automated guarantees.

## Deliberate exclusions

This increment does not provide non-HTTP event triggers.
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
