# First iteration

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
and contain no query, fragment, whitespace, percent escapes, or wildcard syntax.
There are no path parameters. A trailing slash is significant.

Reject unknown fields, duplicate YAML mapping keys, missing values, wrong types,
and duplicate method/path pairs. Accept one YAML document with no custom tags,
merge keys, or aliases. There is no interpolation or inherited configuration.
Errors identify the manifest location and field, such as
`functions.greet.module: file does not exist`.

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

Pass the request method, URI, headers, and body through the WASI HTTP interfaces.
Preserve repeated end-to-end headers. The server owns HTTP framing and removes
hop-by-hop headers. A HEAD response never transmits body bytes.

Buffer at most 1 MiB of request body; return 413 if exceeded. Stream responses
with backpressure and a 1 MiB total body limit. Failure before sending response
headers returns a host-generated error; failure after headers closes the stream.
Never retry an invocation automatically.

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

The deadline covers body intake, guest execution, and response streaming. Bound
transport connections and header-read time separately so idle clients cannot
consume unlimited resources. Implementation must cancel looping guests, propagate
client disconnects, and release resources after traps and timeouts. Do not hold a
global execution lock while awaiting a guest or client.

## Guest contract

[`wit/world.wit`](wit/world.wit) is the authoritative guest world. It includes
standard WASI HTTP 0.2.6; there are no custom host imports. See the
[contract notes](wit/README.md) for the initial host policy.

Log one completion event per invocation with function, method, path without query,
status, duration, and outcome. Capture guest stdout/stderr with function and
invocation context, with bounded buffering. Do not log request or response bodies,
query values, or headers by default.

## Out of scope

KV, SQL, messaging, timers, outbound HTTP, filesystem access, secrets and
environment injection, management APIs, Web UI, TLS, hot reload, deployment
history, remote artifacts, plugins, and automatic guest builds.

Add new host capabilities only when a concrete application needs them. Do not
reserve configuration fields or introduce generic providers for future features.

## Acceptance

- Build a greeting component and an echo component against the guest world.
- Run the example from a different working directory and reach both endpoints.
- Verify exact routing, 404, 405, and repeated response headers.
- Demonstrate useful errors for a bad field, missing module, incompatible component,
  duplicate route, and occupied port; none leaves a partially serving application.
- Demonstrate fresh guest state for successive requests.
- Demonstrate overload rejection, request/response bounds, timeout of a looping
  guest, and cancellation when a client disconnects.
- Stop with Ctrl-C, then start again on the same port.

These are implementation acceptance criteria; this starter does not claim that a
runtime or executable fixtures exist yet.
