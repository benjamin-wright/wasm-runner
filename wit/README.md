# HTTP guest interface

[world.wit](world.wit) defines the authoritative WebAssembly Interface Types
(WIT) contract. Components export `local:http/handler@0.1.0` from the
`http-function` world. This is a custom interface, not WASI HTTP. The package
version identifies the interface, not an application deployment.

## Write a handler

Each direct Rust source in an application's `functions` directory provides
`pub fn handle(request: Request) -> Response` and imports the generated types
from the crate root. A minimal handler is:

```rust
use crate::{Request, Response};

pub fn handle(_request: Request) -> Response {
    Response {
        status: 200,
        headers: vec![],
        body: b"Hello, world!".to_vec(),
    }
}
```

Setup generates bindings and an entry point that implements the exported guest
trait and delegates to this function. Build uses ordinary Cargo with the
`wasm32-wasip2` target. See the [greeting](../examples/functions/greet.rs) and
[JSON echo](../examples/functions/echo.rs) examples, or
[add a function](../docs/guides.md#add-a-function).

## Request and response

The handler receives a complete request and returns a complete response.
Requests contain a method, path, optional query, headers, and body. Responses
contain a `u16` status, headers, and body. Bodies are byte lists; headers are
string pairs. Use the WIT-generated types directly; no second model or automatic
conversion from Rust errors exists. Represent expected failures with an
appropriate HTTP response, as the echo example does for invalid JSON.

Function names come from source filenames. Routes belong to the application
manifest; see [manifest and discovery rules](../docs/reference.md#source-discovery).

## Current limitations

Execution is not implemented. The WIT world declares no host imports, but the
compilation target may introduce WASI imports that an eventual host must satisfy.
HTTP translation, execution limits, and host capability policy remain
[open decisions](../SPEC.md#open-decisions). Event handlers and SQL/KV host
capabilities are not implemented.
