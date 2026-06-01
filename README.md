# Abstract

Abstract is a small schema-backed language for defining validated data with a
pleasant authoring experience. It sits between JSON Schema and a full
programming language: strict enough to protect data contracts, compact enough to
write by hand, and readable enough to review.

```text
.abt schemas + .ab instances -> abstract compile -> human-readable RAW data
```

## Why Abstract Exists

Large content-heavy systems often drift into fragile YAML, noisy JSON, or custom
validation scripts scattered across build steps. Abstract gives that data a
single authored shape:

- schemas describe the allowed structure and vocabulary;
- instances declare concrete records;
- the compiler validates everything and emits canonical RAW data.

## Quick Start

```powershell
cargo test
cargo run -- compile example
cargo run -- lint example
```

After installing the CLI:

```powershell
abstract path\to\object.ab path\to\Template.abt JSON
abstract path\to\object.ab path\to\Template.abt YML true
abstract lint path\to\project
abstract templates path\to\project
```

## Project Layout

```text
project/
  Cargo.toml
  src/                     Rust compiler core and CLI
  tests/                   Compiler behavior tests
  example/                 Generic Abstract project
  docs/                    Language documentation
  editors/                 VSCode and Antigravity support
  site/                    Static documentation website
```

## Current V0 Features

- `schema Name { ... }` templates.
- Scalar fields with `text`, `int`, `enum`, `file`, and schema references.
- List fields using `field[]`.
- Nested groups using `group { ... }`.
- Defaults using `= value`.
- Optional fields using `@optional`.
- Tag shorthand using `@tag` and `#value(...)`.
- Instances using `Template :: @field.value`.
- Dotted assignment paths like `owner.team`.
- Multi-path assignment like `slots.{3, 4}`.
- Tuple arrays like `copy(key, value): (...)`.
- Enum prefix expansion like `es_*`.
- Full-instance cloning with `&atlas.*`.
- Root variable interpolation in paths and strings, such as `./assets/$id.png`.
- Template `logic` with `derive`, `require`, `if`, `for`, `length`, `contains`, and `exists`.
- JSON or YAML output through `abstract compile`.

## Documentation

- `docs/abstract-language.md`: complete language guide.
- `docs/technical-reference.md`: junior-friendly technical reference for `.ab` and `.abt`.
- `docs/raw-data.md`: RAW output contract.
- `docs/ai-primer.md`: compact guide for AI collaborators.
- `site/index.html`: public documentation website.
