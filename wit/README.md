# HTTP guest interface

[world.wit](world.wit) defines the authoritative WebAssembly Interface Types
(WIT) contract. Components export `local:http/handler@0.2.0` from the
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
Requests contain a method, encoded path, decoded named path captures, grouped
form-decoded query values, headers, and body. Each query key appears once with
its values in encounter order. See [HTTP translation](../docs/reference.md#run). Responses
contain a `u16` status, headers, and body. Bodies are byte lists; headers are
string pairs. Use the WIT-generated types directly; no second model or automatic
conversion from Rust errors exists. Represent expected failures with an
appropriate HTTP response, as the echo example does for invalid JSON.

Function names come from source filenames. Routes belong to the application
manifest; see [manifest and discovery rules](../docs/reference.md#source-discovery).

## Execution and compatibility

Build components separately, then serve them with `wr run`. Version 0.2.0 adds
`path-params` and replaces the optional raw query with grouped decoded values;
rebuild all older components. Incompatible interfaces fail server startup.
The WIT world declares no host imports, but Rust's compilation target introduces
WASI imports. The [runtime policy](../docs/reference.md#runtime-and-wasi-policy)
defines supported interfaces and permissions. Guest state is fresh for every
invocation. Event handlers and SQL/KV host capabilities are not implemented.
