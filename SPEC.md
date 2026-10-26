# Current functionality

`wr` is a command-line tool that loads an application manifest, validates it,
prints its metadata, and exits. It does not load WebAssembly components or serve
HTTP requests.

## Command line

```sh
wr run examples/application.yaml
wr run examples/application.yaml --listen 127.0.0.1:9000
```

The manifest path is required. The optional listener address accepts a numeric
IP address and port and defaults to `127.0.0.1:8080`. It is reported as
configuration only; no socket is opened. Environment variables do not alter
application configuration.

## Manifest

An application has a name and a nonempty collection of named functions. Each
function declares a module path and an HTTP method/path pair. See the
[example manifest](examples/application.yaml) for the format and
[application.schema.json](application.schema.json) for field constraints.

The tool accepts one YAML document, including anchors, aliases, and merge keys.
It validates the resolved document against its embedded schema and rejects
duplicate mapping keys and duplicate method/path pairs.

Relative module paths resolve against the manifest's directory. Absolute paths
are also accepted. Module files do not need to exist for metadata output.

## Output and errors

On success, print the application name, manifest path, configured listener,
function count, and routes with resolved module paths in function-name order.
The output states that component execution and HTTP serving are not implemented.

Invalid arguments or a manifest loading or validation failure produce an error
and a nonzero exit status. Manifest failures produce no metadata output.
Diagnostics use the detail provided by the parsing and validation libraries,
with basic operation context; no particular error-location format is required.
