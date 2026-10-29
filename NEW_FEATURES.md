# Planned features

## R9: Execute HTTP handler components through a reusable runtime

**Status:** Agreed design; not implemented.

Provide a self-contained runtime that executes a selected WebAssembly component
for an HTTP request without depending on the CLI, application manifest, or HTTP
server. This enables reuse by other embedding applications.

### Public boundary

- Construct a runtime with a configurable, finite execution timeout.
- Add component bytes, validate the existing [guest contract](wit/world.wit) and
  supported imports, and return an opaque module ID. Prepare and retain compiled
  code for reuse; report incompatible components at registration.
- Synchronously execute a module ID with the contract's request type, returning
  its response or an execution error. Guest HTTP error responses are successful
  invocations; traps and timeouts are execution errors.
- Create a fresh instance, store, and host state for each invocation. Support
  concurrent invocations without serializing execution behind a shared instance.

### Execution policy and ownership

Use Wasmtime with epoch interruption configured to **trap at the deadline**.
The runtime owns the engine and a dedicated epoch-ticker thread, including its
shutdown. Do not implement periodic guest yielding, fuel accounting, or a custom
scheduler. Deadline enforcement is subject to epoch/checkpoint granularity.

The execution allowance starts when the embedding application's blocking job
begins the invocation, includes instantiation and handler execution, and excludes
registration and queue waiting. Expiration must terminate guest execution, not
merely stop waiting for its response.

The embedding application owns blocking-job dispatch, concurrency limits,
admission/queue policy, and any overall request deadline. An execution slot stays
occupied until execution actually ends, including after client disconnection.
Routing, sockets, HTTP translation, and async-executor ownership remain outside
the runtime.

### Acceptance and scope

- A caller can register and invoke a component without a manifest or server.
- Concurrent invocations share compiled code but not mutable guest state.
- A nonterminating guest fails with a timeout and releases invocation resources;
  subsequent invocations remain usable.
- Invalid module IDs and guest traps return errors without terminating the host.

SQL/KV bindings, streaming, hot reload, and cooperative scheduling are deferred.
Before implementation, define the WASI imports and permissions required by the
existing components; grant no implicit host filesystem or environment access.
Future host I/O must respect the remaining deadline: epoch interruption cannot
forcibly terminate arbitrary host code or undo external side effects.

## R10: Serve built applications over HTTP

**Status:** Agreed design; not implemented. Depends on R9.

Make `wr run <DIRECTORY> [--listen <IP:PORT>]` serve the application's configured
handlers using Actix Web. Build remains a separate step: run reads existing
components from the inferred paths and never invokes Cargo. Preserve the default
listener `127.0.0.1:8080`; other addresses require explicit configuration.

### Boundaries and startup

- A CLI-independent server module accepts its own configuration and handler
  types: listener address, execution timeout, maximum concurrent executions,
  maximum request-body size, and named handlers with method, route pattern, and
  component bytes. It encapsulates the runtime; callers do not handle runtime IDs.
- Loading code reads configured component files and reports failures with
  function/path context. Wasm-to-native compilation stays in runtime registration,
  not the loader. Missing components produce an actionable build-first error.
- The run command loads the manifest, assembles server inputs, reports startup
  and errors, and translates process signals into server shutdown requests.
- Validate all routes and register all components before accepting traffic.
  Any preparation or bind failure aborts startup. Announce readiness only after
  preparation and binding succeed.
- Register routes programmatically. Prepare the shared runtime and global
  execution admission limit outside Actix's per-worker application factory.

### Routing and guest request contract

Extend the [manifest schema](application.schema.json) to accept named captures
such as `/users/{user_id}/orders`. A capture matches exactly one nonempty path
segment; names must be unique within a pattern. Custom regex and catch-all
patterns are out of scope. Preserve literal path spelling and significant
trailing slashes; do not add automatic slash normalization or redirects.

Select the most specific matching path, then its method. A pattern is more
specific when it replaces one or more captures with literals and otherwise
matches the same segments. Reject overlapping patterns where neither is more
specific, and equivalent patterns declaring the same method, regardless of
capture names. Group methods for equivalent path patterns into one resource,
preserving each handler's capture names. Manifest iteration order must not
affect routing.

Revise and version the [WIT request contract](wit/world.wit) to include:

```wit
path: string,
path-params: list<tuple<string, string>>,
query: list<tuple<string, list<string>>>,
```

The server preserves the actual encoded request path, extracts decoded named
captures, and parses the query using standard form-query decoding. The runtime
passes these values to the guest without routing or reparsing them. Existing
components must be rebuilt; incompatible interfaces fail registration.

For `/users/42/orders?status=open&tag=a&tag=b`, the guest receives:

```text
path:        "/users/42/orders"
path-params: [("user_id", "42")]
query:       [("status", ["open"]), ("tag", ["a", "b"])]
```

Each decoded query key appears once, with values in encounter order. Key order
has no semantic significance; preserve first occurrence for predictable output.
An absent or empty query yields an empty list. Both `?flag` and `?flag=` yield
`[("flag", [""])]`. Percent escapes are decoded; `+` means space and `%2B` means
literal plus. Do not add polymorphic values or a raw-query alternative.

### Execution and HTTP behaviour

Dispatch synchronous runtime calls onto blocking threads. Bound admission across
all server workers, with no application-level execution waiting queue. Hold each
execution slot until the job actually ends, even after client disconnection.
Use R9's execution deadline, without cooperative guest scheduling. Separately
bound request-body size and request-reading time using server facilities.

| Condition | Response |
| --- | --- |
| No matching path | `404 Not Found` |
| Matching path, unsupported method | `405 Method Not Allowed`, with `Allow` |
| Execution capacity exhausted | `503 Service Unavailable` |
| Request body exceeds the configured limit | `413 Content Too Large` |
| Guest execution deadline expires | `504 Gateway Timeout` |
| Guest trap or invalid guest response | `500 Internal Server Error` |

Forward valid guest responses subject to HTTP protocol rules. Keep execution
diagnostics in host logs, not error response bodies. HEAD and OPTIONS require
explicit routes; do not add automatic GET fallback, OPTIONS responses, or CORS.

The server accepts caller-driven shutdown rather than installing process signal
handlers. Stop admitting requests, drain already admitted work within execution
deadlines, and release the runtime and its ticker.

### Acceptance and implementation scope

- A separately built application serves its configured handlers through `wr run`.
  The server can also be constructed from bytes/configuration without the CLI or
  manifest loader.
- Literal and parameterised routes, method selection, decoded captures, and
  repeated query values behave as specified above.
- Missing/incompatible components and route conflicts fail before serving;
  failed startup never announces readiness.
- Concurrent, overloaded, oversized, trapping, and nonterminating requests
  produce the specified outcomes without disabling subsequent valid requests.
- Shutdown drains admitted executions and releases server/runtime resources.

Explicit prebuilt-module manifest declarations, automatic builds, hot reload,
TLS configuration, rate limiting, and streaming are deferred. Choose concrete
limit defaults during implementation. Update the authoritative schema, WIT,
[requirements](SPEC.md), [reference](docs/reference.md),
[architecture](ARCHITECTURE.md), and affected examples/guides with the behaviour;
this document records planned work rather than current capability.
