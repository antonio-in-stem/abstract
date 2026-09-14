# Documentation examples

Every worked example printed in the Abstract documentation — every place a
document shows source together with the compiled output it produces — is stored
here as a complete, compilable project, so it can be checked mechanically
instead of by eye.

Short fragments that illustrate one rule in isolation, such as a single field
declaration or a two-line comparison of a right and a wrong spelling, are not
separate projects; they are excerpts of these projects or of the specification's
own examples.

## Layout

Each directory is one project root:

```text
<name>/
  data/            the sources: templates/*.abt and one or more *.ab files
  assets/          the assets/ directory of SPEC 2.3 (empty when unused)
  expected.json    the exact bytes of `abstract compile <name> JSON`
```

One project, `formats`, also carries `expected.yml` and `expected.abraw`, because
the documents show all three renderings of the same data. A project may carry a
`README.md` of its own when the reasoning behind it is worth spelling out next
to the source.

## Running them

```sh
abstract compile docs/examples/<name> JSON
abstract compile docs/examples/<name> YML
abstract compile docs/examples/<name> RAW
abstract lint docs/examples/<name>
```

Compare stdout with `expected.json` byte for byte after the one normalisation
SPEC 11.2 permits: the string value of `abstract.compiler` is replaced with
`0.0.0` in both streams. The expected files are written with `1.0.0` so they
read as real output. Line endings are `LF` and every file ends with exactly one
newline.

Every project compiles with exit code 0. None of them needs `--skip-assets`:
the image and file assets they reference are checked in and have the headers
their schemas declare.

## Index

| Project | Covers | SPEC |
|---|---|---|
| `hello` | the smallest project: one schema, one instance, one `text` field | 4.4.1 |
| `hello-rule` | the same project with a default and one `require` in a logic block | 4.10, 6.2 |
| `types-tour` | all nine types in one schema, including `file`, `image`, `ref` and `$(Schema)` | 4.4 |
| `groups-and-lists` | groups, optional groups, list cardinality, list groups with `@tag`, an omitted optional list | 4.6, 4.7, 4.8 |
| `tables-and-wildcards` | tuple arrays, enum wildcards in a list and in a tuple cell, brace file patterns | 5.5, 5.6, 5.8 |
| `clones` | full clone, partial clone, header tags cloned, an explicitly empty list | 5.7 |
| `clone-merge` | two clone sources folded, keyed-list merge by tag value, `#tag` boolean flag | 5.7, 5.5 |
| `interpolation` | `$name`, `${name}`, `$$`, and an asset path derived from the id | 5.11 |
| `paths-and-blocks` | dotted paths, multi-path assignment, body blocks | 5.4 |
| `numbered-keys` | numbered field names as fixed positional slots, defaults inside a present group | 4.9, 7.3 |
| `logic-basics` | `derive`, `derive?`, `require`/`else throw`, `if`/`else if`/`else`, `length`, `contains`, `exists` | 6.2, 6.4 |
| `logic-version` | the `version` built-in in a condition and in a `derive` | 6.6 |
| `versions-overlays` | `@since` / `@removed` on fields, two overlays from one instance | 4.12, 7.5 |
| `instance-windows` | instance version windows, a non-empty `removed` list, an instance absent from the base | 5.14, 7.5 |
| `version-statements` | two annotated statements writing one path over disjoint version ranges | 5.13 |
| `overlays-multi-match` | overlay reduction with overlapping ranges: several overlays matching one version, an instance only an overlay carries, and a non-empty `removed` | 7.5 |
| `formats` | the same document in JSON, YAML and RAW | 8.4, 8.5, 8.6 |
| `release-policy` | a small policy schema with a cardinality-constrained list and one `require` | 4.6, 6.2 |
| `sticker-pack` | the full worked example: versions, a window, a clone, wildcards, a tuple array, logic, overlays | Appendix C |
| `readme-tour` | the front-page tour: versions, `ref`, `image`, a group, a keyed list, one `require`, one overlay | 4.4, 7.5 |

## Where each project is used

| Project | Documents |
|---|---|
| `hello` | `README.md`, `abstract-language.md`, site: `learn/first-project.html` |
| `hello-rule` | site: `learn/first-project.html` |
| `types-tour` | `technical-reference.md`, site: `docs/types.html` |
| `groups-and-lists` | `abstract-language.md`, site: `docs/schemas.html` |
| `tables-and-wildcards` | `abstract-language.md`, site: `docs/instances.html` |
| `clones` | `abstract-language.md`, site: `docs/instances.html`, `examples/product-catalog.html` |
| `clone-merge` | `abstract-language.md`, `technical-reference.md` |
| `interpolation` | `abstract-language.md`, site: `docs/instances.html` |
| `paths-and-blocks` | `abstract-language.md`, site: `docs/instances.html` |
| `numbered-keys` | `technical-reference.md`, site: `docs/schemas.html` |
| `logic-basics` | `abstract-language.md`, site: `docs/logic.html` |
| `logic-version` | `abstract-language.md`, site: `docs/logic.html` |
| `versions-overlays` | `raw-data.md`, site: `docs/versions.html` |
| `instance-windows` | `raw-data.md`, site: `docs/versions.html` |
| `version-statements` | site: `docs/versions.html` |
| `overlays-multi-match` | conformance coverage for SPEC 7.5; referenced from `raw-data.md` |
| `formats` | `raw-data.md`, site: `docs/raw.html` |
| `release-policy` | site: `examples/release-policy.html` |
| `sticker-pack` | [AI authoring guide](../ai/README.md) |
| `readme-tour` | `README.md`, site: `index.html` |
