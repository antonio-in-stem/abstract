# Abstract Editor Notes

Abstract files use:

- `.abt` for schemas.
- `.ab` for instances.

The extension provides syntax highlighting, lint diagnostics, snippets,
completion hints, semantic coloring, and file icons. Diagnostics are delegated
to the configured `abstract` binary, which is always invoked with an argument
vector and never through a shell.

The bundled `Abstract Orbit Dark` theme gives `.abt` files two clear palettes:
schema keywords/types/decorators use a cool blue-cyan family, while logic
keywords/operators/paths use a warmer pink-gold family. Other themes still get
the richer TextMate scopes and semantic tokens when they support them.

## Recommended Project Settings

```json
{
  "files.associations": {
    "*.ab": "abstract",
    "*.abt": "abstract"
  },
  "workbench.iconTheme": "abstract-file-icons",
  "workbench.colorTheme": "Abstract Orbit Dark",
  "abstract.compilerPath": "abstract",
  "abstract.projectPath": ""
}
```

`abstract.compilerPath` may be a bare command name found on PATH or an
absolute path to the binary.

Leaving `abstract.projectPath` empty is the normal setting. The extension then
resolves the project the way the compiler does (SPEC §2.3): it walks up from
the open file for the nearest ancestor directory named `data`, and lints that
directory's parent. It never lints a single `.ab` file out of project context,
because a file separated from its schemas produces nothing but noise. Set the
option only to pin a project explicitly; `${workspaceFolder}` is expanded.

## AI Collaboration

When an AI edits Abstract, point it at the normative definition of the
language:

- `docs/SPEC.md` — the specification. Where code and specification disagree,
  the specification wins.
- `docs/GRAMMAR.ebnf` — the grammar the specification cites.

Ask it to run:

```powershell
abstract lint path\to\project
abstract compile path\to\project JSON
abstract compile path\to\project YML --out path\to\out.yml
```

A command word is always required. There is no direct form naming an instance,
a template and a format as three positionals; that spelling was removed in 1.0
(SPEC §9.1, Appendix A).
