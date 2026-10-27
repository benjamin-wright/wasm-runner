# Architecture

The current implementation is a Cargo workspace containing the `wr` executable
and the `wasm-runner-setup` library. It loads and validates an application manifest
and prints metadata, generates a guest workspace, or builds HTTP components.
[SPEC.md](SPEC.md) describes the implemented behavior.

## Structure

- [main](src/main.rs) resolves the application directory, loads `application.yaml`
  once, passes the application to command handlers, and reports failures.
- [cli](src/cli.rs) defines command-line arguments using `clap`.
- [manifest](src/manifest.rs) loads YAML, validates the resolved document,
  resolves module paths, checks route uniqueness, and returns application data.
- [run](src/run.rs) prints metadata without executing components.
- [setup](src/setup.rs) selects discovered function dependencies and either
  writes the generated workspace or prints its file layout and contents for
  `--dry-run`.
- [build](src/build.rs) refreshes the generated workspace, invokes Cargo once,
  and publishes components from deterministic target/profile/library paths.
- [wasm-runner-setup](crates/wasm-runner-setup/src/lib.rs) defines dependency
  declarations, discovers direct `functions/*.rs` sources, and renders Cargo manifests,
  entry points, and the embedded WIT contract. It takes validated
  function names and dependencies and returns a sorted map of relative file paths
  to contents, including merged ignore rules read from the application's existing
  `.gitignore`. Generation does not write files or run processes. TOML serialization is handled
  by `toml`. It rejects ambiguous global/local dependency declarations. A separate
  writer refuses to overwrite a handwritten root Cargo manifest, creates parent
  directories, and writes changed generated files without touching identical
  files. A private helper appends missing
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

Component paths and HTTP routes are currently metadata, not executable resources.
There is no component runtime or HTTP server. Their structure and technical
details will be documented when implemented and established.

Every direct Rust file in `functions` is a target, independent of route
configuration. Manifest loading rejects prebuilt module declarations and missing
configured sources, and resolves component paths to `.wr/components/<name>.wasm`.
Discovery validates and sorts source names. The generated guest workspace has
one `cdylib` crate per source, with explicit library names for deterministic
artifacts. Its root manifest is beside `application.yaml`, separate from the
host development workspace. Member manifests under `.wr/functions` explicitly
select that workspace and point at generated entry points. These include the
original sources as modules, so rust-analyzer can discover both.

The setup crate embeds the checked-in [WIT world](wit/world.wit), copies it into
the generated workspace, and supplies a managed `wit-bindgen` dependency.
Generated entry points implement the WIT-exported handler trait and expose
`Request` and `Response` at the crate root. User handlers return those types
directly; no parallel request/response model or conversion layer is introduced.

Build uses ordinary Cargo targeting `wasm32-wasip2`, an explicit application-local
target directory, and debug or release profile. Cargo owns dependency resolution,
incremental compilation, concurrency, diagnostics, and the application lockfile.
The build command computes artifact locations rather than parsing JSON messages,
and copies all expected outputs only after Cargo succeeds and every artifact
exists. Writes and publication are not transactional. Obsolete generated files
and components remain untouched.

The HTTP world is the only supported trigger. Event-specific worlds and shared
host capability imports can be introduced separately; no generic trigger
framework is needed for this increment. WASI imports from the compilation target
will need host implementations and a capability policy when execution is added.
See the [guest notes](wit/README.md).
