# Abstract

Abstract is a small schema-backed language for defining validated data with a
pleasant authoring experience. It sits between JSON Schema and a full
programming language: strict enough to protect data contracts, compact enough
to write by hand, and readable enough to review.

```text
.abt schemas + .ab instances -> abstract compile -> RAW / JSON / YAML
                             -> abstract bundle  -> sealed .abx for Java
```

## Why Abstract Exists

Large content-heavy systems often drift into fragile YAML, noisy JSON, or
custom validation scripts scattered across build steps. Abstract gives that
data a single authored shape:

- schemas describe the allowed structure and vocabulary;
- instances declare concrete records with template-instance reuse (clones);
- the compiler validates everything (types, ranges, enums, real files, real
  image headers, business logic) and emits canonical data;
- bundles seal the compiled data for shipping inside applications.

## Install

**Windows (prebuilt):** download `site/downloads/abstract-windows-x64.exe`,
then:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prebuilt abstract-windows-x64.exe
```

**Any platform (from source, needs [Rust](https://rustup.rs)):**

```powershell
# Windows
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

```sh
# Linux / macOS
sh scripts/install.sh
```

Verify with `abstract --version`.

## Quick Start

```powershell
abstract init my-pack          # scaffold a working project
abstract compile my-pack JSON  # validate + emit JSON
abstract lint my-pack          # validate only
abstract templates my-pack     # list schema names
```

Ship data to a Java application:

```powershell
abstract bundle my-pack --key "release passphrase" --out my-pack.abx
```

Load it with the zero-dependency Java runtime in `java/` (Java 8+). See
`java/README.md` and `java/SECURITY.md`.

## Project Layout

```text
project/
  Cargo.toml
  src/                     Rust compiler core, CLI, media probing, crypto
  tests/                   Compiler behavior tests
  example/                 Generic Abstract project
  docs/                    Language documentation (markdown)
  editors/                 VSCode and Antigravity support
  java/                    Java runtime for sealed .abx bundles
  scripts/                 install.ps1 / install.sh
  site/                    Static documentation website
```

## Language Overview (v0.2)

- `schema Name { ... }` templates with `text`, `int`, `float`, `bool`,
  `enum`, `file`, `image`, and `$(SchemaRef)` fields.
- `image(png 128x128)` validates real image headers (PNG, JPG, GIF, BMP,
  WebP) reading only the first bytes of each file: format, dimensions, and
  extension-vs-content mismatches are caught at compile time.
- List fields (`field[]`), nested groups, defaults (`= value`),
  `@optional`, tag shorthand (`@tag`, `#value(...)`).
- Instances: `Template :: @field.value` headers, bare boolean flags
  (`@featured`), dotted paths, multi-paths (`slots.{3, 4}`), tuple arrays,
  brace patterns, enum wildcards (`es_*`).
- Cloning: `&other.*` (full), `&other.field.path` (partial), multiple clones
  merge in order.
- Interpolation: `$field`, `${field}`, `$$` escape.
- Logic: `derive`, `derive?` (only when missing), `require ... else throw`,
  `if / else if / else`, `not` and `!`, `for` loops, `exists`, `contains`,
  `length`, comparisons, `and` / `or`.
- Strict validation: unknown fields are rejected with "did you mean"
  suggestions; duplicate ids and schemas are compile errors.
- Outputs: JSON, YAML, RAW, and sealed `.abx` bundles (ChaCha20-Poly1305).

`NEW-FEATURES.md` documents everything added in v0.2 with examples.

## Documentation

- `site/index.html`: the full documentation website (start here).
- `docs/abstract-language.md`: complete language guide.
- `docs/technical-reference.md`: junior-friendly reference for `.ab`/`.abt`.
- `docs/raw-data.md`: RAW output contract.
- `docs/ai-primer.md`: compact guide for AI collaborators.
- `java/README.md` + `java/SECURITY.md`: Java integration and threat model.

## Development

```powershell
cargo test               # 66 tests: compiler, media probes, RFC 8439 vectors
cargo run -- compile example
cargo build --release    # produces target/release/abstract(.exe)
```
