<p align="center"><img src="editors/vscode/icons/abstract-logo.png" width="96" alt="Abstract"></p>

# Abstract

[Español](README.es.md)

**Define your objects. Write their variations.**

Abstract is a data language for families of objects: their shared structure,
their individual values, and the rules each variation must satisfy. Define a
schema once, write compact instances, and compile the project into validated
JSON, YAML or RAW. Your application decides what those objects do.

[Learn by doing](docs/learn/README.md) · [Language guide](docs/abstract-language.md) ·
[Abstract and CUE](docs/comparison/README.md) · [Downloads](https://github.com/antonio-in-stem/abstract/releases)

```text
.abt  definitions and rules
.ab   instances and variations  →  abstract compile  →  JSON | YAML | RAW
assets/ referenced files
```

## One definition, several objects

`data/templates/Product.abt` defines what a product can be:

```abstract
schema Product {
    title: text(1..60)
    status: enum(draft, active, retired) = draft
    owner {
        team: text(1..50)
        contact: text(1..80)
    }
    capabilities[] {
        id: enum(search, sync, export) @tag
        availability: enum(alpha, beta, stable) = stable
    }
}
```

An instance supplies its values. A variation reuses those values and writes its
differences, while retaining its own identity:

```abstract
Product :: @id.atlas, @status.active
    title: Atlas Search
    owner.team: Knowledge Systems
    owner.contact: systems@example.com
    capabilities: [#search, #sync(availability: beta)]

Product :: @id.beacon
    &atlas.*
    title: Beacon Export
```

This is a working [example project](docs/examples/clones). Reuse does not bypass
validation: both objects must satisfy the same schema.

## What belongs in the model

- **Structure:** nested schemas, bounded lists, enums, defaults and references.
- **Variations:** clone a whole instance or a subtree, then write the differences.
- **Rules:** require relationships between values and derive fields with checked
  [arithmetic](docs/ARITHMETIC.md), including list aggregation.
- **Resources:** require local files or images of a declared format and size.
- **Versions:** compile a current data set and overlays that reconstruct earlier
  model versions from one authored project.

For example, `icon: image(png 128x128)` makes the resource requirement part of the
schema. With asset checks enabled, the compiler checks existence, permitted
paths, format headers and dimensions. It does not decode all pixels or establish
that an image is safe for every downstream decoder. A `file(json)` declaration
checks a file resource; it is not a schema validator for that file's JSON content.

The ordinary output contains resource paths, not their bytes. See the
[compiled data contract](docs/raw-data.md) before writing a consumer.

## Try it

Download a compiler and the VS Code extension from
[Releases](https://github.com/antonio-in-stem/abstract/releases), or build the
compiler with Rust:

```sh
cargo build --release --locked
cargo run -- init my-project
cargo run -- compile my-project JSON --out my-project.json
```

The release executable is `target/release/abstract` (`abstract.exe` on Windows).
The Rust compiler has no external crate dependencies.

Start with [six small exercises](docs/learn/README.md), each with a prompt, hint,
starter and separately explained solution. Then try the
[intermediate library and advanced automotive examples](examples/README.md).

## Visual Studio Code

Install `abstract-language-1.7.2.vsix` using **Extensions: Install from VSIX**.
Set `abstract.compilerPath` to your compiler if `abstract` is not on PATH.

The extension provides contextual completion, syntax explanations on hover,
live compiler diagnostics, evaluated values, navigation and checked renaming.
File icons and the Orbit Dark theme are optional. See the
[extension guide](editors/vscode/README.md) for setup and supported operations.

## Is Abstract the right fit?

Abstract is useful when many objects share rules but differ in their details.
It gives object identity, cloning, resource constraints and model versions an
explicit place in the authoring workflow. A small configuration file without
shared rules may not justify adding a compiler.

CUE also combines data and constraints and supports reusable definitions.
Abstract's clone-and-replace model is different from constraint unification;
neither is a universal replacement for the other. The
[executable comparison](docs/comparison/README.md) models the same problem in
both languages and checks their results.

## Project and release status

This repository contains the language specification, Rust compiler, Java output
reader, VS Code extension, examples and documentation. Compiler **1.4.0**,
language **1.2**, and extension **1.7.2** have separate versions.

Release evidence and remaining limits are recorded in
[the release checklist](docs/RELEASE.md). Automated test results are not a claim
that every platform or every possible input has been independently verified.

| Directory | Contents |
| --- | --- |
| `src/`, `tests/` | Compiler and conformance tests |
| `editors/vscode/` | Editor extension and its tests |
| `examples/`, `docs/examples/` | Exercises and runnable language examples |
| `docs/` | Guides, specification and output contracts |
| `java/` | Compiled-output reader |

See [contributing](CONTRIBUTING.md) for verification commands and
[the changelog](CHANGELOG.md) for compatibility changes.

## License

By **Antonio M.** Source code is available under the [MIT license](LICENSE).
The Abstract name and logo remain the property of Antonio M.
