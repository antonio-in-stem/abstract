# Writing Abstract

This guide helps an AI assistant author correct Abstract 1.2 source. It is a
compact working guide; [SPEC.md](../SPEC.md) remains normative.

## Start from the contract

Before editing, locate the project root and read every relevant `.abt` file.
The usual layout is:

```text
project/
  data/
    templates/   # .abt schemas and logic
    ...           # .ab instances may be organized below data/
  assets/         # files and images referenced by instances
```

The project root is the parent of the nearest ancestor directory named `data`.
Do not invent fields, enum members or schema names. When adding an instance to
an existing project, derive every assignment from its schema and logic. Check
whether the project declares `versions`; if it does, every version in that
closed range must compile.

Use the simplest construct that matches nearby source. Explicit fields are
usually easier to review than clones, tuple arrays or wildcards. Use compact
forms when the project already uses them or when they clearly express declared
structure.

## Small complete project

This schema declares one required text field:

```abstract
schema Label {
    caption: text(1..40)
}
```

This instance selects the schema, supplies a stable id and assigns the field:

```abstract
Label :: @id.hello
    caption: "Hello, world"
```

The complete project and expected JSON are under
[`docs/examples/hello`](../examples/hello/). The repository test suite compiles
that project and compares its output with the checked-in result.

## Write `.abt` schemas

A schema names the allowed shape. Schema names are case-sensitive and match
`[A-Za-z][A-Za-z0-9_]*`. Field names are ASCII identifiers; they are
case-insensitive after normalisation, and `-` normalises to `_`.

```abstract
schema Product {
    title: text(1..80)
    quantity: int(1..100)
    price: float(0..10000)
    active: bool = true
    tier: enum(basic, plus, premium)
    note: text @optional
    tags[1..8]: enum(core, seasonal, featured)
}
```

Choose a type from the current language: `text`, `int`, `float`, `bool`,
`enum`, `file`, `image`, `ref(Schema)` or an embedded `$(Schema)`. A field head
ending in `[]`, `[min..]` or `[min..max]` is a list. A brace block declares a
group.

Requiredness is explicit:

- A field with neither `@optional` nor a default is required.
- An absent `@optional` field is omitted. Abstract has no `null` value.
- A default supplies the value when the author omits it.
- A field cannot have both `@optional` and a default.

Place modifiers after the type and before a default:

```abstract
caption: text(1..80) @public = "Welcome"
legacy_code: int @optional @removed(3)
```

`@public` nominates exactly that declared field for a public contract. It does
not nominate a group's children, change validation, or make the value mutable
at runtime. Use it only when the product's authoring contract calls for public
data or overrides.

At the root, `template` cannot be declared. `id` is supplied by the instance
header or file stem and is never assigned in an instance body. Most schemas can
use the implicit `id: text(1..64)`; an explicit root `id` declaration may only
be `id: text` or `id: text(<range>)`, with no modifiers or default.

## Write `.ab` instances

Start each instance with the exact, case-sensitive schema name:

```abstract
Product :: @id.atlas
    title: Atlas
    quantity: 4
    price: 12
    tier: plus
    tags: [core, featured]
```

The id is project-wide and normalised. If `@id` is omitted, the source file
stem becomes the id. Never write `id:` or `template:` in the body.

For a schema that declares `owner.name` and `owner.email`, choose either dotted
paths or an equivalent body block:

```abstract
    owner.name: Ada
    owner.email: ada@example.com
```

```abstract
    owner {
        name: Ada
        email: ada@example.com
    }
```

Write each path at most once for any overlapping set of versions. Assigning a
path again is an error; it does not replace the earlier statement.

### Match values to their declared types

- `int` accepts signed integer literals.
- `float` accepts float literals and integer literals. For example, `price: 12`
  is valid for a `float` field and is emitted as `12.0`. An `int` field does not
  accept `12.0`.
- `bool` accepts only lowercase `true` or `false`.
- `enum` accepts only declared members and emits their normalised spelling.
- Quoting does not convert types: `"12"`, `"12.0"` and `"true"` are strings,
  so they fail on numeric and boolean fields.

A comma outside brackets always separates list items. Quote text containing a
comma:

```abstract
caption: "Hello, world"
tags: [core, featured]
```

The list can also be written as `tags: core, featured`. Choose one spelling;
do not assign `tags` twice.

Quote text that begins with `[`, `#`, `"` or `//`. Quoted strings support only
`\"`, `\\`, `\n`, `\r` and `\t` escapes. A spaced `//` begins a comment outside
a quoted string.

Asset values are relative to the project's `assets/` directory. Do not include
an `assets/` prefix and do not use an absolute path or `..`:

```abstract
icon: ./textures/atlas.png
```

## Use reuse and versions deliberately

A clone line belongs directly below the instance header, before assignments:

```abstract
Product :: @id.beacon
&atlas.*
    title: Beacon
```

A clone copies authored values before defaults, interpolation and logic. The
new instance's statements override cloned values. When overriding a cloned
list, restate the complete list unless the schema's keyed-list behavior is
specifically intended. See [SPEC 5.7](../SPEC.md#57-clones) before composing
multiple clones.

`$name`, `${name}` and `$id` interpolate root scalar values. Interpolation
happens after cloning, so a cloned `./textures/$id.png` uses the new instance's
id. Use `$$` for a literal dollar sign; interpolation also applies inside quoted
values. See [SPEC 5.11](../SPEC.md#511-interpolation).

Use `@since(n)` and `@removed(n)` only within the declared project range. A
field exists for `since <= version < removed`. An unannotated assignment to a
versioned field is automatically limited to the versions where that field
exists. Add statement annotations only to narrow that window or provide
different values over disjoint windows.

## Write logic only from declared fields

Logic belongs in `.abt` and names an existing schema:

```abstract
logic Product {
    require .quantity > 0 else throw "Quantity must be positive."
}
```

Field paths in conditions start with `.`. Conditions are boolean; Abstract has
no general truthiness. Use `exists` to distinguish an absent optional value.
Use `contains` for a list or projected path. Every `require` needs a quoted
`else throw` message.

Use `derive` to write a value and `derive?` to write only when the target is
absent. Because defaults are filled before logic, `derive?` cannot target a
field with a default. Guard a write to a versioned field so it cannot execute
when the field is absent.

Arithmetic is available only through explicit `calc(...)` expressions in
logic. For a `Line` schema that declares the three numeric fields below:

```abstract
logic Line {
    derive .total_cents = calc(.quantity * .unit_price_cents)
}
```

An instance value such as `note: 2 + 3` is text, not a calculation. Read the
normative [arithmetic contract](../ARITHMETIC.md) before using operators or
numeric functions. Use integer minor units for exact money values; `float` is
finite IEEE-754 binary64.

## Validate and inspect

After every coherent edit:

```sh
abstract lint <project>
abstract compile <project> JSON
```

Fix the source location named by the first diagnostic, then rerun. Common
causes are an unknown field, a missing required field, a value with the wrong
shape, a value outside its range, an undeclared enum member, or a path written
twice. Do not work around a diagnostic by weakening the schema unless changing
the contract is part of the request.

Before handing off the files, confirm that:

- every assignment comes from the selected schema;
- every required field has a value in every applicable version;
- values match their declared types and ranges;
- asset paths resolve under `assets/`;
- `lint` exits successfully; and
- compiled JSON contains the intended defaults, derived values and overlays.
