# Abstract Compiled Data

Abstract's compiled output is validated structured data. The source language is
small and pleasant for authors; the compiler output is standard JSON or YAML so
other runtimes can consume it without learning a custom artifact format.

Every compiled document has a top-level `data` array:

```json
{
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "status": "active",
      "tags": ["core", "public"]
    }
  ]
}
```

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

## Contract

Every compiled instance has:

- `template`: the schema name used for validation.
- `id`: explicit `@id.x` or the source file stem.
- Schema fields in schema order.
- Any extra fields after schema fields.

Enums are normalized to lowercase snake case. Text values preserve human casing,
spaces, and accents.

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
