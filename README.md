# WASM RUNNER

A small, standalone WebAssembly application server: one manifest, one component
per function, and HTTP endpoints. `local-http` is a working name.

This repository is a specification and contract starter. It does not yet contain
a server, build tooling, or compiled example components.

## Intended experience

Build your functions as WebAssembly components implementing the
[`http-function` world](wit/world.wit), then run:

```sh
local-http run application.yaml
```

The server validates the whole application before listening, prints its routes,
and stays in the foreground. Edit, rebuild, and restart to change the application.

```text
hello listening on http://127.0.0.1:8080
GET   /hello  -> greet
POST  /echo   -> echo
Ctrl-C to stop
```

Read the [first-iteration spec](SPEC.md), edit the [example manifest](application.yaml),
and see the [guest contract notes](wit/README.md).
