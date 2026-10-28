# Architecture

The current implementation is a single Rust package containing the `wr` executable.
It loads and validates an application manifest
and prints metadata, generates a guest workspace, or builds HTTP components.
It also cleans generated workspaces and build outputs without loading a manifest.
[SPEC.md](SPEC.md) describes the implemented behavior.

## Structure

- [main](src/main.rs) invokes the CLI, reports failures, and returns an exit status.
- [cli](src/cli/mod.rs) defines the top-level parser and command dispatch, with a
  shared helper for resolving application directories. Each command module owns
  its arguments, workflow, and presentation:
  [setup](src/cli/setup.rs) previews or writes a generated workspace;
  [build](src/cli/build.rs) refreshes the workspace, compiles it, publishes
  components, and reports their paths; [clean](src/cli/clean.rs) removes generated
  files; [run](src/cli/run.rs) loads and prints application metadata.
  Setup, build, and run load the manifest once within their workflows. Clean does
  not load it, so broken applications can still be cleaned.
- [services](src/services/mod.rs) supplies operations independent of CLI argument
  types and presentation. Services return data or errors and never depend on the
  CLI. Cargo's own diagnostics are inherited directly during compilation.
- [manifest](src/services/manifest.rs) owns application and dependency declaration
  types, loads YAML, validates the resolved document, resolves module paths,
  checks route uniqueness and source-file existence, and returns application data.
- [build](src/services/build.rs) invokes Cargo and publishes components from
  deterministic target/profile/library paths, returning the published paths.
- [workspace](src/services/workspace.rs) discovers direct `functions/*.rs` sources,
  selects their dependencies from the application, and renders Cargo manifests,
  entry points, and the embedded WIT contract. It returns a sorted map of relative file paths
  to contents, including merged ignore rules and editor settings read from the
  application's existing `.gitignore` and `.vscode/settings.json`.
  `jsonc-parser` edits settings while preserving comments and formatting.
  Generation does not write files or run processes. TOML serialization is handled
  by `toml`. It rejects ambiguous global/local dependency declarations. A separate
  writer refuses to overwrite a handwritten root Cargo manifest, creates parent
  directories, and writes changed generated files without touching identical
  files. A private helper appends missing
  literal ignore entries in memory; generation includes this content in the same
  file map used for previewing and writing.
  Cleanup removes only `.wr`, `target`, and the generated root Cargo manifest,
  checking the manifest's ownership marker before any removals. It preserves
  lockfiles and user configuration and unlinks directory symlinks without
  following them.

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
original sources as modules. For editor analysis, rust-analyzer also needs the
original `functions` directory in `vfs.extraIncludes`, because it is outside the
generated Cargo package directories. Setup merges this setting automatically;
Cargo builds need no such setting.

The workspace service embeds the checked-in [WIT world](wit/world.wit), copies it into
the generated workspace, and supplies a managed `wit-bindgen` dependency.
Generated entry points implement the WIT-exported handler trait and expose
`Request` and `Response` at the crate root. User handlers return those types
directly; no parallel request/response model or conversion layer is introduced.

Build uses ordinary Cargo targeting `wasm32-wasip2`, an explicit application-local
target directory, and debug or release profile. Cargo owns dependency resolution,
incremental compilation, concurrency, diagnostics, and the application lockfile.
The build service computes artifact locations rather than parsing JSON messages,
and copies all expected outputs only after Cargo succeeds and every artifact
exists. Writes and publication are not transactional. Obsolete generated files
and components remain untouched.

The HTTP world is the only supported trigger. Event-specific worlds and shared
host capability imports can be introduced separately; no generic trigger
framework is needed for this increment. WASI imports from the compilation target
will need host implementations and a capability policy when execution is added.
See the [guest notes](wit/README.md).
