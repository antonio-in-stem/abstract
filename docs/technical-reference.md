# Abstract Technical Reference

This document is the practical reference for building Abstract projects. It
separates the two languages that live in Abstract:

- `.ab` instance files: authored data objects.
- `.abt` template files: schemas and logic.

The compiler reads both, validates every instance, runs template logic, and
emits JSON or YAML.

## Project Layout

A normal project can be a folder with `.ab` and `.abt` files:

```text
project/
  data/
    Product.abt
    atlas.ab
    beacon.ab
  assets/
    textures/
      atlas.png
```

When the compiled folder is named `data`, Abstract treats its parent as the
project root. That means file paths inside `.ab` can reference sibling assets.

```abstract
image: ./textures/atlas.png
```

At validation time this resolves to:

```text
project/assets/textures/atlas.png
```

If the path already starts with `./assets/...`, Abstract also resolves it from
the project root:

```abstract
image: ./assets/textures/atlas.png
```

## CLI

Compile one object against one template:

```powershell
abstract object.ab Template.abt JSON
abstract object.ab Template.abt YML
```

Write the output next to the first input:

```powershell
abstract object.ab Template.abt JSON true
```

Compile or lint a whole project:

```powershell
abstract compile ./data JSON
abstract compile ./data YML true
abstract compile ./data RAW --out review.abraw
abstract lint ./data
abstract templates ./data
```

Scaffold, seal, and inspect:

```powershell
abstract init my-pack
abstract bundle ./data --key "release passphrase" --out data.abx
abstract unbundle data.abx --key "release passphrase"
```

Useful flags: `--skip-assets` (skip on-disk file/image checks),
`--allow-unknown` (accept undeclared fields), `--out <file>`.

When compiling direct files, Abstract loads sibling `.ab` and `.abt` files as
symbol context so clone references can resolve. The emitted data is still
limited to the explicit `.ab` file or files passed on the command line.

## `.ab` Instance Files

An instance starts with a template name, then `::`, then optional header tags.

```abstract
Product :: @id.atlas, @status.active
    name: Atlas Search
```

Every instance compiles to an object that includes:

```abstract
template: "Product"
id: "atlas"
```

If `@id.*` is missing, the file stem is used as the id.

### Header Tags

Header tags are short assignments:

```abstract
Product :: @id.atlas, @status.active, @tier.premium
```

They are equivalent to:

```abstract
id: atlas
status: active
tier: premium
```

Use header tags for compact required values. Use body assignments for long,
nested, or repeated values.

### Body Assignments

Body assignments use `field: value`:

```abstract
name: Atlas Search
status: active
```

Dotted paths write nested objects:

```abstract
owner.team: Knowledge Systems
owner.contact: systems@example.com
```

Multi-path braces assign the same value to several branches:

```abstract
limits.{soft, hard}: 10
```

### Lists

Abstract infers lists from comma-separated values:

```abstract
tags: core, public, ai_ready
```

Line breaks after commas also continue the list:

```abstract
tags: core,
      public,
      ai_ready
```

Brackets are still valid when they make the source clearer:

```abstract
tags: [core, public, ai_ready]
```

### Tagged Objects

Schemas can mark one field in a group with `@tag`. Instances can then use
`#tag` shorthand.

Template:

```abstract
capability[] {
    id: enum(search, sync, export) @tag
    availability: enum(alpha, beta, stable) = stable
}
```

Instance:

```abstract
capability: #search, #sync(availability: beta)
```

Output:

```json
[
  {"id": "search", "availability": "stable"},
  {"id": "sync", "availability": "beta"}
]
```

### Tuple Arrays

Tuple arrays create repeated objects from column names:

```abstract
copy(key, value): (en_us, Welcome), (es_es, Bienvenido)
```

Output:

```json
[
  {"key": "en_us", "value": "Welcome"},
  {"key": "es_es", "value": "Bienvenido"}
]
```

Enum wildcards expand when the target column is an enum:

```abstract
copy(key, value): (es_*, Bienvenido)
```

If the schema enum contains `es_es`, `es_mx`, and `es_ar`, the tuple expands to
three objects.

### Clone Inheritance

Clone a previous instance by id:

```abstract
&atlas.*

Product :: @id.beacon, @status.draft
    name: Beacon Export
```

Clone runs before local assignments. Local assignments override cloned values.
Clone targets resolve by instance id, not by file path. In direct CLI mode,
sibling `.ab` files are loaded as clone context.

### File Patterns

Brace patterns expand file paths:

```abstract
distribution: ./textures/{hero,thumbnail}.png
```

Output:

```json
["./textures/hero.png", "./textures/thumbnail.png"]
```

### Root Variable Interpolation

`$name` inside a string or unquoted path resolves from a scalar value on the
current instance.

```abstract
Product :: @id.atlas
    image: ./textures/$id.png
```

Output:

```json
{"image": "./textures/atlas.png"}
```

This is general: `$id`, `$status`, `$variant_count`, or any other root scalar can
be interpolated. Interpolation happens after clone overrides, so cloned variants
use their own final values.

## `.abt` Template Files

Template files contain `schema` and `logic` blocks.

```abstract
schema Product {
    id: text(1..40)
    status: enum(draft, active, retired)
}

logic Product {
    require .status != "retired"
        else throw "Retired products cannot be compiled for this catalog."
}
```

### Schemas

A schema names the object shape:

```abstract
schema Product {
    id: text(1..40)
}
```

Supported field types:

- `text(1..40)`: text with allowed length ranges (bare `text` allows any).
- `int(0, 2..15)`: integer with exact values and ranges.
- `float(0..1)`: decimal number with ranges; emitted as a native number.
- `bool`: `true` or `false`; emitted as a native boolean.
- `enum(a, b, c)`: one normalized value from a fixed set.
- `file(png, jpg)`: path whose extension must match; the file must exist on disk.
- `image(png 128x128, jpg)`: image path validated by its real header bytes
  (format, dimensions, extension-content mismatches). `*` means any size on
  one axis: `image(png 64x*)`.
- `$(OtherSchema)`: nested object validated by another schema.

Instances may only assign declared fields; unknown names fail with a
suggestion. Duplicate field names, schema names, and instance ids are errors.

### Lists

Add `[]` after the field name:

```abstract
tags[]: enum(core, public, internal) @optional
```

List fields accept either a single value or multiple values. A single value is
coerced into a one-item list after validation.

### Defaults

Defaults use `=`:

```abstract
status: enum(draft, active, retired) = draft
```

Missing fields with defaults are filled before logic runs.

### Optional Fields

Optional fields use `@optional`:

```abstract
notes: text(1..120) @optional
```

Optional lists default to an empty list.

### Groups

Groups create nested objects:

```abstract
owner {
    team: text(1..50)
    contact: text(1..80)
}
```

List groups create arrays of objects:

```abstract
capability[] {
    id: enum(search, sync, export) @tag
    availability: enum(alpha, beta, stable) = stable
}
```

### `@tag`

`@tag` marks the field that receives `#value` shorthand in `.ab` files.

Rules:

- A tagged field should be the natural identity of the group.
- Tagged enum fields allow wildcard expansion in tuple arrays and group lists.
- Tagged groups can still receive additional values with `#value(key: other)`.

### File Types and `exists`

`file(png, jpg)` validates the extension and, when compiling from disk, that
the file exists. `image(...)` additionally probes the file header (reading
only the first bytes) to verify the real format and dimensions. The logic
operator `exists` performs the same on-disk resolution inside conditions.

```abstract
schema Asset {
    image: image(png 128x128)
    manual: file(pdf) @optional
    has_manual: bool = false
}

logic Asset {
    if .manual exists {
        derive .has_manual = true
    }
}
```

`--skip-assets` disables all of these on-disk checks for machines that do not
have the binary assets checked out.

Resolution rules:

- `./textures/a.png` -> `<project>/assets/textures/a.png`
- `textures/a.png` -> `<project>/assets/textures/a.png`
- `./assets/textures/a.png` -> `<project>/assets/textures/a.png`
- Absolute paths are checked as-is.

For arrays, `exists` requires the array to be non-empty and every item to exist.

## Logic Blocks

`logic Name` attaches rules to `schema Name`.

Validation order:

1. Parse `.ab` and `.abt`.
2. Build clone inheritance.
3. Interpolate root `$variables`.
4. Validate schema types, defaults, optional fields, and enums.
5. Run logic.
6. Re-validate schema after derived values.
7. Emit JSON or YAML.

### `derive` and `derive?`

`derive` writes a value to the current instance:

```abstract
logic Product {
    derive .shipping_class = standard
    derive? .release_wave = 1
}
```

`derive?` writes only when the field is missing, so authored values win. Both
appear in output; derived values are data, not hidden metadata. Derive values
interpolate `$loop` variables and root `$fields`; a value that is exactly one
variable keeps its native type (`derive .count = $n` stays an int).

### `if` / `else if` / `else`

`if` runs nested statements when the condition is true; chains branch:

```abstract
if .status == "active" {
    derive .published = true
} else if .status == "retired" {
    derive .published = false
} else {
    derive .published = false
}
```

### `require ... else throw`

`require` rejects invalid instances:

```abstract
require .owner.contact exists
    else throw "Active products require owner.contact."
```

Write throw messages for the person editing the `.ab` file. A good error says
what failed, why it matters, and what to change.

### Loops

Loop over literal lists:

```abstract
for $slot in [1,2,3] {
    if .slots.$slot.mode == "custom" {
        require .slots.$slot.distribution exists
            else throw "Slot $slot is custom and needs distribution files."
    }
}
```

Loop variables can be used in:

- Dynamic paths: `.slots.$slot.mode`
- Comparisons: `.type == "$kind"`
- Error messages: `"Slot $slot is invalid"`

### Conditions

Supported checks:

```abstract
.field
.field exists
.flags contains "public"
not .flags contains "banned"
!(.count > 3)
.status == "active"
.status != "draft"
.count >= 2
.price < 99.5
length(.items) == 1
.a == "x" && .b exists
.a == "x" || .b == "y"
```

Comparisons work across ints and floats; equality also understands booleans.

Paths over arrays are projected. For example:

```abstract
.capability.id contains "search"
```

checks every object in `capability` and succeeds if any `id` is `search`.

Indexed access is supported:

```abstract
.items[0].id == "first"
```

## Diagnostics

Abstract diagnostics are designed to name the broken contract:

- Missing required field: add the field, header tag, default, or `@optional`.
- Unknown field: strict mode rejects typos with a "did you mean" suggestion.
- Type mismatch: the value shape does not match the schema type.
- Enum mismatch: the value is not in the schema vocabulary (with suggestion).
- Range mismatch: the text length or numeric value is outside allowed ranges.
- Image mismatch: wrong size, or content that does not match the extension.
- Duplicate id / duplicate schema: both definition sites are named.
- Tuple arity mismatch: a row has the wrong number of values.
- Unknown template: the instance references a schema that was not loaded.
- Unknown clone target: `&id.*` cannot find an instance with that id.
- Missing asset: a `file(...)`/`image(...)` value or `exists` check could not
  find the referenced file.

The terminal format is:

```text
abstract: path/to/file.ab: Detailed message.
abstract: path/to/file.ab:12: Parse-stage message with a line number.
```

Editor integrations use this format to attach diagnostics to the relevant file.

## Style Guide

For `.abt`:

- Use schemas for shape and types.
- Use logic for relationships between fields.
- Keep throw messages explicit and action-oriented.
- Prefer derived values over duplicating facts in every instance.
- Keep the language general; domain rules belong in templates, not in the compiler.

For `.ab`:

- Put short required fields in the header.
- Put repeated and nested data in the body.
- Use clone only for true variants that should inherit future changes.
- Use `$id` interpolation for asset paths that follow the instance id.
- Keep generated JSON/YAML out of hand-authored workflows.

