# HTTP guest interface

[`world.wit`](world.wit) defines the HTTP guest contract. `wr setup` copies it
into the generated application workspace and generates entry points using
`wit-bindgen`. `wr build` builds components exporting this interface through
ordinary Cargo and the `wasm32-wasip2` target. Execution is not yet implemented.

The `http-function` world exports a handler that takes a request and returns a
response. Requests contain a method, path, optional query, headers, and body.
Responses contain a status, headers, and body. Bodies are byte lists and headers
are string pairs. The world declares no host imports; the compilation target
may introduce WASI imports that an eventual host must satisfy.

Each source file provides `pub fn handle(request: Request) -> Response` and
imports the generated types with `use crate::{Request, Response};`. The generated
entry point implements the exported guest trait and delegates to that function.
Expected failures return an appropriate HTTP response. There is no automatic
conversion from Rust errors or a second set of request/response types.

The package version identifies the interface, not an application deployment.
Function names come from source filenames; routes belong to the application manifest.
This is a custom interface, not WASI HTTP.

Future event handlers should have their own handler interface and world, not
HTTP envelopes. SQL/KV capabilities can be defined as shared imported interfaces
without sharing trigger-specific types. Neither feature is implemented now.

Runtime behavior, HTTP translation, and resource limits remain undecided.
See [SPEC.md](../SPEC.md) for current CLI behavior and
[ARCHITECTURE.md](../ARCHITECTURE.md) for the implemented structure.
