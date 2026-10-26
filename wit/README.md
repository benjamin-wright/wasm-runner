# Guest interface sketch

[`world.wit`](world.wit) defines a proposed guest interface. The current CLI
does not use it, generate bindings, or execute components. `wr setup` generates
Cargo manifests pointing directly at handler source files, but these do not yet
export this interface. Guest builds and runnable components are not implemented.

The `http-function` world exports a handler that takes a request and returns a
response. Requests contain a method, path, optional query, headers, and body.
Responses contain a status, headers, and body. Bodies are byte lists and headers
are string pairs. The world declares no host imports.

The package version identifies the interface, not an application deployment.
Function names, module paths, and routes belong to the application manifest.
This is a custom interface, not WASI HTTP.

Runtime behavior, HTTP translation, resource limits, and guest tooling remain
undecided. See [SPEC.md](../SPEC.md) for current CLI behavior and
[ARCHITECTURE.md](../ARCHITECTURE.md) for the implemented structure.
