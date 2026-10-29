# Documentation review and proposed structure

Review date: 28 October 2026. Scope: the documentation and supporting files in this checkout, plus published guidance on technical documentation, architecture, requirements, and agent instructions. This records the proposal before migration. The documentation has since been reorganized; findings and counts below describe the pre-migration checkout, and section links now follow the migrated content. Use [Contributing](CONTRIBUTING.md#documentation-ownership) for the maintained documentation map.

The project has useful, unusually specific documentation for its size. Its main weakness is unclear ownership of information: the README, specification, and architecture guide repeatedly explain the same workflows at similar levels of detail. At the same time, readers must assemble a first-use journey themselves, and the product rationale is largely absent. One agent instruction directly contradicts the implemented source-based build workflow.

Keep the proposed user, contributor, and product perspectives, but refine them into **application authors**, **contributors and maintainers**, and **product decision-makers**. Use those as entry points, not three separate copies of the documentation. Organize individual pages around their purpose: orientation, learning, completing a task, looking up a contract, understanding a design, or agreeing requirements. Retain one shared reference and the existing machine-readable contracts.

## Current documentation

The seven authored Markdown files total 705 lines and approximately 5,004 whitespace-delimited words. The four main project documents account for 546 lines and 3,936 words. These counts exclude this report, generated files, source comments, and machine-readable contracts; they measure size, not the percentage of duplicated content.

| Document | Size | Current role and assessment |
| --- | --- | --- |
| [README.md](README.md) | 269 lines; 1,796 words | Introduction, installation, command reference, guest tutorial, workspace internals, editor troubleshooting, contributor workflow, and architecture summary. Most of the useful content is here, but its purposes compete. |
| [SPEC.md](SPEC.md) | 152 lines; 1,147 words | Detailed current-behavior specification. Useful as a source for reference material and acceptance criteria, but mixes external guarantees with implementation choices. It does not establish the product problem or priorities. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 95 lines; 752 words | Component responsibilities, validation flow, guest generation, build strategy, and future boundaries. Good foundation; repeats behavior and contains some private implementation narration. |
| [wit/README.md](wit/README.md) | 30 lines; 241 words | Guest contract explanation and authoring instructions, with future event/capability design guidance mixed in. Appropriately close to its authoritative interface. |
| [AGENTS.md](AGENTS.md) | 64 lines; 411 words | Agent working rules, engineering principles, verification requirements, and routing to skills. Some principles also belong in a discoverable contributor guide. |
| [Design skill](.agents/skills/wasm-runner-change-design/SKILL.md) | 48 lines; 315 words | A focused design workflow, but contains a stale assertion that the current workflow uses prebuilt components. |
| [Test-selection skill](.agents/skills/wasm-runner-test-selection/SKILL.md) | 47 lines; 342 words | A focused verification workflow. Its pointer to architecture “proof points” is imprecise: the architecture document provides explanation, not an explicit verification map. |

Other documentation surfaces matter too:

- [application.schema.json](application.schema.json) defines manifest shapes and field constraints. Cross-function and filesystem-dependent validation also lives in the implementation.
- [wit/world.wit](wit/world.wit) defines the guest interface. The adjacent prose explains its use and limitations.
- [examples/application.yaml](examples/application.yaml), [greet.rs](examples/functions/greet.rs), and [echo.rs](examples/functions/echo.rs) provide concrete examples. Generated application files should not become another manually maintained specification.
- [Makefile](Makefile) and [rust-toolchain.toml](rust-toolchain.toml) define lifecycle commands and toolchain configuration.
- [CLI source](src/cli/mod.rs) and its command modules define help text and argument parsing. [tests/cli.rs](tests/cli.rs) supplies executable examples of expected behavior, although it is not a substitute for user documentation.

### Strengths to preserve

The documents clearly acknowledge that HTTP serving and component execution are not implemented. They explain overwrite protection, dry-run behavior, cleanup boundaries, lockfiles, and publication failure limitations. The architecture already distinguishes CLI orchestration from services, and delegates compilation concerns to Cargo. The schema, WIT, examples, and source links provide a strong basis for documentation that can stay close to the implementation.

All **54 local Markdown link occurrences** in the seven documents resolved to existing files or directories in this checkout. The problem is primarily meaning and organization, rather than broken navigation targets.

## What published guidance supports

The sources below offer complementary principles. None establishes a universal list of stakeholder audiences or requires this project to adopt a particular directory layout.

| Guidance | Relevant principle | Application here |
| --- | --- | --- |
| [Google Technical Writing: Audience](https://developers.google.com/tech-writing/one/audience) | Roles are a starting point; existing knowledge, familiarity, and the task also determine what readers need. | Distinguish someone writing a guest function from someone modifying the runner, even though both are Rust developers. |
| [Diátaxis](https://diataxis.fr/) | Tutorials, how-to guides, reference, and explanation serve different information needs. | Separate a first successful build, practical recipes, exact behavior, and architecture reasoning. These are document purposes, not four audiences. |
| [Applying Diátaxis](https://diataxis.fr/how-to-use-diataxis/) | Improve real material incrementally; do not create empty sections to satisfy a framework. | Introduce only pages justified by existing content and actual gaps. Avoid a directory matrix of every audience and document type. |
| [GitHub: About README files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-readmes) | A README orients visitors: what the project does, why it is useful, and how to get started. | Make the root README the entrance, with a clear capability statement and links to deeper material. |
| [arc42: Solution strategy](https://docs.arc42.org/section-4/) and [architecture decisions](https://docs.arc42.org/section-9/) | Explain significant choices and their reasons concisely; link instead of repeating, and record consequential decisions. | Focus architecture on boundaries, important flows, constraints, and tradeoffs. Add separate decision records only when useful. |
| [NASA: Writing a good requirement](https://www.nasa.gov/reference/appendix-c-how-to-write-a-good-requirement/) | Requirements should be clear, necessary, individually understandable, verifiable, and distinguish desired outcomes from implementation. | Extract a small set of product commitments from the behavioral inventory, with rationale and acceptance evidence. Do not import a heavyweight systems-engineering process. |
| [Write the Docs: Docs as code](https://www.writethedocs.org/guide/docs-as-code/) | Documentation benefits from the development team's review and maintenance workflow. | Keep repository documentation close to changes, and review affected contracts, examples, and guidance together. |
| [OpenAI: AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md) and [Build skills](https://learn.chatgpt.com/docs/build-skills) | Project instructions establish working expectations; skills package focused workflows and load their detailed guidance when needed. | Keep agent entry instructions and conditional procedures distinct from product facts. Link skills to the current contracts instead of embedding a second account of current capability. |

The proposed structure below is an application of these principles to this checkout, not a claim that the sources prescribe these exact files.

## Recommended audiences and entry points

| Reader | Questions to answer | Recommended route |
| --- | --- | --- |
| Evaluator or first-time visitor | What exists today? Is it useful for my task? What cannot it do yet? | README, then the first-use guide. This is an entry stage, not a separate documentation collection. |
| Application author | How do I write a function, configure routes and dependencies, build it, and work in my editor? What files will commands change? | First-use guide, task guides, shared reference, and guest contract. |
| Contributor or maintainer | How do I build and check the runner? Where does a change belong? Which guarantees and design constraints must it preserve? | Contributor guide, architecture, requirements, and the same shared reference. |
| Product decision-maker | Whose problem are we solving? Which outcomes are committed? What is excluded or undecided? How will success be judged? | Product requirements, with links to relevant design decisions and contracts. |

The original three perspectives are therefore useful, with qualifications:

- **“User” needs a concrete definition.** The current user is primarily a developer authoring an application. The HTTP clients of a future server are not the current primary documentation readers.
- **“Contributor” needs practical instructions as well as high-level design.** Architecture alone does not explain how to make and verify a change.
- **“Product” describes a decision context shared by people.** Maintainers may also make product decisions; there need not be a separate product team. Requirements remain useful as a separate document purpose.
- **Agents are another consumer of engineering guidance.** They do not need a parallel product knowledge base. Their operational instructions should lead to the same maintained contracts used by humans.
- **Operators may become a distinct audience when a server exists.** At that point, deployment, configuration, observability, failure recovery, and capability policy will justify operational documentation. Creating those guides now would risk documenting imaginary behavior.

A strict audience-only layout would reproduce the current problem: both contributors and application authors need the manifest and guest contracts. A strict four-part Diátaxis layout would also be incomplete for this task, because product commitments and agent operating rules have additional purposes. Use audience-aware navigation with purpose-specific pages.

## Findings and consequences

### A stale agent instruction contradicts the supported workflow

**Priority: high. Confirmed contradiction.** The [design skill](.agents/skills/wasm-runner-change-design/SKILL.md#choose-responsibilities-and-the-simplest-approach) says the current workflow takes prebuilt local components and treats automatic guest builds as a new scope expansion. The [manifest specification](docs/reference.md#manifest) rejects prebuilt `module` declarations, while the [build command](src/cli/build.rs) generates and compiles a source workspace.

An agent can consequently interpret an existing capability as unauthorized expansion. This is more consequential than duplicated prose because the text actively governs changes. Replace the stale capability assertion with a direction to consult current requirements and reference material. Preserve the useful rule against introducing unrelated host capabilities.

The [test-selection skill](.agents/skills/wasm-runner-test-selection/SKILL.md#select-meaningful-verification) should likewise point to actual requirement acceptance criteria and existing tests, rather than suggesting that architecture is the primary location of verification “proof points.”

### The README repeatedly changes purpose and level of detail

**Priority: high. Information architecture problem.** [Local setup](docs/getting-started.md#prerequisites-and-installation) combines installation with listener syntax, validation semantics, and diagnostic policy. [Guest workspace setup](docs/reference.md#setup) combines author instructions with package naming and generated entry-point internals. [CLI development](CONTRIBUTING.md#development-workflow) adds both contributor commands and a source-level architecture tour.

The result is useful to an already-informed maintainer but harder for a new application author to scan. A reader installing the tool does not yet need to know why the build service avoids parsing Cargo JSON messages. Move detailed behavior to reference, recipes to guides, and internal rationale to architecture.

### The specification does not yet serve the proposed product purpose

**Priority: high. Missing purpose, not an inaccurate document title.** [SPEC.md](SPEC.md) honestly presents itself as “Current functionality.” It is valuable in that role. However, it provides little explicit account of the target user, the problem being solved, why particular constraints matter, or which outcomes guide future decisions.

Its [setup section](docs/reference.md#setup) also combines preservation guarantees with exact generated package structure, dependency inheritance, and wrapper construction. Readers cannot readily distinguish a product commitment from a changeable implementation choice.

Preserve the externally significant guarantees, but organize them as concise requirements. Put operational detail in shared reference and implementation reasoning in architecture. Separate implemented commitments, accepted future requirements, and open questions. Do not turn mentions of possible runtime features into an approved roadmap.

### Architecture is partly a second behavioral reference

**Priority: medium. Duplicated ownership.** [Architecture boundaries](ARCHITECTURE.md#guest-workspace-boundary) repeats discovery rules, module paths, editor setup, build options, and publication behavior already described in the README and specification. The [structure section](ARCHITECTURE.md#structure) goes as far as describing a private helper that appends literal ignore entries.

Retain the responsibility map and explain the important flows and reasons: why guest and runner workspaces are separate, why generation precedes writing, why Cargo owns compilation, and why WIT-generated types cross the handler boundary directly. Link to exact public behavior instead of restating it. Internal function-by-function narration belongs in code only when it adds information the code cannot readily express.

### First use is incomplete as a learning journey

**Priority: medium. Usability gap.** The [initial command sequence](docs/getting-started.md#prerequisites-and-installation) installs and checks the runner, sets up the example, builds it, and invokes `run`. It does not show an expected result alongside those steps. The minimal handler appears later, and the manifest example is linked much later still.

A reader needs an early explanation of the application directory, manifest, function source, resulting component, and metadata-only run. The tutorial should finish with an observable success condition: components exist and the manifest can be inspected. It should explicitly explain that there is no HTTP endpoint to call yet.

Use `wr` consistently after installation. Put `cargo run --locked -- ...` and the direct debug-binary workflow in contributor instructions. Explain that explicit setup is useful for previewing or editor preparation, while build already performs setup. Keep repository verification available without presenting it as an unexplained guest-authoring step.

### Guest notes combine a current contract with prospective design

**Priority: medium. Status ambiguity.** [wit/README.md](wit/README.md) succinctly explains the current handler and the custom interface. Its final paragraphs then prescribe how future event handlers and SQL/KV capabilities should be modeled.

Those ideas may be sensible, but a guest author cannot act on them today. Keep a brief current limitation in the guest notes. Move accepted architectural constraints to architecture, and unresolved possibilities to the requirements document's open questions. Preserve the distinction between the WIT world's declared imports and imports introduced by the compilation target.

### Important reference distinctions are dispersed or overly broad

**Priority: medium. Precision and discoverability.** Three examples illustrate the needed reference structure:

- The [schema](application.schema.json) constrains method names and literal route paths, while [manifest loading](src/services/manifest.rs) checks route uniqueness and configured source existence. [Workspace generation](src/services/workspace.rs) performs additional dependency checks. A schema-valid manifest is not a guarantee that every command will succeed. Document the validation stages together.
- The [README](docs/getting-started.md#prerequisites-and-installation) says environment variables do not alter configuration. That is useful for application settings, but overly broad when read as a claim about builds. The [build service](src/services/build.rs) invokes Cargo without clearing its environment and explicitly overrides its target directory. Scope the statement to application/listener configuration and link to Cargo's separate build concerns.
- Source discovery and route configuration are independent, but the [schema](application.schema.json) still requires a nonempty configured function map. Unlisted functions are additional build targets; the documented behavior does not establish support for a completely route-free manifest. Explain this distinction in one place.

Do not solve these issues by reproducing the entire JSON Schema or WIT file in prose. Provide a readable model, examples, validation boundaries, and direct links to exact definitions.

### Current capability and product ambition need clearer opening language

**Priority: medium. Expectation setting.** The [README opening](README.md) calls the project a standalone application server, then immediately explains that it does not serve HTTP. The limitation is disclosed clearly; the issue is the conflicting first impression, not concealment.

A possible opening is: “WASM Runner is a Rust CLI for preparing and building WebAssembly function components from an application manifest. HTTP serving is planned; currently, `wr run` validates the application and prints metadata.” Keep the longer-term ambition distinct from the current capability statement. The [package description](Cargo.toml) should be considered too, because the [top-level parser](src/cli/mod.rs) derives its help description from package metadata.

### Maintenance rules do not assign information a clear home

**Priority: medium. Recurrence risk.** The [design skill](.agents/skills/wasm-runner-change-design/SKILL.md#establish-the-contract-and-current-state) already says to update affected documents with contract changes. What is missing is a rule for deciding which document owns a fact. Updating all three broad documents perpetuates duplication.

The contributor guide should state which pages own product outcomes, exact behavior, implementation rationale, examples, and working procedures. Agent instructions should route to that policy. Architecture explanations, product commitments, and reference facts should not all require editing for a purely internal refactor.

## Which overlap to retain

The goal is one authoritative home for each detailed fact, with brief contextual summaries where readers need them. Zero repetition would create excessive cross-referencing and make individual guides harder to use.

| Topic currently repeated | Current locations | Recommended ownership and permitted overlap |
| --- | --- | --- |
| Runtime not implemented | [README](README.md), [specification](SPEC.md), [architecture](ARCHITECTURE.md), [guest notes](wit/README.md) | Retain a short limitation wherever it prevents misunderstanding. Put scope/status decisions in requirements. |
| Source discovery and dependency rules | [README setup](docs/reference.md#setup), [specification manifest](docs/reference.md#manifest), [architecture boundaries](ARCHITECTURE.md#guest-workspace-boundary) | Shared reference owns precise behavior; a guide demonstrates adding a function or dependency; architecture explains the source/route separation only where useful. |
| Generated workspace and wrapper structure | [README setup](docs/reference.md#setup), [specification setup](docs/reference.md#setup), [architecture boundaries](ARCHITECTURE.md#guest-workspace-boundary) | Architecture owns internal construction. Reference owns externally relevant generated paths and overwrite/preservation behavior. |
| Editor setup | [README setup](docs/reference.md#setup), [README VS Code](docs/guides.md#prepare-vs-code), [specification setup](docs/reference.md#setup), [architecture](ARCHITECTURE.md#guest-workspace-boundary) | Guide owns editor steps and troubleshooting. Reference owns automatic settings changes. Architecture retains only the reason generated packages need access to original sources. |
| Build failure and cleanup guarantees | [README build](docs/reference.md#build), [README clean](docs/reference.md#clean), [specification](SPEC.md), [architecture](ARCHITECTURE.md) | Requirements own the promised outcomes; reference gives exact scope and caveats. Safety-critical cautions remain next to the relevant commands. |
| CLI/service responsibility map | [README development](CONTRIBUTING.md#development-workflow), [architecture structure](ARCHITECTURE.md#structure) | Architecture owns the map. Contributor guide links to it. |
| Handler types and signatures | [README setup](docs/reference.md#setup), [specification build](docs/reference.md#build), [guest notes](wit/README.md), [WIT](wit/world.wit) | WIT owns interface definitions; guest notes own author-facing semantics. Tutorial uses one small example and links to those notes. |
| Development and verification procedures | [README development](CONTRIBUTING.md#development-workflow), [agent instructions](AGENTS.md), [test-selection skill](.agents/skills/wasm-runner-test-selection/SKILL.md) | Contributor guide owns human-readable policy and commands; agent instructions retain mandatory routing and agent-specific constraints; skill owns conditional execution steps. |

File-preservation rules and partial-failure caveats are not expendable implementation trivia. Likewise, an output path can be an external contract even though it is chosen in code. Classify material by its effect on readers and compatibility, not just whether it sounds technical.

## Proposed ideal structure for the current project

Use a small set of purpose-specific Markdown pages. Retain established root filenames and contract locations. The following is a proposed layout, not a set of files created by this review:

```text
README.md
CONTRIBUTING.md                  new
SPEC.md
ARCHITECTURE.md
docs/
  getting-started.md             new
  guides.md                      new
  reference.md                   new
wit/
  README.md
  world.wit
application.schema.json
examples/
AGENTS.md
.agents/skills/
```

### Page responsibilities

| Page | Primary purpose | Contents |
| --- | --- | --- |
| [README.md](README.md) | Orientation and navigation | One clear current-capability statement, intended use, current limits, and routes for “build an application,” “contribute,” and “understand requirements/design.” Aim for a few screens, not an exhaustive manual. |
| Proposed `docs/getting-started.md` | Guided first success | Prerequisites and source installation; explain the checked-in example; build components; inspect metadata; show expected output shape and file locations; suggest one small handler edit. State the working directory for commands. |
| Proposed `docs/guides.md` | Completing recurring tasks | Separate recipes for adding a function, configuring dependencies, preparing VS Code, making a locked/release build, and cleaning/recovering generated state. Keep each recipe independently usable. |
| Proposed `docs/reference.md` | Shared behavioral reference | Command arguments/defaults; manifest model and validation stages; source discovery; generated files and ownership; outputs, errors, preservation, and failure limitations. Use clear sections so these can split later if needed. |
| [wit/README.md](wit/README.md) | Guest authoring contract | Required Rust entry point, generated types, request/response semantics, error representation, interface identity, and current limitations. Link to WIT and examples. |
| Proposed `CONTRIBUTING.md` | Changing the runner | Development prerequisites and lifecycle, edit/check loop, engineering and test principles, documentation ownership, and links to architecture and requirements. Installation instructions shared with first use should be linked rather than independently maintained. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Understanding implementation decisions | Context, runner/guest boundary, responsibility map, important flows, significant rationale/tradeoffs, and known architectural limitations. One small flow diagram would replace repeated procedural paragraphs. |
| [SPEC.md](SPEC.md), retitled “Product requirements” | Agreeing scope and outcomes | Target user and problem, current scope, concise requirements with acceptance evidence, deliberate exclusions, and explicitly unresolved decisions. Link to detailed public contracts. |
| [AGENTS.md](AGENTS.md) and [skills](.agents/skills) | Agent operating instructions | Concise mandatory working rules, links to shared engineering policy and authoritative docs, and focused conditional procedures. Keep agent-specific Git/permission restrictions here. |

The [schema](application.schema.json), [WIT](wit/world.wit), [examples](examples), [Makefile](Makefile), and [toolchain file](rust-toolchain.toml) remain their respective executable or machine-readable definitions.

This introduces four new prose pages by extracting existing material and filling the first-use gap. It does not require a site generator, documentation framework, new dependencies, a separate product portal, or an ADR directory on day one. A single shared reference is proportionate to this codebase; split it into CLI, manifest, and workspace pages only when its size or reader tasks warrant that change.

### How navigation would work

The README should offer descriptive links such as “Build your first application,” “Common tasks,” “Command and manifest reference,” “Write a guest function,” “Contribute to the runner,” and “Requirements and architecture.” A contributor can follow the shared manifest reference directly; they should not have to find its equivalent under another audience's folder.

Every deeper page should identify its scope in the opening paragraph and link to related tasks or definitions where needed. Use headings that describe tasks or subjects, spell out unfamiliar terms such as WebAssembly Interface Types (WIT) on first introduction, and use descriptive link text. Avoid a separate glossary until repeated terminology actually needs one.

## What concise product requirements would look like

The examples below show how to restructure existing commitments; the identifiers and grouping are proposed. They are not new feature approvals or proof of a fresh passing test run.

Start with a short statement of the application author's problem. The current documents suggest reducing the work of assembling Rust function components and route configuration. That rationale should be confirmed as product intent rather than silently treated as established history.

| Proposed requirement | Acceptance evidence or detailed contract |
| --- | --- |
| R1. Manifest-dependent commands reject an invalid manifest or a missing configured source before producing successful application results. | Manifest/validation reference; relevant CLI failure cases in [tests/cli.rs](tests/cli.rs). |
| R2. An application author can preview generated workspace files without modifying the application directory. | Setup reference; existing preview and write-equivalence tests. |
| R3. Build produces a component for every valid directly discovered function source, including sources without route entries. | Discovery reference and [WIT contract](wit/world.wit); existing component build test. |
| R4. A failed Cargo compilation leaves previously published components unchanged. | Build reference; existing Cargo-failure preservation test. Publication-copy failure remains separately documented as nontransactional. |
| R5. Cleanup removes only the documented generated workspace and build outputs while preserving application inputs and user configuration. | Exact cleanup reference; existing preservation and ownership-protection tests. |
| R6. In the current increment, run reports validated application metadata and explicitly communicates that it does not execute components or serve HTTP. | Run reference; existing metadata-only CLI test. |

Each requirement should have a brief reason where it is not self-evident and an observable acceptance condition. Important constraints such as protection of handwritten files must remain explicit. Short identifiers help changes and tests refer to requirements without copying their entire text.

Detailed reference can be incorporated by link for things such as accepted manifest fields and the exact cleanup set. The requirements document should make that relationship explicit. Architecture then explains how the system meets the requirements. Tests provide evidence of behavior; they do not independently determine product intent or automatically convert an implementation accident into a commitment.

Do not copy every option, generated package name, parser library, or internal algorithm into this table. Conversely, do not reduce requirements to vague ambitions such as “safe,” “simple,” or “fast.” Preserve specific public guarantees and mark any proposed future requirement separately from what works today.

For future HTTP serving, record genuine open decisions such as execution behavior and capability policy without inventing answers, performance targets, delivery dates, or operating-system support. A short open-questions section is sufficient until a concrete increment is agreed.

## Migration sequence

### First pass

Correct the stale prebuilt-component assertion in the [design skill](.agents/skills/wasm-runner-change-design/SKILL.md). Establish the page responsibility table in contributor guidance. Clarify current capability in the [README](README.md) and consider the [package description](Cargo.toml). These are small changes with immediate benefit.

### Second pass

| Existing content | Destination and treatment |
| --- | --- |
| [README local setup](docs/getting-started.md#prerequisites-and-installation) | Installation and first example to the first-use guide; listener and validation detail to reference. Keep a short capability summary in the README. |
| [README guest setup](docs/reference.md#setup) | Authoring steps to guides/guest notes; dependency and discovery rules to reference; wrapper construction to architecture. |
| [README build, editor, and cleanup material](docs/reference.md#build) | Commands and troubleshooting to guides; exact side effects and failure behavior to reference. Retain relevant cautions beside task steps. |
| [README CLI development](CONTRIBUTING.md#development-workflow) | Commands and engineering workflow to contributor guide; responsibility map to architecture. |
| [Specification](SPEC.md) | Extract exact behavior to reference; keep meaningful product outcomes, constraints, and acceptance conditions in the narrowed specification. |
| [Architecture](ARCHITECTURE.md) | Consolidate responsibility descriptions; replace duplicated reference paragraphs with rationale and links. |
| [Guest notes](wit/README.md) | Preserve current authoring contract; move future design guidance according to its accepted or unresolved status. |
| [Agent instructions](AGENTS.md) and [skills](.agents/skills) | Update document routing and semantic descriptions of the targets; eliminate embedded capability summaries that can become stale. |

Move useful material before deleting its old copy. Review each removal against the original preservation, validation, and failure guarantees. Keep the schema and WIT at their current paths to avoid unnecessary changes to embedding and generation.

### Third pass

Write and walk through the first-use guide, then follow representative tasks from the README: adding a dependency, configuring the editor, understanding cleanup, and locating the correct code boundary for a change. Add expected outcomes and prerequisite explanations where the walkthrough exposes a gap.

Check all inbound links, including relative paths in skills and textual pointers in diagnostics. The [build service](src/services/build.rs) currently refers users to the README for prerequisites; keeping a clear setup link there preserves that route. If code or package metadata is changed during implementation, run `make check` under the existing project rules.

### Fourth pass

Assign responsibility by the type of change, rather than inventing a separate documentation team:

- A behavior change updates the applicable requirement, reference section, and affected examples or guides together.
- An internal refactor updates architecture only where responsibilities, flows, or important rationale change.
- A workflow change updates contributor instructions and the affected agent procedure.
- A schema or WIT change updates its authoritative definition and the adjacent explanation/examples.
- A capability-status change updates the short summaries that readers rely on at entry points.

At review time, ask whether each detailed fact has a clear owner, whether repeated summaries agree, and whether a new reader can finish the documented task. Link checking alone would not have caught the stale design skill.

Keep automated checks proportionate: validate local links and referenced assets, reuse existing public-interface and component tests, and check representative documented workflows. If new tooling becomes worthwhile, integrate it durably through the existing setup/check conventions. Do not introduce infrastructure solely to enforce arbitrary word counts or formatting preferences.

This report is a dated review artifact. It should not become another maintained source of current product behavior after the migration.

## Completion criteria for the reorganization

1. The README accurately describes current capability and gives each reader a clear next link.
2. A new application author can follow one guide to a built component and validated metadata without searching architecture or reading generated wrappers.
3. Exact command, manifest, filesystem, and failure behavior has one discoverable reference home, backed by the existing schema and WIT where appropriate.
4. Contributors have a documented build/check workflow and one concise architecture explanation.
5. Product requirements state outcomes and deliberate scope; implementation details and speculative features are clearly separated.
6. Agent instructions agree with those documents and do not carry another capability history.
7. Local links resolve, important examples/workflows are checked, and no existing public guarantee disappears merely to shorten prose.

## Review checks and limits

Read all seven authored Markdown files in full, including the two repository skills. Compared their claims with the schema, WIT, example manifest and handlers, toolchain configuration, Makefile, CLI modules, and services. Inspected relevant portions of the CLI tests as supporting evidence; this was not a comprehensive code or test-quality review.

The local-link check found **54 references and no missing targets** in the original Markdown set. This report's **92 local links** also passed file and heading-target checks. The external rustup installation link resolved. The primary guidance sources cited above were opened and reviewed, including the two shared OpenAI pages. External links were not recursively crawled.

`make -n setup build install check` completed successfully and printed recipes consistent with the documented lifecycle. It did not execute those recipes. The macOS tool wrappers emitted sandbox cache/event-stream warnings during the read-only checks, but both the link-check script and Make dry run completed with exit status zero.

No fresh installation, compilation, live editor walkthrough, or test suite was run. No Rust, dependency, build-configuration, or existing documentation file was changed, so `make check` was not required for this report. Repository history, remote project material, and supported-platform claims were not inferred from Git. A future documentation migration should validate its actual first-use instructions before claiming that journey works end to end.
