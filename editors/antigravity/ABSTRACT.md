# Abstract Editor Notes

Abstract files use:

- `.abt` for schemas.
- `.ab` for instances.

The extension provides syntax highlighting, lint diagnostics, snippets,
completion hints, semantic coloring, and file icons. Diagnostics are delegated
to the configured `abstract` binary.

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
  "abstract.compilerPath": "C:\\\\Users\\\\PC\\\\.cargo\\\\bin\\\\abstract.exe",
  "abstract.projectPath": "${workspaceFolder}\\\\.abstract\\\\project\\\\example"
}
```

## AI Collaboration

When an AI edits Abstract, point it to:

- `docs/abstract-language.md`
- `docs/raw-data.md`
- `docs/ai-primer.md`

Ask it to run:

```powershell
abstract lint path\to\project
abstract path\to\object.ab path\to\Template.abt JSON
abstract path\to\object.ab path\to\Template.abt YML true
```
