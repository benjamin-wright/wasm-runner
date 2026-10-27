# Architecture

The current implementation is a Cargo workspace containing the `wr` executable
and the `wasm-runner-setup` library. It loads and validates an application manifest
and prints metadata or generates a guest workspace.
[SPEC.md](SPEC.md) describes the implemented behavior.

## Structure

- [main](src/main.rs) dispatches commands and reports failures.
- [cli](src/cli.rs) defines command-line arguments using `clap`.
- [manifest](src/manifest.rs) loads YAML, validates the resolved document,
  resolves module paths, checks route uniqueness, and returns application data.
- [run](src/run.rs) coordinates loading and prints metadata.
- [setup](src/setup.rs) coordinates loading, selects source functions, and either
  writes the generated workspace or prints its file layout and contents for
  `--dry-run`.
- [wasm-runner-setup](crates/wasm-runner-setup/src/lib.rs) defines dependency
  declarations and renders Cargo manifests. It takes validated
  function names and dependencies and returns a sorted map of relative file paths
  to contents, including merged ignore rules read from the application's existing
  `.gitignore`. Generation does not write files or run processes. TOML serialization is handled
  by `toml`. It rejects ambiguous global/local dependency declarations. A separate
  writer refuses to overwrite a handwritten root Cargo manifest, creates parent
  directories, and writes the generated files. A private helper appends missing
  literal ignore entries in memory; generation includes this content in the same
  file map used for previewing and writing.

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

Source functions omit the module path; manifest loading resolves their expected
output to `.wr/components/<name>.wasm`. The generated guest workspace has one
`cdylib` crate per source function. Its root manifest is beside the application
manifest, separate from the host's development workspace. Member manifests stay
under `.wr/functions`, explicitly select the application workspace, and point
directly at source files with `lib.path`. This allows rust-analyzer to discover
the application workspace and its source roots when opened as an editor workspace.
Cargo maintains the guest lockfile at the application root. Setup does not produce
WIT exports or execute builds. Removal of obsolete generated files remains future work.

The checked-in [WIT world](wit/world.wit) sketches a guest interface but is not
used by the current executable. See the [guest notes](wit/README.md).
