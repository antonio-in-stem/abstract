# Abstract extension authoring reference

This reference covers the detailed behavior and operating limits of the Abstract
extension for Visual Studio Code. For the language itself, start with the
[language guide](../../../docs/abstract-language.md).

## Project model

The default project layout contains a `data/` directory with `.abt` definitions
and `.ab` instances at any depth. Without a `data/` marker, the extension uses the
open file's directory when it contains a definition. Set `abstract.projectPath`
for another layout. Relative paths resolve against the owning workspace folder;
`${workspaceFolder}` and `${workspaceFolder:name}` are supported.

Project discovery is case-insensitive for source extensions and the `data`
marker. It ignores dot, vendor and build directories and de-duplicates linked
files by canonical path. Saved source events invalidate the disk cache. A
configured project outside the workspace refreshes on the next request after at
most two seconds.

| Setting | Default | Purpose |
| --- | --- | --- |
| `abstract.compilerPath` | `abstract` | Compiler executable used for analysis and commands |
| `abstract.projectPath` | empty | Optional project root or `data` directory |
| `abstract.outputFormat` | `JSON` | Compile output: JSON, YML, YAML or RAW |

## Editing features

The extension provides syntax and semantic highlighting for `.ab` and `.abt`
files, contextual syntax help, whole-document indentation formatting, an Outline,
and completion for schema fields, dotted paths, values, references, tags, tuples,
assets, schema names and language constructs. These features tolerate incomplete
input so they can assist while a document is being written. The compiler remains
authoritative.

Completions and navigation use current editor buffers, including unsaved
definitions. If another `files.associations` rule claims `.ab` or `.abt`, run
**Abstract: Use for .ab and .abt in This Workspace**. **Abstract: Show Diagnostic
Status** explains language, workspace-trust, project and compiler readiness.

**Refactor → Rewrite → Flatten body block to dotted paths** converts direct body
assignments into equivalent dotted assignments. The previewable edit preserves
tokens, strings, comments, line endings and version annotations. Blocks with
comments on their braces remain unchanged.

Go to Definition, Find All References and Rename Symbol understand schemas,
fields, explicit instance IDs and local loop variables. Rename uses compiler
identities, includes unsaved buffers, rejects collisions and validates the
proposed edit before showing it. File-stem instance IDs can participate in
navigation and references, but renaming them requires an explicit `@id` first.
Semantic references and rename require the compiler capability
`schemaBindings: 1`.

## Compiler-backed analysis

A compiler advertising analysis protocol 1 validates dirty file-backed buffers
after a 250 ms debounce. It receives a bounded snapshot of the real project and
assets directory without the extension writing source files. Replaced requests
are canceled and stale responses are discarded. The
[analysis protocol](../../../docs/ANALYSIS-PROTOCOL.md) defines the request limits
and Unicode ranges.

Older compilers remain usable. The status bar labels their results **Saved-file
diagnostics**, and analysis waits until every project buffer is saved before
running legacy `lint`. Compile also requires saved buffers. Unsupported analysis
is never presented as live validation.

Evaluated-value hover requires a valid whole-project snapshot and the optional
`values: 1` capability. If the project is invalid or that capability is absent,
hover still shows the resolved declaration and its declared metadata. A completion
proposal does not establish that the whole document is valid.

## Public fields

`@public` field nominations have contextual completion, highlighting and hover
help. **Abstract: Inspect Public Fields** analyzes the current project, including
unsaved file-backed sources. It lists nominations, unused declarations and
resolved values with version windows. Selecting an entry opens its declaration
or root instance.

The command requires `publicInventory: 1`. An older compiler does not fall back
to saved files for this operation. Inspection reports author-declared public data;
it does not install an override, authorize a consumer or change compiled output.
See the [public data contract](../../../docs/PUBLIC-CONTRACT.md).

## Workspace trust and source boundaries

Compiler analysis, compilation, public-field inspection, semantic references and
rename require a trusted workspace. Static completion, syntax help, navigation
from the tolerant index and the dotted-path action remain available in Restricted
Mode.

Schema operations require one valid compiler snapshot and stay within one
discovered project. Rename refuses edits to linked canonical files outside that
root, while Find References may still show their occurrences. Multiple open URI
aliases of a file must be closed before Rename. The extension checks source
snapshots before returning an edit; later editor changes are outside that checked
snapshot.

Semantic operations admit at most 1,024 canonical sources, 4 MiB per source and
16 MiB of aggregate UTF-8 source text. Discovery rejects scans above 32,768
entries, 4,096 directories or 128 levels. The compiler protocol also limits a
request to 128 overlays. These are capture limits for editor operations, not
compiler heap limits.

Abstract 1.x has no import syntax, so project discovery defines the reference
graph. Renaming an emitted schema changes its `template` identity and can require
updates in downstream readers.

## Build and verify

Build the compiler from the repository root, then the extension:

```sh
cargo build --bin abstract
cd editors/vscode
npm ci
npm test
npm run test:integration
npm run test:integration:public
npm run benchmark
npm run package
```

`npm test` includes compiler-backed byte comparisons and therefore needs the
compiler build or `ABSTRACT_COMPILER_PATH`. Integration tests use isolated VS
Code profiles and temporary projects. `ABSTRACT_VSCODE_PATH` can select an
existing VS Code executable; `ABSTRACT_TEST_VSCODE_VERSION` selects a released
test version.

Set `ABSTRACT_TEST_RESTRICTED=1` to exercise Restricted Mode. Set
`ABSTRACT_LEGACY_COMPILER_PATH` to a preserved pre-protocol executable to test
saved-file compatibility. Public integration tests require
`ABSTRACT_PUBLIC_OLD_COMPILER` to point to an actual older compiler without the
public inventory capability.

For compiler-analysis performance, build the release compiler and run:

```sh
cargo build --release --bin abstract
cd editors/vscode
npm run benchmark:analysis
```

The benchmark separates client-side project discovery and encoding from a new
compiler process and includes asset checks. The
[validation ledger](../../../docs/ai/vscode-analysis-validation.md) records its
fixture, environment, distributions and limits.

The extension bundles no compiler binary, background server or Node runtime
dependency.
