# Abstract Language Guide

Abstract is a compact language for declaring schemas and instances. A schema
describes what a class of data may contain. An instance declares one concrete
object, and the compiler checks it against the schema before producing JSON or
YAML data.

The design goal is simple: make structured data feel authored, not merely
configured.

## File Types

`.abt` files define schemas:

```abstract
schema Product {
    id: text(1..40)
    status: enum(draft, active, retired)
    release_year: int(2000..2100)
}
```

`.ab` files define instances:

```abstract
Product :: @id.atlas, @status.active, @release_year.2026
```

The compiler emits validated JSON or YAML:

```json
{
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "status": "active",
      "release_year": 2026
    }
  ]
}
```

## Schemas

A schema is declared with `schema Name { ... }`.

```abstract
schema Service {
    id: text(1..40)
    tier: enum(free, standard, premium)
    replicas: int(1..12)
}
```

Supported field types in v0:

- `text(1..40)`: string with allowed length ranges.
- `int(0, 2..15)`: integer with exact values and ranges.
- `enum(a, b, c)`: one value from a fixed vocabulary.
- `file(png, jpg)`: file path with an allowed extension.
- `$(OtherSchema)`: nested object validated by another schema.

Lists are marked with `[]`:

```abstract
tags[]: enum(core, public, internal) @optional
```

Defaults use `=`:

```abstract
availability: enum(alpha, beta, stable) = stable
```

Optional fields use `@optional`:

```abstract
notes: text(1..120) @optional
```

## Groups

Groups are nested objects.

```abstract
schema Product {
    owner {
        team: text(1..50)
        contact: text(1..80)
    }
}
```

List groups represent arrays of objects:

```abstract
capabilities[] {
    id: enum(search, sync, export) @tag
    availability: enum(alpha, beta, stable) = stable
}
```

## Tags

`@tag` marks the field filled by shorthand syntax.

Given:

```abstract
capabilities[] {
    id: enum(search, export) @tag
    availability: enum(alpha, stable) = stable
}
```

This instance:

```abstract
capabilities: [#search, #export(availability: alpha)]
```

Compiles to:

```abstract
capabilities: [
    {id: "search", availability: "stable"},
    {id: "export", availability: "alpha"}
]
```

Tags also work for non-`id` fields. If a schema marks `mode` as `@tag`, then
`#generate(...)` fills `mode: "generate"`.

## Instances

An instance starts with `Template ::`.

```abstract
Product :: @id.atlas, @status.active, @tier.premium
```

Header tags are concise field assignments. They are best for short required
values. Longer or nested values go below the header:

```abstract
Product :: @id.atlas, @status.active
    name: Atlas Search
    owner.team: Knowledge Systems
    capabilities: [#search, #sync(availability: beta)]
```

## Tuple Arrays

Tuple arrays are compact table-like assignments.

```abstract
copy(key, value): (en_us, Welcome), (es_*, Bienvenido)
```

The compiler maps each tuple to an object:

```abstract
copy_values: [
    {key: "en_us", value: "Welcome"},
    {key: "es_es", value: "Bienvenido"},
    {key: "es_mx", value: "Bienvenido"}
]
```

`es_*` expands against the enum declared for `key`.

## Path Assignment

Dotted paths assign nested data:

```abstract
owner.team: Knowledge Systems
```

Multi-paths assign the same value to many keys:

```abstract
thresholds.{warning, critical}: 90
```

Brace file patterns expand into arrays:

```abstract
./assets/{hero,thumbnail}.png
```

becomes:

```abstract
["./assets/hero.png", "./assets/thumbnail.png"]
```

Root variables can be interpolated into paths and strings. The variable is read
from the final instance after clone overrides:

```abstract
Product :: @id.atlas
    image: ./textures/$id.png
```

When compiled from a `data` folder, file paths are checked relative to the
project assets folder. For example `./textures/$id.png` resolves to
`../assets/textures/atlas.png` when used with the `exists` logic operator.

## Cloning

An instance can clone another instance before applying overrides:

```abstract
&atlas.*

Product :: @id.beacon, @status.draft
    name: Beacon Export
```

The target is resolved by instance id. If no `@id` is provided, the file stem is
used as the id.

## Logic Blocks

`logic Template { ... }` binds business rules to a schema. Schema validation runs
first, so logic receives typed and normalized data. Logic may derive values on
the current instance and may reject the instance with a `throw` message.

```abstract
logic Product {
    derive .shipping_class = standard

    if .status == "active" {
        require .owner.contact exists
            else throw "Active products need an owner contact."
    }
}
```

Supported statements in v0:

- `if <condition> { ... }`: runs nested rules when the condition is true.
- `derive .path = value`: writes a derived value onto the current instance.
- `require <condition> else throw "message"`: fails compilation when false.
- `for item in .list { ... }`: loops over array values and binds `item`.
- `for $name in [a, b, c] { ... }`: loops over literal values and exposes a
  `$name` variable for dynamic paths and comparisons.

Supported condition operators:

- Existence: `.owner.contact exists`
- Membership: `.flags contains "public"`
- Equality: `.status == "active"`, `.status != "draft"`
- Numeric comparison: `.variant_count >= 2`
- Length: `length(.items) == 1`
- Boolean composition: `and`, `or`, `&&`, `||`

Paths start with `.` for the root instance. Inside loops, variable paths such as
`entry.value exists` read from the current loop item. `$variables` can be used in
logic paths and messages, for example `.slots.$slot.mode` or
`else throw "slot $slot needs distribution"`. Indexed paths such as `.items[0]`
read a single array element.

For `file(...)` fields, `exists` checks the real filesystem when the compiler is
reading from disk. Arrays must be non-empty and every path in the array must
exist. Paths like `./textures/a.png` resolve under the project `assets` folder;
paths beginning with `./assets/` resolve from the project root.

## CLI

```powershell
abstract object.ab Template.abt JSON
abstract object.ab Template.abt YML true
abstract compile example JSON
abstract lint example
abstract templates example
```

Direct compilation accepts one or more source paths followed by `JSON` or `YML`.
The optional final boolean controls file writing. If it is `true`, Abstract also
writes a sibling `.json` or `.yml` file next to the first input. If it is
missing or `false`, Abstract only prints the compiled data.

When a direct `.ab` file references a clone in the same directory, Abstract loads
the sibling `.ab`/`.abt` files as symbol context and still emits only the
explicit `.ab` file that was requested.

`lint` validates schemas and instances without writing output.

`templates` lists known schema names.

## V0 Boundaries

V0 is intentionally small:

- JSON and YAML are the supported compiler outputs.
- Logic can derive values and validate relationships, but it should remain small
  policy code attached to a schema.
- The parser targets the authored Abstract style in this repository; future
  versions can add a formal grammar/LSP protocol once the language shape settles.
