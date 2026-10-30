# Application features and constraints

Read the relevant section when designing a handler or troubleshooting an
application using the installed `wr` command.

## HTTP routes and request data

Each configured function has one `http` mapping with a `method` and `path`.
Methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, and `OPTIONS`.
Unknown manifest fields are rejected. YAML anchors, aliases, and merge keys are
supported; duplicate keys and multiple YAML documents are rejected.

Use `/users/<user_id>` for a single-segment capture and `/files/<path..>` for
a trailing capture. `{user_id}` is not capture syntax. Avoid ambiguous routes
with the same method: overlaps can prevent startup even after a successful
build. Reordering manifest entries does not resolve route conflicts.

Handler request fields are:

| Field | Rust type and meaning |
| --- | --- |
| `method` | `String`, HTTP method |
| `path` | `String`, encoded application-relative request path after prefix removal |
| `path_params` | `Vec<(String, String)>`, decoded named captures |
| `query` | `Vec<(String, Vec<String>)>`, form-decoded keys with grouped values |
| `headers` | `Vec<(String, String)>`, including repeated header values |
| `body` | `Vec<u8>`, complete buffered body |

For example, `?tag=a&tag=b` gives one `tag` entry with two values. Query `+`
decodes to a space; `%2B` decodes to a literal plus. Trailing captures join decoded
segments with `/`. Do not parse `path` again to recover already-decoded captures.

Responses have `status: u16`, `headers: Vec<(String, String)>`, and `body: Vec<u8>`.
Return status 200–599 with valid HTTP headers. Encode JSON explicitly and set
`content-type`; there is no automatic serialization. Let `wr` set response
length and connection headers. HEAD falls back to GET if no explicit HEAD route
matches; on fallback the handler sees GET and the response body is stripped.
OPTIONS needs an explicit route; automatic CORS handling is not provided.

## Dependencies

Put shared dependencies at the manifest root and function-specific dependencies
inside that function. For example:

```yaml
name: my-app
path-prefix: /my-app
dependencies:
  serde: {version: "1.0", features: ["derive"]}
functions:
  echo:
    dependencies:
      serde_json: "1.0"
    http:
      method: POST
      path: /echo
```

This example needs `functions/echo.rs`. Shared dependencies apply to every
discovered function, including unlisted sources. Do not declare the same crate
both globally and locally for one function. Supported values are version strings
or objects with `version`, optional `features`, and optional `default-features`.
Path, Git, and optional dependency declarations are unsupported. `wit-bindgen`
is reserved and managed by the runner. After changing dependencies, run
`wr update <DIRECTORY>`, review the lockfile changes, then build and redeploy (or restart foreground serving).

Choose dependencies that work within the application capabilities below.
A dependency can compile successfully but still prevent the application from
starting if it requires unsupported capabilities.

## Application capabilities and limits

Each request starts with fresh handler state. Do not use globals as persistent
application storage. Handlers cannot access host environment variables, command
arguments, files, or the network. Printing from a handler does not produce
application logs.

Randomness, wall-clock time, and sleeping on timers are unsupported. SQL/KV
services and non-HTTP events are not implemented. Neither streaming nor hot
reload is provided. Do not propose outbound HTTP, local file storage, environment-based secrets, or
database clients as working handler capabilities. Surface the missing capability
when a requested application depends on one.

CLI defaults are 5 seconds per execution, 16 concurrent executions, a 1 MiB
request-body limit, and a 10-second body-reading timeout. The CLI and manifest
do not expose settings for these limits. Requests beyond execution capacity
receive 503; requests that exceed execution time receive 504.

## Troubleshooting

| Symptom | Next step |
| --- | --- |
| Manifest validation or missing source | Check the fixed `application.yaml` filename, name rules, supported fields, and matching `functions/<name>.rs`. |
| Setup refuses root Cargo manifest | Preserve the handwritten manifest and use a separate application directory. |
| Setup rejects editor settings | Repair invalid JSONC or a non-string-array `rust-analyzer.vfs.extraIncludes`, preserving unrelated settings. |
| Build reports a missing or outdated lockfile | Run `wr update <DIRECTORY>` for intentional dependency or function-list changes, or to create a new lockfile; review it and build again. Use `--unlocked` only to explicitly allow lockfile changes during a build. |
| Cargo compilation fails | Read Cargo's diagnostics against the original sources and YAML dependencies. Previously published components remain unchanged. |
| Run reports missing/incompatible component | Rebuild the application with the installed `wr`, then redeploy (or restart foreground serving). |
| Run rejects imports | Check dependency and handler use of unsupported capabilities. |
| No readiness message | Read the startup error; check the manifest, conflicting routes, and whether the listen port is already in use. |
| HTTP 404 | Check the declared path-prefix, method, and application-relative route. |
| HTTP 413 / 408 | Request body too large / body reading timed out. |
| HTTP 503 / 504 | Execution capacity unavailable or server shutting down / handler execution timed out. |
| HTTP 500 | Inspect the server terminal for a handler failure or invalid response; the handler may also deliberately return 500. |
| Old response after editing | Rebuild successfully, then redeploy, or restart the foreground server, to load new components. |
