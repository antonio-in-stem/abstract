# Abstract Language for VS Code

Editor support for the Abstract 1.x language in m-project. The extension version
is independent of the compiler version: this release targets the language and
CLI in Abstract **1.0.1**. Install the compiler separately and set
`abstract.compilerPath` if `abstract` is not on your PATH.

## Authoring

- Syntax and semantic highlighting for `.ab` instances and `.abt` templates,
  plus optional Abstract Orbit Dark and file icon themes.
- Contextual completion from the project's schemas: root fields, dotted paths,
  body blocks, scalar enum/boolean values, `ref(Schema)` instance IDs, header
  tags, enum `#tag` members, schema names and type/modifier snippets.
- Go to Definition and hover for instance schemas, assigned fields, nested
  `$(Schema)` fields and unquoted reference values. The Outline lists schemas,
  fields and instance IDs.
- **Refactor → Rewrite → Use a dotted path** turns a single-assignment body
  block into its equivalent dotted assignment. The edit is previewable and
  undoable. Comments attached to the assignment and version annotations stay
  intact; blocks with comments on their braces are left as written.
- Compiler diagnostics on opening and saving files, including error codes and
  related locations. Errors stay associated with their project in multi-root
  workspaces. Compile and Lint commands write compiler output to the Abstract
  output channel.

Completions, navigation and refactoring use current editor buffers, including
unsaved schemas. The compiler checks **saved project files**. Editing a project
clears its previous diagnostics until its Abstract buffers are saved; this avoids
showing errors from an older disk version as if they described the current text.
Compiler commands require saved buffers and a trusted workspace. Other editing
features remain available in Restricted Mode.

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

This release is an authoring increment, not complete language-server coverage.
It does not provide compiler diagnostics for unsaved buffers, whole-document
formatting, rename/references, tuple-column or tagged-argument completions,
asset-path completion, loop-variable inference, or general compression.
Hover displays a resolved declaration; it does not evaluate defaults, logic or
version applicability. Completion proposals are not a proof that the whole
document is valid. Ambiguous schema declarations yield no assumed field shape.

The dotted-path action covers one existing SPEC 5.4 equivalence. It does not
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
npm run benchmark
npm run package
```

`npm test` includes compiler-backed byte comparisons, so it requires that build
or `ABSTRACT_COMPILER_PATH`. `npm run test:integration` uses an isolated VS Code
profile and temporary project. By default it downloads the declared minimum
VS Code 1.92.2; `ABSTRACT_VSCODE_PATH` can select an existing VS Code executable.
It does not install the extension into your regular profile. The package command
produces `abstract-language-1.1.0.vsix`; use **Extensions: Install from VSIX** to
install it.

The implementation uses the official [VS Code language-provider APIs](https://code.visualstudio.com/api/language-extensions/programmatic-language-features),
[extension-host testing API](https://code.visualstudio.com/api/working-with-extensions/testing-extension)
and [Workspace Trust API](https://code.visualstudio.com/api/extension-guides/workspace-trust).
