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
| R12 | Host independently managed applications without interrupting unrelated routes. | A deployment contains identity, an explicit path prefix, routes, and component bytes. Invalid components, internal route collisions, or overlapping prefixes leave all active applications unchanged. Replacement and removal affect future requests; requests already using a generation retain it. The component-build workflow covers two applications, failed and successful replacements, concurrent requests, removal, independence from source paths, and empty restart; replacing an executing generation remains a manual scenario. |
| R13 | Declare application mount points without deployment flags or retained defaults. | Every manifest requires a canonical `path-prefix`. Run and deploy serve application-relative routes beneath it and strip the prefix before guest execution. Different applications may share function paths but cannot have equal or nested prefixes. Listing/status show current prefixes. Prefix changes publish atomically with the application, and failures preserve old mounts. `manifests_require_literal_canonical_path_prefixes` covers input constraints; the component-build workflow covers prefix routing, capture/query preservation, boundaries, root mounts, conflicts, replacement, and removal. |

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
  An empty daemon is explicitly shown as having no deployed applications.
- Loading or refreshing a page queries live daemon metadata, so subsequent
  requests reflect deployments, replacements, and removals without restarting
  the UI. Inspection does not require application source directories or local
  build outputs.
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
and daemon lifecycle changes, remote access, logs, and
metrics are outside this requirement. The [admin reference](docs/reference.md#admin-ui)
defines command options, page responses, and timeouts. Coverage in
`admin_ui_reports_live_status_and_recovers` checks startup, metadata, errors,
escaping, recovery, and independent shutdown; `admin_ui_cancels_stalled_metadata_requests`
checks the control deadline and connection cancellation. The existing
component-build workflow checks live deployment, replacement, removal, root
mounts, and independence from local build outputs. It also checks function
names, ordering, methods, escaped dynamic paths, and metadata preservation on
failed deployment and refresh after replacement or removal.

## Proposed increment: persistent application KV

Status: planned, not implemented. Use one embedded Fjall database per host
storage root with one isolated keyspace per application. The current schema,
guest WIT, and reference still describe implemented behavior. The
[proposed architecture](ARCHITECTURE.md#proposed-kv-storage) owns implementation
responsibilities and rationale. KV persistence does not persist deployments.

### Author workflow and scope

An application opts in with one optional top-level boolean, `kv`, defaulting to
`false`. The header of the [example manifest](examples/application.yaml) would be:

```yaml
name: hello
path-prefix: /demo
kv: true
# Existing functions follow unchanged.
```

All functions in an enabled application share its keyspace. The host binds
access to application identity; guests cannot select another keyspace or
supply a database path. Path-like keys do not escape that binding. Guests
receive no filesystem access, database-management API, or Fjall dependency.
Handlers use generated host bindings.

The first version provides get, set, and delete. Guest-selected named stores,
per-function permissions, scans, TTL, compare-and-set, multi-key transactions,
SQL, remote backends, replication, online backup, and a data browser are outside
this increment. Size and storage-health inspection is included. Hibernation,
resource quotas, and horizontal scaling remain future work.

### Proposed guest behavior

Encode this interface in the [WIT contract](wit/world.wit) and explain generated
bindings in the [guest notes](wit/README.md).

| Operation | Result and meaning |
| --- | --- |
| `get(key)` | Optional bytes; absence and a present zero-byte value are distinct. |
| `set(key, value)` | Success after Fjall accepts the replacement under the default OS-buffer persistence policy. |
| `delete(key)` | Atomically remove the key and report whether a value existed; absence succeeds with `false`. |

Keys are case-sensitive UTF-8 strings of 1–1,024 encoded bytes, compared without
normalization. Values are opaque bytes of 0–1,048,576 bytes (1 MiB). These are
per-operation validation limits, not application quotas. Typed errors cover
disabled capability, invalid key, oversized value, and storage failure. Guest
errors do not expose host paths or stored values. The guest chooses how ordinary
KV errors map to HTTP responses.

Operations are individually atomic. Reads after a completed mutation see its
result unless a later mutation has superseded it. Concurrent changes follow
the database's operation ordering. Get followed by set is not an atomic update
and is unsuitable for concurrent counters. Deletion's boolean must not be
implemented by an unprotected get followed by remove.

### Persistence policy

Use Fjall's default flushing to OS buffers. Do not request a durable sync for
every guest mutation. This writes to the filesystem through the OS cache; it
is not a promise that calls touch only memory. Fjall owns journaling, buffering,
background flushing, compaction, and recovery. There is no runner-owned live
map, mutation queue, size/time batching policy, or snapshot writer.

OS-buffered data is intended to survive a runner-process crash while the OS
remains healthy. Successful mutations are not guaranteed durable against an OS
crash or power loss. There is no promised maximum window of data loss or extra
runner-managed periodic sync in the first version. A later guest trap, timeout,
HTTP error, or disconnect does not roll back an accepted mutation. Failed or
unacknowledged calls can have unknown outcomes; do not automatically replay
guest operations or promise exactly-once execution.

Graceful shutdown closes admission, drains control work and guest executions,
then explicitly requests a database-wide durable sync before releasing handles.
Report sync failure rather than claiming successful data preservation. Keep
this operation observable instead of depending only on best-effort flushing
when database handles are dropped.

### Isolation and latency

Keyspaces provide application data separation, enforced by binding each guest
to only its own handle. They share database resources, journaling, background
workers, and the underlying filesystem. They do not provide independent I/O,
memory, fairness, corruption, or failure domains. A busy application or a shared
storage fault can affect other KV applications.

Avoiding a durable sync per call is the initial latency policy, not a guarantee
against transient disk stalls. OS-buffer writes, reads missing caches, locking,
and background maintenance can delay calls. Measure operation latency during
alpha testing, including tail latency under storage pressure, before adding a
second memory layer or changing engines. This replaces the earlier requirement
that guest KV operations never wait on database I/O.

Synchronous calls stay in the existing bounded blocking execution path. Check
invocation expiry before and after host calls, but an in-flight native database
call cannot be forcibly stopped by guest epochs. Retain the execution slot until
it finishes; an expired call reports timeout even if its mutation succeeded.
No application-registry publication lock may be held during database I/O.

### Alpha resource policy and observability

Add no runner-enforced per-application size limits in the alpha. Keep Fjall's
normal cache, buffer, and maintenance controls; this policy does not require
unbounded engine memory or disabling its backpressure. Retain per-operation
validation. Assess future per-application quotas using observed workloads.

Expose database and keyspace metadata through daemon status and the read-only
admin UI, periodic foreground diagnostics, and the host storage API. Include
retained keyspaces after application removal or disabling KV. Record:

- Per-application operation counts, failures, and elapsed-time summaries,
  including maxima to reveal intermittent latency spikes.
- Fjall-reported per-keyspace disk usage and estimated entry count, clearly
  labelled as estimates rather than exact live payload or memory usage.
- Database-wide disk usage and journal count, plus additional inexpensive engine
  statistics when the selected release exposes them.
- Storage errors and the status/duration of explicit durable sync operations.

Estimated entry counts can diverge substantially after updates/deletes. Shared
journals and caches cannot be assigned accurately to individual applications;
do not present per-keyspace disk usage as an application's complete footprint.
Do not rebuild a mirror map or scan all entries merely to provide exact metrics.
Do not invent per-application pending-queue or flush-age metrics that the engine
does not expose. Counter scope is the host lifetime; samples reset on restart.
Logs contain metadata, never keys or values. No separate telemetry service is
required. Report fatal shared-engine errors with their database-wide scope.

### Storage identity and lifecycle

Fjall uses a database directory containing managed files. One database is owned
by each selected host root, with the validated manifest name as the keyspace:

| Mode | Database directory | Application keyspace |
| --- | --- | --- |
| Daemon | `<state-dir>/kv/` | Manifest `name` |
| Foreground `wr run` | `<application-directory>/.wr-data/kv/` | Manifest `name` |

Daemon state-directory selection retains its [existing rules](docs/reference.md#daemon-and-deployment).
Foreground and daemon data remain separate. Setup adds the foreground data
directory to ignore rules; clean preserves it outside generated `.wr/` output.
Setup, build, update, and previews do not open/create the database.

Open the shared database lazily on the first enabled application's preparation,
then retain it until host shutdown. Open/create its keyspace before publication;
a disabled application neither initializes the database nor gains a handle.
Validate managed paths/private Unix permissions. Locked, inaccessible, corrupt,
or incompatible storage fails preparation without truncating existing data or
changing published routes. Failed preparation may leave a newly created empty
keyspace or initialized database. Do not silently substitute empty storage.

Retain application handles across generations and removal. Changing routes,
prefixes, or components preserves the keyspace. A daemon deployment's source
location is irrelevant. Renaming selects a different keyspace and retains the
old data. Old requests can finish writes after replacement/removal; application
data formats must tolerate overlapping generations.

Removal, disabling KV, and clean do not delete keyspaces. Redeploying the same
identity reuses its data. Restart recovery reopens the database, while daemon
deployments still start empty until explicitly resubmitted. No second process
may concurrently own the same database directory. There is no automatic data
deletion, reset command, or multi-process sharing. Offline backup covers the
whole database directory after stopping its owner, not an individual keyspace
file.

### Guest compatibility

Add a separately versioned KV import while preserving the existing HTTP export
and compatibility with previously built HTTP-only components. Link the import
and enforce capability on every call. Unsupported imports fail preparation
with an actionable diagnostic. Deployment carries only the capability flag,
never database contents or paths; the daemon validates it independently.
Document daemon/client upgrade requirements for changed wire metadata.

### Acceptance conditions

These are future checks, not claims of current coverage. Exercise public storage
operations and the real-component workflows in [tests/cli.rs](tests/cli.rs):

- Old HTTP-only components work; invalid flags fail YAML/control validation;
  disabled access creates no storage and cannot acquire another app's handle.
- Binary/empty values, absence, overwrite, atomic delete results, per-operation
  limits, and read-after-write behavior work through real guest calls.
- Identical keys in different application keyspaces remain independent; functions
  in one application and overlapping generations share its data.
- Replacement, removal/redeployment, disabling/re-enabling, prefix changes,
  and clean preserve data; authoring commands do not initialize the database.
- Graceful shutdown durably syncs before release. Runner-process restart recovers
  OS-buffered writes under a healthy OS; power-loss durability is tested only
  after an explicit successful durable sync and is not claimed per mutation.
- Invalid/locked storage cannot publish a replacement or truncate data. Shared
  engine errors are reported without pretending other keyspaces are independent.
- Status includes retained keyspaces, labelled engine estimates, operation
  timing/errors, and database sync results without exposing keys or values.

Measure normal and tail latency under representative alpha workloads and slow
storage. Do not turn the previous no-I/O-stall promise into an unsupported test
assertion. Use documented manual scenarios when deterministic public-interface
coverage is impractical. Run `make check` after implementation; documentation
review does not verify KV behavior. Move exact implemented contracts to their
schema/WIT/reference owners when shipping.

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
