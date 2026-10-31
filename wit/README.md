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
Requests contain a method, encoded application-relative path (the mount prefix
is removed), decoded named path captures, grouped
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
The WIT world imports `local:kv/store@0.1.0`; Rust's compilation target introduces
WASI imports. The [runtime policy](../docs/reference.md#runtime-and-wasi-policy)
defines supported interfaces and permissions. Guest state is fresh for every
invocation. Previously built HTTP-only components remain compatible. Event
handlers and SQL are not implemented.

## Persistent application values

Set `kv: true` in the application manifest. Setup exports the generated
[KV interface](deps/kv/store.wit) as `crate::kv`:

```rust
use crate::kv::{self, Value};

// Use these inside a handler, returning an appropriate HTTP response on error.
fn example() -> Result<i64, kv::KvError> {
    kv::set("greeting", &Value::Text("Hello".into()))?;
    let _value: Option<Value> = kv::get("greeting")?;
    kv::increment("visits")
}
```

`Value` preserves bytes, UTF-8 text, or signed 64-bit integers. `delete` returns
whether a value existed. Increment atomically adds one (a missing key becomes
integer `1`); it rejects text/bytes and overflow without changing the value.
All functions in the application share storage. A disabled application receives
`KvError::Disabled` on every operation, even if it imports the interface.

Use the generated enums directly. Guests cannot select another application's
keyspace or a host path. Ordinary typed errors are for the handler to map to
HTTP; see the [counter example](../examples/functions/counter.rs). A later
trap or timeout does not undo writes, so retrying an increment can count twice.
See [storage behavior](../docs/reference.md#persistent-application-kv) for limits,
durability, ownership, and inspection.
