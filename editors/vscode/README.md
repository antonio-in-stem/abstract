# Abstract Language for VS Code

Version 1.7.0 adds contextual completion, hover help, and highlighting for
Abstract 1.2 explicit `calc(...)` arithmetic. Only glued lowercase `calc(` in a
derive right-hand side or condition selects arithmetic; an older unquoted literal
with that spelling must be quoted when migrating. Version 1.6.1 fills in hover help for defaults and constraints, and distinguishes
instance keys from bare values, including tuple columns and cells.
Version 1.6.0 added offline, contextual syntax hover: language constructs explain
their purpose and show a valid Abstract example without requiring a compiler or
saved project. Version 1.5.0 added the planned authoring features: formatting,
compiler-bound rename/references, tuple, tagged-object and asset completion,
loop element inference, and broader safe dotted-path rewriting. The optional
file icon theme distinguishes `.ab` data files from `.abt` template files.
Version 1.4.0 added **Abstract: Inspect Public Fields**: a compiler-backed
inventory with unsaved-source support, export diagnostics and declaration navigation.
Nomination highlighting, completion, field hover, semantic references and rename remain available.

The `.ab` glyph is a diamond with a round cutout. The `.abt` glyph uses three
stacked chevrons so data and template files remain distinct at 16 pixels.

The extension is authored by Antonio M. The local VSIX uses
`antonio-in-stem.abstract-language`, matching the owner's repository namespace;
this does not claim that a Visual Studio Marketplace publisher account exists.
The visual assets are extension UI artwork under [LICENSE.txt](https://github.com/antonio-in-stem/abstract/blob/main/editors/vscode/LICENSE.txt).

Editor support for the Abstract 1.x language in m-project. The extension version
is independent of the compiler version: this release targets the language and
CLI in Abstract **1.2.0**, plus its optional analysis protocol when advertised.
Install the compiler separately and set
`abstract.compilerPath` if `abstract` is not on your PATH.

## Authoring

- `@public` field nominations have contextual completion, highlighting and a
  hover explanation. They require compiler 1.1 or newer. Completion is static
  authoring assistance; dependency admission belongs to the separate
  [public compilation command](https://github.com/antonio-in-stem/abstract/blob/antonio-in-stem/abstract-vscode-intelligence/docs/PUBLIC-CONTRACT.md).
- **Abstract: Inspect Public Fields** analyzes the current project, including unsaved
  file-backed sources. The native list shows nominations, unused declarations and
  resolved values/version windows; select a nomination to open its declaration or
  an occurrence to open its root instance. Public export failures appear separately
  as `abstract-public` diagnostics with related locations. This requires the negotiated
  `publicInventory: 1` capability; an older compiler does not fall back to saved files.
  Export admission remains unbound: it does not install runtime overrides or authorize
  a Covenant consumer. A rejected export never becomes a partially admitted list.
- Syntax and semantic highlighting for `.ab` instances and `.abt` templates,
  plus optional Abstract Orbit Dark and file icon themes.
- Contextual syntax hover for declarations, schema types and modifiers, logic
  statements and operators, version annotations, and instance sigils/compact
  structures. It works in untitled Abstract buffers and uses grammar position to
  avoid treating comments, quoted prose, bare values or ordinary identifiers as
  language constructs. Declaration metadata and compiler-evaluated value hovers
  remain available alongside it.
- Whole-document formatting through **Format Document**. Formatting changes
  indentation only and preserves authored tokens, strings, comments, line endings
  and compiler meaning.
- Contextual completion from the project's schemas: root fields, dotted paths,
  body blocks, scalar enum/boolean values, `ref(Schema)` instance IDs, header
  tags, enum `#tag` members and arguments, tuple columns/cells, matching asset
  paths, inferred loop element fields, schema names and type/modifier snippets.
- Go to Definition and hover for instance schemas, assigned fields, nested
  `$(Schema)` fields and unquoted reference values. Schema field hover identifies
  declared defaults and version windows. With the optional `values: 1` capability,
  instance field hover also shows compiler-evaluated values after defaults and
  logic for each base/overlay version range. The Outline lists schemas, fields
  and instance IDs.
- **Refactor → Rewrite → Flatten body block to dotted paths** turns one or more
  direct assignments into equivalent dotted assignments. The edit is previewable and
  undoable. Comments attached to the assignment and version annotations stay
  intact; blocks with comments on their braces are left as written.
- **Find All References** and **Rename Symbol** for schema names, fields, explicit
  instance IDs and local loop variables, using compiler identities across every
  declaration and resolved use. Rename includes unsaved buffers, respects field
  and loop scope, rejects collisions, preserves homonymous comments/value strings,
  and validates the proposed changes with the compiler before offering the edit.
  File-stem instance IDs navigate and participate in references but are not renamed
  as token edits; add an explicit `@id` first. This requires the optional
  `schemaBindings: 1` compiler capability; the release number alone is insufficient.
- Compiler diagnostics while editing, including error codes and
  related locations. Errors stay associated with their project in multi-root
  workspaces. The status bar distinguishes live analysis from legacy saved-file
  linting. Compile writes compiler output to the Abstract channel.
- If another `files.associations` rule opens `.ab`/`.abt` under a different
  language, the extension reports the conflict. Run **Abstract: Use for .ab and
  .abt in This Workspace** to fix only that workspace. **Abstract: Show Diagnostic
  Status** explains trust, project and compiler readiness from the status bar.

Completions, navigation and refactoring use current editor buffers, including
unsaved schemas. A compiler advertising **analysis protocol 1** also checks dirty
file-backed buffers after a 250 ms debounce, using the actual project and assets
directory without writing source files. Replaced requests are canceled and stale
results are discarded. The [protocol contract](https://github.com/antonio-in-stem/abstract/blob/antonio-in-stem/abstract-vscode-intelligence/docs/ANALYSIS-PROTOCOL.md)
defines limits and Unicode ranges.

Older compilers remain usable: the extension explicitly reports **Saved-file
diagnostics** and waits until every project buffer is saved before invoking
legacy `lint`. Unsupported analysis is never presented as live validation.
Analysis and compiler commands require a trusted workspace; Compile also needs
saved buffers. Semantic references and rename also require Workspace Trust.
Public inventory requires Workspace Trust and runs only when explicitly requested.
Closing its selector releases the captured source text. Export diagnostics retain
only bounded source identities and diagnostic text for up to 16 projects; inspecting
more projects may evict older public diagnostics. Ordinary diagnostics are separate.
Static completion, navigation and the dotted-path action remain available in
Restricted Mode.

## Project layout and settings

The default layout is a project containing `data/` with schemas and instances at
any depth. Without a `data/` marker, the open file's directory is used when it
contains a template. Set `abstract.projectPath` to the project root for other
layouts; `${workspaceFolder}` and `${workspaceFolder:name}` are supported, and
relative paths are resolved against the owning workspace folder.

Discovery follows SPEC 2.3–2.4: case-insensitive source extensions/data marker,
ignored dot/vendor/build directories and canonical-path de-duplication of links.
Saved source events invalidate the disk cache; configured projects outside the
workspace refresh on the next request after at most two seconds. The editor
index is tolerant of incomplete input; the compiler remains authoritative.

| Setting | Default | Purpose |
| --- | --- | --- |
| `abstract.compilerPath` | `abstract` | Executable used for compiler diagnostics and commands |
| `abstract.projectPath` | empty | Optional project root or data directory |
| `abstract.outputFormat` | `JSON` | Compile output: JSON, YML, YAML or RAW |

## Current boundaries

Untitled Abstract buffers show a Save As prompt and **Abstract: Show Diagnostic
Status** reports **Save required** until the file has a project identity.
Compiler-evaluated hover requires a valid whole-project snapshot and the optional
`values: 1` capability. With an older compiler or a project error, hover keeps the
resolved declaration and declared metadata without presenting a partial effective
value. Completion proposals are not a proof that the whole document is valid.
Ambiguous schema declarations yield no assumed field shape.

Schema operations require a valid compiler snapshot and reject incomplete,
truncated or stale binding results. They are scoped to one discovered project.
Rename refuses edits to linked canonical files outside its discovery root;
Find References still shows their occurrences in this project's context. Other
project roots and external consumers are not part of that reference graph.
Renaming an emitted schema changes its `template` identity, which can require
updates in downstream readers. Source snapshots are checked again before the
edit is returned, but they are not filesystem transactions.
Multiple open URI aliases of a file to edit must be closed before Rename; a
retargeted open alias also cancels the operation. The public VS Code 1.92 Rename
API does not carry our captured versions through a later F2 preview/application,
so edits made after the provider returns are outside this checked snapshot.

Semantic references/rename admit at most 1,024 canonical sources, 4 MiB per source
and 16 MiB of aggregate UTF-8 source bytes (including dirty/new buffers). Saved
and open sources are checked before reading/joining full text; open documents
use line-count and UTF-16 length preflight followed by actual UTF-8 admission.
Saved reads use handles and enforce actual byte limits even if a file grows. Discovery
rejects scans above 32,768 entries, 4,096 directories or 128 levels; it never
silently omits project sources. Candidate Rename expansion is checked before
constructing edited text. These are limits of this feature's source capture,
not compiler heap limits or restrictions newly imposed on the tolerant index.
The existing 128-overlay/request-frame limits also apply.

Compiler-bound names use the compiler's normalization, version, clone/reference,
tuple/tag, nested-schema and loop-scope rules; the tolerant editor index remains
responsible only for assistance while a document is incomplete. Abstract 1.x has
no import syntax, so project discovery defines the reference graph.

The dotted-path action flattens one or several assignments in a body block using
the SPEC 5.4 path-prefix equivalence. It does not
remove defaults, merge repeated values, introduce wildcards or rewrite clones;
those transformations need separate semantic equivalence checks. No compiler
binary, background server or Node runtime dependency is bundled.

## Build and verify

From the repository root, build the existing compiler:

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

`npm test` includes compiler-backed byte comparisons, so it requires that build
or `ABSTRACT_COMPILER_PATH`. `npm run test:integration` uses an isolated VS Code
profile and temporary project. By default it downloads the declared minimum
VS Code 1.92.0; `ABSTRACT_VSCODE_PATH` can select an existing VS Code executable.
It does not install the extension into your regular profile. The package command
produces `abstract-language-1.7.0.vsix`; use **Extensions: Install from VSIX** to
install it.

The public-inventory host tests also use a temporary profile. Set
`ABSTRACT_TEST_VSCODE_VERSION` to select a released version, and
`ABSTRACT_PUBLIC_OLD_COMPILER` to an actual previous compiler without that capability.
This compatibility fixture is required for the public host gate; it is never rebuilt
from the new sources and labelled as an old compiler.

Set `ABSTRACT_TEST_RESTRICTED=1` to run the real Restricted Mode branch. Set
`ABSTRACT_LEGACY_COMPILER_PATH` to a preserved pre-protocol executable to test
saved-file compatibility; the test reports a skip when that binary is absent.
The runner launches the official host with explicit profiles and arguments,
because the test library's `runTests` helper disables Workspace Trust.

For compiler-analysis performance, first run `cargo build --release --bin abstract`
at the repository root, then `npm run benchmark:analysis` here. It measures client
membership discovery and request encoding separately from a new compiler process,
including actual asset checks. The [analysis validation ledger](https://github.com/antonio-in-stem/abstract/blob/antonio-in-stem/abstract-vscode-intelligence/docs/ai/vscode-analysis-validation.md)
records the fixture, environment, distributions and remaining limits.

The implementation uses the official [VS Code language-provider APIs](https://code.visualstudio.com/api/language-extensions/programmatic-language-features),
[extension-host testing API](https://code.visualstudio.com/api/working-with-extensions/testing-extension)
and [Workspace Trust API](https://code.visualstudio.com/api/extension-guides/workspace-trust).
