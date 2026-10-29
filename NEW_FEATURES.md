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
