# Abstract AI Primer

Use this document when an AI collaborator needs to read or write Abstract.

## Mental Model

Abstract has two sides:

1. `.abt` schemas define classes and validation.
2. `.ab` files instantiate those classes.

The compiler emits validated JSON or YAML. Those files are build artifacts, not
the preferred hand-authored source.

## Authoring Rules

- Prefer concise header tags for short required fields; a bare `@flag` means
  `true`.
- Put nested, repeated, or long fields below the header.
- Use enums for business vocabularies.
- Use `@tag` when a group has a natural shorthand.
- Use `#value(...)` only when the target schema has a tagged field.
- Use wildcard expansion only when the schema enum contains matching values.
- Keep instance ids stable and lowercase snake case; ids must be unique.
- Respect types: `9.5` compiles to a float, `true` to a bool, `1.21.5` stays
  text. Never assign fields the schema does not declare; strict mode rejects
  them with a suggestion.
- Do not treat compiled JSON/YAML as hand-authored source; regenerate it from
  `.ab`.

## Generic Instance Pattern

```abstract
Product :: @id.atlas, @status.active, @tier.premium, @release_year.2026
    name: Atlas Search
    tags: [core, public, ai_ready]
    owner.team: Knowledge Systems
    owner.contact: systems@example.com
    capabilities: [#search, #sync(availability: beta), #analytics]
```

## Compiler Commands

From the public project directory:

```powershell
cargo run -- compile example JSON
cargo run -- lint example
cargo test
```

After installing or downloading the binary:

```powershell
abstract init my-pack
abstract path\to\object.ab path\to\Template.abt JSON
abstract path\to\object.ab path\to\Template.abt YML true
abstract compile path\to\project JSON
abstract compile path\to\project RAW --skip-assets
abstract lint path\to\project
abstract templates path\to\project
abstract bundle path\to\project --key "passphrase" --out data.abx
abstract unbundle data.abx --key "passphrase"
```

## Review Checklist

When editing Abstract:

- Does every instance target an existing schema?
- Are enum values valid after snake-case normalization?
- Are list fields written as arrays when there is only one item?
- Are generated JSON/YAML files excluded from hand edits?
- Does the source remain more readable than the equivalent JSON/YAML?

## Current Caveats

- `logic` blocks can derive values with `derive .path = value` (and
  `derive? .path = value` for author-overridable defaults), branch with
  `if / else if / else`, negate with `not` or `!`, loop, and reject invalid
  instances with `require ... else throw`.
- `image(...)` fields are validated against real file headers; compile with
  `--skip-assets` on machines without the binary assets.
- The editor extension delegates linting to the `abstract` binary.
- JSON/YAML output is intentionally standard so downstream Java/Web parsers do
  not need a custom artifact parser; `.abx` bundles wrap the same JSON in an
  encrypted container (see `java/SECURITY.md`). Never write bundle keys into
  source files.
