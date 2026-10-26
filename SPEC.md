# First iteration

This is the intended behavior, not an implemented server. The
[WIT world](wit/world.wit) is authoritative for the guest interface. The
[architecture](ARCHITECTURE.md) records the implementation decisions.

## Goal

Run an application described by one YAML manifest. Each named function points to
a local WebAssembly component and serves one HTTP endpoint. Optimize for a clear
edit, build, run, request, and debug loop on a developer's machine.

One foreground process serves one application. There is no daemon or persistent
server state. Reload means stopping the process, rebuilding, and running again.

## Command line

```sh
wr run application.yaml
wr run application.yaml --listen 127.0.0.1:9000
```

The manifest path is required. The default listener is `127.0.0.1:8080`.
`--listen` takes a numeric IP address and port; a non-loopback bind requires an
explicit value. No environment variables implicitly alter configuration.

On success, print the application name, bound address, and method/path/function
table only after preparation and socket binding succeed. Stay in the foreground.
Configuration, preparation, and bind failures exit nonzero with a useful error.
Ctrl-C or SIGTERM stops admission, allows active work up to 30 seconds to finish,
then cancels remaining work and exits. A second signal stops immediately.

## Manifest

The [example](application.yaml) defines the complete initial format:

| Field | Meaning |
| --- | --- |
| `name` | Required application identifier, used in terminal output |
| `functions` | Required, nonempty mapping from function identifier to definition |
| `functions.<id>.module` | Required path to a WebAssembly component file |
| `functions.<id>.http.method` | Required uppercase HTTP method |
| `functions.<id>.http.path` | Required literal route path |

Application and function identifiers match `[a-z][a-z0-9-]{0,63}`. Module paths
resolve relative to the manifest's directory, regardless of the working directory.
Absolute paths are allowed. Multiple functions may reference the same component.

Methods are GET, HEAD, POST, PUT, PATCH, DELETE, and OPTIONS. Paths start with `/`
and contain no query, fragment, whitespace, control characters, percent escapes,
or wildcard syntax.
There are no path parameters. A trailing slash is significant.

Accept one YAML document. Delegate YAML syntax and tag handling to the parser,
including anchors, aliases, and merge-key resolution. The application adds no
tag handlers, interpolation, or loading of external configuration.

Validate the resolved data against [application.schema.json](application.schema.json),
the JSON Schema draft-07 contract for required fields, types, identifiers, paths,
and allowed properties. Module paths must be nonempty and contain no NUL characters.
Reject duplicate YAML mapping keys during parsing and duplicate method/path pairs
in application validation. Schema validation uses the embedded contract; it does
not require a schema file beside the manifest or fetch remote schemas.

Errors identify the manifest file and relevant field. Schema errors use JSON
Pointer paths, such as `/functions/greet/module`. YAML syntax errors retain the
parser's source locations; schema errors need not include YAML line numbers.

There is no application version, namespace, placement, capability block, storage
configuration, or build command. Build tools are the user's choice.

## Preparation

Before accepting requests:

1. Parse and validate the entire manifest.
2. Read and compile every referenced component.
3. Check that each component's imports and exports fit the guest contract.
4. Construct an immutable application and route table.
5. Bind the listener and print readiness.

A failure in any function prevents startup. Errors name the function and relevant
module path. Do not start a partially working application or watch files.

Components, rather than core Wasm modules needing adaptation, are the input
format. Compilation can be shared within a process for identical code. Each
request receives a fresh instance and guest state; guest globals do not persist.

## HTTP behavior

Match the method and raw URL path exactly, before reading the request body. Match
only the path portion; preserve the query string for the guest. Do not percent
decode, normalize slashes, or redirect. HEAD and OPTIONS require explicit routes;
there are no implicit handlers. Unknown paths return 404. A known path with an
unregistered method returns 405 with the registered methods in `Allow`.

Serve HTTP/1.1 over TCP in the first iteration. Accept origin-form request targets
(`/path?query`); reject other target forms with 400. HTTP/2, upgrades, and request
trailers are outside this iteration; a request with trailers receives 400.

Pass method, raw path, optional raw query without `?`, headers, and body through
the custom WIT request record. Preserve the distinction between no query and an
empty query. Header names are lowercase; values must be UTF-8 text. A request
with non-UTF-8 header values receives 400 without invoking a guest.

Preserve repeated end-to-end headers, including separate `Set-Cookie` fields.
Remove hop-by-hop headers and every field named by `Connection`, in both
directions. The host owns framing: consume request framing headers rather than
forwarding them, and ignore guest `Content-Length` and `Transfer-Encoding` in
favor of framing the actual returned body. A HEAD response never transmits body
bytes; its content length describes the returned representation. Statuses 204
and 304 require an empty guest body, otherwise the response is invalid.

Buffer at most 1 MiB of request body, counting actual bytes regardless of declared
length; return 413 if exceeded. Obtain the complete guest response and validate
its status (200–599), header syntax and limits, and body size (at most 1 MiB)
before committing headers. An oversized or malformed response receives 500.
Socket writes use backpressure, but there is no guest response streaming.
Failure before sending response headers returns a host-generated error; failure
after headers closes the connection. Never retry an invocation automatically.

Route misses, overload, and other rejections may leave the request body unread.
In that case close the connection after the response rather than draining an
unbounded body or reusing a connection with unread bytes.

| Condition before response headers | Response |
| --- | --- |
| All execution slots occupied | 503 |
| Invocation exceeds deadline | 504 |
| Guest trap, missing response, or invalid response | 500 |

Host-generated errors have short plain-text bodies without stack traces or guest
data. A normally returned guest response, including a guest-generated 4xx or 5xx,
is passed through.

## Execution bounds

Initial fixed defaults are four concurrent invocations, 64 MiB of guest linear
memory per invocation, and a 30-second invocation deadline. These are execution
limits, not a guarantee about total process memory. Reject excess work immediately;
there is no application request queue.

The memory budget applies across all linear memories in the invocation, including
initial allocations. It is not a separate 64 MiB allowance for each memory.

Acquire an execution slot without waiting after route matching, before body
intake. Start the deadline then and retain the slot until the response finishes
or the connection closes. The deadline covers body intake, instantiation, guest
execution, response validation, and socket writes. If it expires after headers
have been committed, close the connection; a new 504 cannot be sent.

Use a separate limit of 128 accepted connections, a 10-second header-read/idle
timeout, and at most 100 header fields and 32 KiB of header data in either
direction. Incoming header violations are rejected before invocation; guest
header violations receive 500. Refuse excess connections without creating more
connection tasks. HTTP parser failures may close the connection or send its own
4xx response. Bound guest tables and internal instance counts as described in
the architecture as well as linear memory.

Implementation must cancel looping guests, propagate observed client disconnects,
and release resources after traps and timeouts. A silent network failure is
bounded by the deadline, not instantaneously detectable. Do not hold a global
execution lock while awaiting a guest or client.

## Guest contract

[`wit/world.wit`](wit/world.wit) is the authoritative guest world:
`local:http@0.1.0`, exporting `handler.handle(request) -> response`. It has no
imports or dependencies. Both bodies are buffered byte lists; header values are
strings. This is not the WASI HTTP contract. Components requiring WASI or any
other host imports fail preparation. See the [contract notes](wit/README.md).

Log one completion event per invocation with function, method, path without query,
status if committed, duration, and outcome. Include a host-generated invocation
identifier. There is no guest stdout/stderr or logging import to capture. Do not
log request or response bodies, query values, or headers by default. Detailed
startup errors may name local files; HTTP errors must not expose runtime internals.

## Out of scope

KV, SQL, messaging, timers, outbound HTTP, filesystem access, secrets and
environment injection, management APIs, Web UI, TLS, hot reload, deployment
history, remote artifacts, plugins, automatic guest builds, WASI HTTP compatibility,
guest streaming, and guest standard I/O.

Add new host capabilities only when a concrete application needs them. Do not
reserve configuration fields or introduce generic providers for future features.

## Acceptance

- Build a greeting component and an echo component against the guest world.
- Run the example from a different working directory and reach both endpoints.
- Verify exact routing, 404, 405, and repeated response headers.
- Verify absent versus empty queries, binary bodies, non-UTF-8 request header
  rejection, hop-by-hop header removal, and HEAD/204/304 response handling.
- Demonstrate useful errors for a bad field, missing module, incompatible component,
  duplicate route, and occupied port; none leaves a partially serving application.
- Demonstrate fresh guest state for successive requests.
- Demonstrate overload rejection, request/response bounds, timeout of a looping
  guest, and cancellation when a client disconnects.
- Verify that aggregate memory growth across multiple memories is bounded and
  that slow body uploads and response consumers cannot bypass admission limits.
- Stop with Ctrl-C, then start again on the same port.

These are implementation acceptance criteria; this starter does not claim that a
runtime or executable fixtures exist yet.
