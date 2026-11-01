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
The WIT world imports `local:kv/store@0.1.0` and `local:sql/database@0.2.0`; Rust's compilation target introduces
WASI imports. The [runtime policy](../docs/reference.md#runtime-and-wasi-policy)
defines supported interfaces and permissions. Guest state is fresh for every
invocation. Previously built HTTP-only components remain compatible. Event
handlers are not implemented. Previously built KV components also remain compatible.

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

## SQL queries and mutations

Enable SQL with `sql: {enabled: true}` in the manifest and put schema changes in
the application's `migrations` directory. The runner applies pending migrations
after preparing the application and before publishing it. See the
[SQL reference](../docs/reference.md#persistent-application-sql) for naming,
history integrity, targets, compatibility, and rollback requirements.

The generated [SQL interface](deps/sql/database.wit) is exported as `crate::sql`:

```rust
use crate::sql::{self, Statement, Value};

fn greeting() -> Result<Option<String>, sql::SqlError> {
    let cursor = sql::query(&Statement {
        sql: "SELECT message FROM greetings WHERE id = ?1".into(),
        parameters: vec![Value::Integer(1)],
    })?;
    Ok(cursor.next()?.and_then(|row| match row.into_iter().next() {
        Some(Value::Text(message)) => Some(message),
        _ => None,
    }))
}
```

`query` returns a cursor resource. Use `cursor.columns()` for metadata and
`while let Some(row) = cursor.next()?` to process one row at a time. There is no
total row-count limit. A cursor retains one pooled read connection and a snapshot;
writes performed while iterating do not alter that snapshot. Drop it for early
release. Exhaustion/errors release the reader, and invocation teardown also
cleans up cursors that the guest failed to drop. SQL 0.1.0 components must be
rebuilt for this cursor API; HTTP-only and KV components remain compatible.

`execute` accepts one mutation without returned columns and returns the affected
row count. `batch` executes a list of such mutations in one transaction and
returns each statement's count. Bind parameters instead of interpolating values.
SQL null, integers, real numbers, text, and bytes have distinct variants; SQLite
column affinity still controls how stored values are interpreted.

Schema changes belong in migrations. Guests cannot access migration history,
open other databases, change engine settings, or manage transaction boundaries.
Every operation checks its application's capability and returns `Disabled` when
SQL is off. Writers wait for the application's write connection; queries lease
readers from a pool. First-query waits use the invocation deadline. At most four
cursor resources may be held per invocation; nested queries return `Limit` if
no reader is immediately available. An expired waiter executes no SQL. `Busy`
is reserved for SQLite-level lock conflicts outside host coordination; no
operation is automatically retried. A committed mutation survives a
later guest trap or timeout. The [SQL greeting](../examples/functions/sql-greet.rs)
demonstrates error handling without any migration code in the handler.
