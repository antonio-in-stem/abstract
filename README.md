<p align="center"><img src="vscode/icons/abstract-logo.png" width="96" alt="Abstract"></p>

# Abstract

**Define objects, express their variations, and compile checked data.**

Abstract is a data language for related objects. Define a schema once, write
instances and variations, then compile validated JSON, YAML, or RAW for the
application that uses the data.

[Learn](docs/learn/README.md) · [Language guide](docs/language.md) ·
[Reference](docs/README.md) · [Releases](https://github.com/antonio-in-stem/abstract/releases)

```text
.abt  definitions and rules
.ab   instances and variations  →  abstract compile  →  JSON | YAML | RAW
assets/ referenced files
```

## A schema, then variations

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

Product :: @id.atlas, @status.active
    title: Atlas Search
    owner.team: Knowledge Systems
    owner.contact: systems@example.com
    capabilities: [#search, #sync(availability: beta)]

Product :: @id.beacon
    &atlas.*
    title: Beacon Export
```

The second instance reuses the first and writes only its differences. Both must
satisfy the same schema. Schemas can also declare lists, defaults, references,
resource constraints, logic, arithmetic, and model `versions`.

## Try it

Download a release and put `abstract` on PATH, then create and compile a
project:

```sh
abstract init my-project
abstract compile my-project JSON --out my-project.json
```

To build from this checkout:

```sh
cargo build --release --locked
cargo run -- init my-project
cargo run -- compile my-project JSON --out my-project.json
```

Start with the [guided exercises](docs/learn/README.md), then use the
[tutorials](examples/README.md) and [feature examples](examples/features/README.md).
For AI-assisted authoring, use the [AI guide](docs/ai/README.md).

## Tooling

The VS Code extension provides completion, diagnostics, evaluated values,
navigation, and checked renaming. Follow the [extension guide](vscode/README.md)
for installation and setup.

The Java reader is optional. It reads compiled JSON and can reconstruct a model
version from overlays. See [its guide](java/README.md).

## Project layout

| Directory | Contents |
| --- | --- |
| `src/`, `tests/` | Compiler and conformance tests |
| `vscode/` | VS Code extension |
| `examples/` | Tutorials, feature examples, and exercises |
| `docs/` | Learning material and language reference |
| `java/` | Optional compiled-data reader |

See [contributing](CONTRIBUTING.md), [release guidance](docs/releasing.md), and
the [release manifest](release-manifest.json) for current component values.

## License

Source code is available under the [MIT license](LICENSE). The Abstract name
and logo remain the property of Antonio M.
