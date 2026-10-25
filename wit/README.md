# Guest contract

[`world.wit`](world.wit) is the entire contract. It has no dependencies or host
imports. Generate bindings for its `http-function` world in your guest language
and implement the exported `handler.handle` function.

Conceptually, a handler is just:

```text
handle(request):
    return {
        status: 200,
        headers: [("content-type", "text/plain; charset=utf-8")],
        body: utf8("Hello!")
    }
```

This is pseudocode; the binding generator supplies the language-specific types
and export wiring. The output must be a WebAssembly component, not a raw core
Wasm module. Packaging and a runnable language example remain implementation
work; the small contract does not eliminate that toolchain step.

## Values

- `method`: the uppercase HTTP method, such as `GET`.
- `path`: the original request path, without the query, such as `/hello`.
- `query`: the raw query without `?`, or `none` when there is no query. Values are
  not URL-decoded. `/hello?` has an empty query, distinct from no query.
- `headers`: name/value pairs, preserving repeated fields such as `Set-Cookie`.
  Incoming names are lowercase; values are UTF-8 text. Non-UTF-8 request header
  values receive 400 before invocation. Header names and response values must
  satisfy HTTP syntax; newline injection is invalid.
- `body`: bytes, so binary requests and responses need no text conversion.
- `status`: a final HTTP status from 200 through 599. Return an error response
  explicitly for application failures. A trap becomes a host-generated 500.

Both bodies are complete, buffered values, limited to 1 MiB each. There are no
streams, resource handles, callbacks, or explicit resource cleanup in guest code.
The host owns HTTP framing and transport. See [the spec](../SPEC.md) for limits,
header handling, routing, and cancellation.

## Host access

Each request runs in a fresh instance. The guest receives only the request and
returns only the response. There is no filesystem, network, clock, randomness,
environment, standard I/O, or logging import. Components requiring such imports
fail preparation. A guest language's standard library may introduce these imports;
the eventual guest example must demonstrate building without them.

The package version identifies this WIT contract, not an application deployment.
Function names and routing belong to the manifest, not component export names.

This is a project-specific HTTP contract. Existing WASI HTTP components need to
be adapted or rebuilt against it. The tradeoff buys a small authoring surface;
streaming and standard WASI HTTP compatibility are outside the first iteration.
