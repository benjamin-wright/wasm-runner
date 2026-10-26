# Architecture

The current implementation is one Rust command-line executable, `wr`, in one
Cargo package. It loads and validates an application manifest and prints metadata.
[SPEC.md](SPEC.md) describes the implemented behavior.

## Structure

- [main](src/main.rs) dispatches commands and reports failures.
- [cli](src/cli.rs) defines command-line arguments using `clap`.
- [manifest](src/manifest.rs) loads YAML, validates the resolved document,
  resolves module paths, checks route uniqueness, and returns application data.
- [run](src/run.rs) coordinates loading and prints metadata.

## Validation and errors

YAML parsing and merge resolution are delegated to `yaml_serde`. The complete
resolved document is validated by `jsonschema` against the embedded
[application schema](application.schema.json), then deserialized into application
types. Route uniqueness is checked separately because it spans functions.

Parsing and validation libraries supply diagnostic detail. The command adds
basic context and reports failures through `anyhow`; custom source-location
tracking is not part of the design.

## Boundaries

Module paths and HTTP routes are currently metadata, not executable resources.
There is no component runtime or HTTP server. Their structure and technical
details will be documented when implemented and established.

The checked-in [WIT world](wit/world.wit) sketches a guest interface but is not
used by the current executable. See the [guest notes](wit/README.md).
