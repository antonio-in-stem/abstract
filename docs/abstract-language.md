# Abstract Language Guide

Abstract is a compact language for declaring schemas and instances. A schema
describes what a class of data may contain. An instance declares one concrete
object, and the compiler checks it against the schema before producing JSON,
YAML, or RAW data.

The design goal is simple: make structured data feel authored, not merely
configured.

## File Types

`.abt` files define schemas:

```abstract
schema Product {
    id: text(1..40)
    status: enum(draft, active, retired)
    release_year: int(2000..2100)
    price: float(0..9999) = 0.0
    featured: bool = false
}
```

`.ab` files define instances:

```abstract
Product :: @id.atlas, @status.active, @release_year.2026, @featured
    price: 19.5
```

The compiler emits validated JSON (or YAML/RAW):

```json
{
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "status": "active",
      "release_year": 2026,
      "price": 19.5,
      "featured": true
    }
  ]
}
```

Values keep their authored types: `19.5` is a number, `true` is a boolean,
`1.21.5` stays text (multiple dots is not a number).

## Schemas

A schema is declared with `schema Name { ... }`. Field names normalize to
lowercase snake case. Schema names must be unique across the project.

```abstract
schema Service {
    id: text(1..40)
    tier: enum(free, standard, premium)
    replicas: int(1..12)
    weight: float
    public: bool
}
```

Supported field types:

- `text(1..40)`: string with allowed length ranges (characters, not bytes).
- `int(0, 2..15)`: integer with exact values and ranges.
- `float(0..1)`: decimal number with ranges; `float(0.5, 1.5..2.5)` mixes both.
- `bool`: `true` or `false`.
- `enum(a, b, c)`: one value from a fixed vocabulary.
- `file(png, jpg)`: file path with an allowed extension; the file must exist.
- `image(png 128x128, jpg)`: image path validated by its real header bytes.
- `$(OtherSchema)`: nested object validated by another schema.

`text`, `int`, and `float` also work bare (no parentheses) for unconstrained
values.

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

### The image type

`image(...)` validates assets without decoding them. The compiler reads only
the header bytes (26 bytes for a PNG) to learn the true format and
dimensions, so validating hundreds of textures stays instant.

```abstract
icon: image(png 128x128)          // exact size
hero: image(png 1920x1080, jpg)   // alternatives
thumb: image(png 64x*)            // fixed width, any height
```

Three failures are caught at compile time: a disallowed extension, a file
whose bytes do not match its extension (a GIF renamed to `.png`), and wrong
dimensions. Supported containers: PNG, JPG, GIF, BMP, WebP.

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

Header tags are concise field assignments. A bare tag assigns `true`, which
is the natural shape for boolean flags:

```abstract
Product :: @id.atlas, @featured        // featured: true
```

Longer or nested values go below the header:

```abstract
Product :: @id.atlas, @status.active
    name: Atlas Search
    owner.team: Knowledge Systems
    capabilities: [#search, #sync(availability: beta)]
```

Strings support `\"`, `\\`, `\n`, `\t`, and `\r` escapes. Unquoted URLs
(`https://...`) and values containing `::` are safe; quote values that
contain commas or an `@`.

A statement continues across lines while brackets are open or while the line
ends with a comma:

```abstract
tags: [
    core,
    public
]
```

### Strict fields

Instances may only assign fields their schema declares. A typo fails with a
suggestion:

```
Unknown field 'rarty' at Sticker. Did you mean 'rarity'? Declared fields: ...
```

Every instance id must be unique across the project. Use `--allow-unknown`
only while migrating legacy data.

## Tuple Arrays

Tuple arrays are compact table-like assignments.

```abstract
copy(key, value): (en_us, Welcome), (es_*, Bienvenido)
```

The compiler maps each tuple to an object:

```abstract
copy: [
    {key: "en_us", value: "Welcome"},
    {key: "es_es", value: "Bienvenido"},
    {key: "es_mx", value: "Bienvenido"}
]
```

`es_*` expands against the enum declared for `key`. Tuple arity is checked:
a row with the wrong number of values is a compile error with a line number.
One naming rule to know: a field named `lang` is emitted as `lang_values`.

Wildcards also work in plain enum list fields:

```abstract
flags: hat_*        // expands to every enum member starting with hat_
```

## Path Assignment

Dotted paths assign nested data:

```abstract
owner.team: Knowledge Systems
```

Multi-paths assign the same value to many keys:

```abstract
slots.{3, 4}: #custom
```

Brace file patterns expand into arrays:

```abstract
images: ./assets/{hero,thumbnail}.png
```

becomes `["./assets/hero.png", "./assets/thumbnail.png"]`. Quoted strings
never expand, and pieces with spaces outside the braces stay literal text.

## Interpolation

Root variables can be interpolated into paths and strings. The variable is
read from the final instance after clone overrides. Longer names win, so
`$id` never clobbers `$identity`; `${id}` disambiguates explicitly and `$$`
writes a literal dollar sign.

```abstract
Product :: @id.atlas
    image: ./textures/$id.png
    label: ${id}_display
    note: "$$99 special"
```

When compiled from a project folder, file paths are checked relative to the
project `assets` folder. For example `./textures/$id.png` resolves to
`<project>/assets/textures/atlas.png`.

## Cloning

An instance can clone another instance before applying overrides:

```abstract
&atlas.*

Product :: @id.beacon, @status.draft
    name: Beacon Export
```

The target is resolved by instance id. If no `@id` is provided, the file
stem is used as the id. Cycles are rejected.

Two more forms complete the template-instance model:

```abstract
&hero.stats          // partial clone: copy only the stats subtree

&base.*              // multiple clones deep-merge in order;
&winter_theme.*      // later clones win on conflicting fields
```

## Logic Blocks

`logic Template { ... }` binds business rules to a schema. Schema validation
runs first, so logic receives typed and normalized data. Logic may derive
values on the current instance and may reject the instance with a `throw`
message.

```abstract
logic Product {
    derive .shipping_class = standard
    derive? .release_wave = 1            // only when the author did not set it

    if .status == "active" {
        require .owner.contact exists
            else throw "Active products need an owner contact."
    } else if .status == "retired" {
        derive .visibility = hidden
    } else {
        derive .visibility = internal
    }

    require not .flags contains "banned"
        else throw "Banned products cannot ship."
}
```

Supported statements:

- `if <condition> { ... } else if ... { ... } else { ... }`: branching. Both
  `}` + `else {` and `} else {` styles parse.
- `derive .path = value`: writes a derived value onto the current instance.
- `derive? .path = value`: writes only when the field is missing.
- `require <condition> else throw "message"`: fails compilation when false.
- `for item in .list { ... }`: loops over array values and binds `item`.
- `for $name in [a, b, c] { ... }`: loops over literal values and exposes a
  `$name` variable for dynamic paths, derive values, and messages.

Supported condition operators:

- Existence: `.owner.contact exists`
- Membership: `.flags contains "public"`
- Equality: `.status == "active"`, `.status != "draft"`
- Numeric comparison: `.variant_count >= 2`, `.price < 99.5` (ints and floats)
- Length: `length(.items) == 1`
- Negation: `not <condition>`, `!(<condition>)`
- Boolean composition: `and`, `or`, `&&`, `||`

Paths start with `.` for the root instance. Inside loops, variable paths such
as `entry.value exists` read from the current loop item. `$variables` can be
used in logic paths, derive values, and messages, for example
`.slots.$slot.mode` or `else throw "slot $slot needs distribution"`. A derive
value that is exactly one variable keeps its native type. Indexed paths such
as `.items[0]` read a single array element.

For `file(...)` and `image(...)` fields, `exists` checks the real filesystem
when the compiler is reading from disk. Arrays must be non-empty and every
path in the array must exist. Paths like `./textures/a.png` resolve under the
project `assets` folder; paths beginning with `./assets/` resolve from the
project root.

## CLI

```powershell
abstract init my-pack
abstract compile my-pack JSON
abstract compile my-pack YML true
abstract compile my-pack RAW --out review.abraw
abstract object.ab Template.abt JSON
abstract lint my-pack
abstract templates my-pack
abstract bundle my-pack --key "release passphrase" --out my-pack.abx
abstract unbundle my-pack.abx --key "release passphrase"
```

Direct compilation accepts one or more source paths followed by `JSON`,
`YML`, or `RAW`. The optional final `true` writes a sibling output file;
`--out <file>` chooses the path explicitly.

When a direct `.ab` file references a clone in the same directory, Abstract
loads the sibling `.ab`/`.abt` files as symbol context and still emits only
the explicit `.ab` file that was requested.

`lint` validates schemas and instances without writing output. `templates`
lists known schema names. `--skip-assets` skips on-disk file and image
checks; `--allow-unknown` accepts undeclared fields.

`bundle` seals compiled JSON into a tamper-evident encrypted `.abx`
container (ChaCha20-Poly1305, RFC 8439) for shipping inside applications.
The `java/` directory contains a zero-dependency Java 8+ runtime that opens
bundles and exposes typed access; see `java/README.md` and
`java/SECURITY.md`.

## Boundaries

v0.2 is deliberately small:

- JSON, YAML, RAW, and sealed bundles are the compiler outputs.
- Logic can derive values and validate relationships, but it should remain
  small policy code attached to a schema.
- The parser targets the authored Abstract style in this repository; future
  versions can add a formal grammar/LSP protocol once the language shape
  settles.
