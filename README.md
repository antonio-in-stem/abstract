# Abstract

Abstract is a small schema-backed data language. You describe the shape of your
data once, author it in a compact syntax, and compile it into a canonical JSON,
YAML or RAW document that every other tool reads.

```text
.abt schemas + .ab instances  ->  abstract compile  ->  JSON | YAML | RAW
```

Version 1.0.0 is the first stable definition of the language. It is specified
byte for byte in [`docs/SPEC.md`](docs/SPEC.md), with the complete grammar in
[`docs/GRAMMAR.ebnf`](docs/GRAMMAR.ebnf). The compiler is written in Rust with
**zero dependencies**.

---

## Why

Structured data starts as a few neat config files and then grows rules: ids must
be unique, values must come from a vocabulary, translations must cover the same
keys, asset paths must point at files that exist and have the right dimensions.
Abstract makes those rules part of the authoring language instead of leaving
them in review comments and build scripts.

- **Schemas** describe the allowed structure, types, ranges and vocabulary.
- **Instances** declare concrete records, with reuse by cloning.
- **Logic** attaches rules that depend on relationships between fields.
- **The compiler** validates everything — types, ranges, enums, references, real
  files, real image headers — and emits canonical data.

Four principles decide every rule: minimal, elegant, deterministic, strict. The
same input bytes produce the same output bytes on every machine, and every
construct is either defined or an error with a stable identifier and a source
position.

---

## Install

**Windows, prebuilt.** Download
[`site/downloads/abstract-windows-x64.exe`](site/downloads/abstract-windows-x64.exe),
then, from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prebuilt abstract-windows-x64.exe
```

The SHA-256 of the published binary is recorded in
[`site/downloads/SHA256SUMS.txt`](site/downloads/SHA256SUMS.txt).

**Any platform, from source.** Needs [Rust](https://rustup.rs) and nothing else.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

```sh
sh scripts/install.sh
```

Verify:

```sh
abstract --version
```

---

## Quick start

```sh
abstract init m-project          # scaffold a project that compiles
abstract compile m-project JSON  # validate and emit JSON to stdout
abstract compile m-project YML   # or YAML
abstract compile m-project RAW   # or RAW, the review format
abstract lint m-project          # validate only; prints 'abstract: ok'
abstract templates m-project     # list the schema names
```

Write the document to a file instead of stdout:

```sh
abstract compile m-project JSON --out m-project.json
```

A first project is two files. `data/templates/Label.abt`:

```abstract
schema Label {
    caption: text(1..40)
}
```

`data/labels/hello.ab`:

```abstract
Label :: @id.hello
    caption: "Hello, world"
```

```sh
abstract compile m-project JSON
```

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [
    {
      "template": "Label",
      "id": "hello",
      "caption": "Hello, world"
    }
  ],
  "overlays": []
}
```

This project is checked in as [`docs/examples/hello`](docs/examples/hello),
together with the exact bytes above.

---

## A taste of the language

```abstract
versions 1..2

schema Pack {
    title: text(1..40)
}

schema Product {
    title: text(1..60)
    status: enum(draft, active, retired) = draft
    price: float(0..9999)
    icon: image(png 128x128)
    pack: ref(Pack)
    glow: bool @since(2) = false
    owner {
        team: text(1..50)
        contact: text(1..80) @optional
    }
    capabilities[] {
        id: enum(search, sync, export) @tag
        availability: enum(alpha, beta, stable) = stable
    }
}

logic Product {
    require .status != "retired" or not .capabilities.id contains "search"
        else throw "A retired product cannot advertise search."
}
```

```abstract
Product :: @id.atlas, @status.active
    title: Atlas Search
    price: 19.5
    icon: ./textures/$id.png
    pack: core
    owner.team: Knowledge Systems
    capabilities: [#search, #export(availability: alpha)]
```

Nine types (`text`, `int`, `float`, `bool`, `enum`, `file`, `image`, `ref`,
`$(Schema)`), groups and list groups, defaults, `@optional`, list cardinality,
tagged shorthand, tuple arrays, enum wildcards, brace file patterns,
interpolation, cloning, logic, and one project carrying every version of its
data.

That project is checked in as
[`docs/examples/readme-tour`](docs/examples/readme-tour), with the document it
compiles to — including the `overlays` entry for version 1, where `glow` does
not yet exist.

---

## Project layout

```text
abstract/                  the repository root
  Cargo.toml               the abstract-lang crate; zero dependencies
  src/                     compiler core and CLI
  tests/                   compiler behaviour tests
    conformance/           the conformance corpus; cases/ holds the case trees
  docs/
    SPEC.md                the normative specification
    GRAMMAR.ebnf           the normative grammar
    abstract-language.md   the language guide
    technical-reference.md the lookup reference
    raw-data.md            the compiled-document contract for consumers
    ai-primer.md           writing correct Abstract, for assistants and agents
    ai/README.md           working inside this repository
    examples/              every documented example, as a compilable project
  example/                 the scaffold `abstract init` produces
  editors/                 VS Code and Antigravity support
  java/                    optional Java runtime for sealed containers
  scripts/                 install.ps1, install.sh
  site/                    the static documentation website
  CHANGELOG.md             every change in 1.0.0, with before and after
```

A **project** you author looks like this:

```text
m-project/            <- project root
  assets/
    textures/atlas.png
  data/               <- data directory, and the root of source discovery
    templates/Product.abt
    products/atlas.ab
```

The compiler finds the project by walking up from the path you name until a
directory called `data` appears; the project root is that directory's parent,
and assets live in `<project root>/assets`. A project may also keep its sources
in one plain directory with no `data/` at all.

---

## Documentation

Start here:

- [`site/index.html`](site/index.html) — the documentation website.
- [`docs/abstract-language.md`](docs/abstract-language.md) — the language guide.

Then:

- [`docs/SPEC.md`](docs/SPEC.md) — the normative specification, including the
  diagnostics catalogue and a full worked example.
- [`docs/GRAMMAR.ebnf`](docs/GRAMMAR.ebnf) — the complete grammar.
- [`docs/technical-reference.md`](docs/technical-reference.md) — a lookup table
  for editing `.ab` and `.abt` files.
- [`docs/raw-data.md`](docs/raw-data.md) — what consumers of the compiled
  document may rely on, including the overlay contract.
- [`docs/ai-primer.md`](docs/ai-primer.md) — rules, pitfalls, reserved words and
  a checklist for assistants and agents writing Abstract.
- [`docs/examples/README.md`](docs/examples/README.md) — every documented
  example as a project you can compile.
- [`CHANGELOG.md`](CHANGELOG.md) — every 1.0.0 change with before and after, and
  the migration checklist.

---

## Optional tooling

These ship with the reference implementation, operate on an already-compiled
document, and are not part of the language. A conforming Abstract
implementation is not required to provide them (SPEC 9.9, Appendix D).

- `abstract bundle` / `abstract unbundle` — sealed `.abx` containers for
  shipping compiled data inside an application.
- [`java/`](java/README.md) — a zero-dependency Java runtime that reads a
  compiled document or a sealed container, including the version axis
  (`forVersion(int)`). See [`java/SECURITY.md`](java/SECURITY.md) for the threat
  model.
- [`editors/`](editors) — VS Code and Antigravity support: syntax, icons and
  on-save linting.

---

## Development

```sh
cargo test                       # compiler behaviour tests
cargo run -- compile example     # compile the scaffold project
cargo build --release            # target/release/abstract(.exe)
```

Every documented example is a project under `docs/examples/`, stored with the
exact bytes the compiler emits, so a documentation change can be checked
mechanically:

```sh
abstract lint docs/examples/hello
abstract compile docs/examples/hello JSON
```

Conformance requirements, the golden-test directory format and the required
coverage are specified in SPEC section 11. The corpus itself is checked in
under [`tests/conformance/`](tests/conformance): `cases/<area>/<id>/` holds
the case trees `cargo test` runs, and the notes beside it record the index,
the corrections made to expectations and the defects each case pins.

---

## License

MIT. See [`LICENSE`](LICENSE).
