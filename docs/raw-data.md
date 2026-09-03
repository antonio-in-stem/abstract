# Abstract Compiled Data

Abstract's compiled output is validated structured data. The source language is
small and pleasant for authors; the compiler output is standard JSON, YAML, or
RAW so other runtimes can consume it without learning a custom artifact format.

Every compiled document has a top-level `data` array:

```json
{
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "status": "active",
      "price": 19.5,
      "featured": true,
      "tags": ["core", "public"]
    }
  ]
}
```

Numbers and booleans are native values: an authored `19.5` is a JSON number
and `true` is a JSON boolean, never the strings `"19.5"` or `"true"`.

The same project can be emitted as YAML:

```yaml
data:
  - template: "Product"
    id: "atlas"
    status: "active"
    tags:
      - "core"
      - "public"
```

## The RAW text format

`abstract compile <path> RAW` emits the same data as a review-friendly text
document (`.abraw`): unquoted keys, four-space indentation, one instance per
block. It exists for diffs and reviews where JSON punctuation is noise.

```text
data: [
    {
        template: "Product",
        id: "atlas",
        status: "active",
        price: 19.5
    }
]
```

## Contract

Every compiled instance has:

- `template`: the schema name used for validation.
- `id`: explicit `@id.x` or the source file stem.
- Schema fields in schema order (nested `$(Schema)` objects and groups are
  also ordered by their schema).
- Any extra fields after schema fields.

Enums are normalized to lowercase snake case. Text values preserve human casing,
spaces, and accents; `\n`, `\t`, quotes, and backslashes are escaped in output.

## Sealed bundles

`abstract bundle` wraps the compiled JSON document in an encrypted,
tamper-evident `.abx` container (ChaCha20-Poly1305). The Java runtime in
`java/` opens bundles on Java 8+; see `java/README.md`.

## CLI

Direct compilation names the instance, the template, the output format, and an
optional write flag:

```text
abstract object.ab Template.abt JSON
abstract object.ab Template.abt YML true
```

The compiler always prints the validated JSON or YAML to stdout. When the final
argument is `true`, it also writes a sibling file next to the first input:

- `object.json` for `JSON`
- `object.yml` for `YML`

If the final argument is missing or `false`, no file is written.
