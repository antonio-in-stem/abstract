# The Abstract language

Abstract is a schema-backed data language. You describe the shape of your
data once, write the data in a compact authoring syntax, and compile it into a
canonical JSON, YAML or RAW document that every other tool reads.

This guide teaches the language. It is not the specification. Whenever this
guide and [the specification](reference/specification.md) disagree, the specification wins; every section
here names the specification section it summarises. The complete grammar is in
[the grammar](reference/grammar.ebnf).

Every worked example in this guide is a real project under
[`examples/features/`](../examples/features/README.md), with the exact compiler output stored beside
it as `expected.json`.

---

## 1. The idea

A project has two kinds of source file.

| Extension | Kind | Contains |
|---|---|---|
| `.abt` | template | `schema` declarations, `logic` blocks, at most one `versions` line |
| `.ab` | instance | concrete objects, each validated against a schema |

The compiler reads both, checks every object against its schema, evaluates the
logic, and emits one **compiled document**. That document is the only artifact
consumers read.

```text
.abt schemas + .ab instances  ->  abstract compile  ->  JSON | YAML | RAW
```

Four principles decide every rule in the language (SPEC 1.2):

- **Minimal**; one way to express each thing. No imports, no free-key maps, no
  user-defined functions, no nested lists, no comments in output. Numeric
  calculations explicitly use `calc(...)` in logic.
- **Elegant**; the source is shorter and clearer than the JSON it produces.
- **Deterministic**; the same input bytes produce the same output bytes on
  every machine, in every locale, in any command-line order.
- **Strict**; every construct is either defined or an error with an identifier
  and a source position. Nothing is silently ignored, coerced or dropped.

---

## 2. A first project

Example project: [`examples/features/hello`](../examples/features/hello).

`data/templates/Label.abt`

```abstract
schema Label {
    caption: text(1..40)
}
```

`data/labels/hello.ab`

```abstract
Label :: @id.hello
    caption: "Hello, world"
```

```sh
abstract compile hello JSON
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

Three things are already visible:

- `Label ::` is an **instance header**. The name before `::` must be a declared
  schema; `@id.hello` gives the object its id.
- Every object carries `template` and `id` first. You never write them as
  fields; the compiler does (SPEC 4.11, 8.2).
- The document has an envelope: `abstract`, `data`, `overlays` (SPEC 8.1).

The value is quoted because an unquoted comma always separates list items. Read
section 5.4 before you write your first comma.

---

## 3. Files, projects and assets

SPEC 2.

### 3.1 Layout

The recommended layout puts sources under `data/` and binary assets under
`assets/`:

```text
m-project/            <- project root
  assets/
    textures/atlas.png
  data/               <- data directory, and the root of source discovery
    templates/Product.abt
    products/atlas.ab
```

The compiler finds the project by walking **up** from the path you name until a
directory called `data` appears; the project root is that directory's parent. A
project may also keep its sources in one plain directory with no `data/` at all,
in which case the directory you compile is the project root.

All three of these compile the same source set:

```sh
abstract compile m-project
abstract compile m-project/data
abstract compile m-project/data/products/atlas.ab
```

Naming a directory *inside* `data/` is an error (E806): it would silently
compile more than it names. Name the project root or the `data` directory.

### 3.2 Discovery

Discovery walks the data directory recursively and collects every `.ab` and
`.abt` file. Extensions are compared case-insensitively. Directories whose name
starts with `.`, and directories named `node_modules`, `target`, `build` or
`out`, are skipped. Symbolic links are resolved and each canonical directory is
visited once, so link cycles terminate and a file reachable by two paths appears
once. A project with no source files at all is an error (E103).

### 3.3 Single-file mode

Naming one or more `.ab` files compiles the whole project but emits only the
instances declared in those files. Everything else is still parsed and
validated, so the output for a given instance is identical either way
(SPEC 2.5).

### 3.4 Assets

`file` and `image` values name assets under `<project root>/assets`. There is
one resolution rule: strip one leading `./`, then resolve what remains relative
to `assets/`. So `./textures/a.png` and `textures/a.png` both mean
`assets/textures/a.png`, and `./assets/textures/a.png` means
`assets/assets/textures/a.png`; the prefix is not special-cased.

Asset values must be relative, must not contain `..`, and must not start with a
drive letter, a UNC prefix or a slash (E424). Use `/` as the separator so
sources stay platform-independent.

### 3.5 Case rules

Schema names and keywords are case-**sensitive**. Everything else that is an
identifier; field names, enum members, tag names, path segments, instance ids,
`ref` values; is **normalised**: ASCII-lowercased, with `-` replaced by `_`.
`Max-Count`, `max_count` and `MAX_COUNT` are one name (SPEC 2.6, 3.3).

---

## 4. Writing schemas

SPEC 4.

```abstract
schema Product {
    title: text(1..60)
    replicas: int(1, 3..12)
    price: float(0..9999)
    featured: bool
    status: enum(draft, active, retired)
}
```

A schema is a name and a block of fields. Names must be unique across the whole
project (E301). The **declared order of fields is the key order of the output**
(SPEC 8.3), so a schema is also a layout.

### 4.1 The nine types

SPEC 4.4. Example project: [`examples/features/types-tour`](../examples/features/types-tour).

| Type | Written | Value |
|---|---|---|
| text | `text`, `text(1..40)` | a string; ranges count Unicode scalar values |
| int | `int`, `int(1, 3..12)` | a signed 64-bit integer |
| float | `float`, `float(0..1)` | a finite binary64 number |
| bool | `bool` | `true` or `false` |
| enum | `enum(draft, active)` | one of the declared members, emitted normalised |
| file | `file(png, jpg)` | an asset path with one of the declared extensions |
| image | `image(png 128x128)` | an asset path, checked for format and size |
| ref | `ref(Pack)` | the id of another instance of that schema |
| nested | `$(Owner)` | an object validated against that schema |

```abstract
schema Owner {
    team: text(1..50)
    contact: text(1..80)
}

schema Collection {
    title: text(1..40)
}

schema Product {
    title: text(1..60)
    replicas: int(1, 3..12)
    price: float(0..9999)
    featured: bool
    status: enum(draft, active, retired)
    manual: file(txt, pdf)
    icon: image(png 32x32)
    collection: ref(Collection)
    owner: $(Owner)
}
```

Notes that catch people out:

- **Ranges need both bounds.** `int(5..)` and `int(..5)` are errors (E305), and
  a reversed range like `int(10..5)` is rejected at the schema, not later at an
  instance. Several parts are allowed and a value satisfies the type when it
  satisfies at least one: `int(1, 3..12)` accepts 1, 3 and 12 but not 2.
- **`int` and `float` do not substitute for each other.** `2` is not a float
  value and `2.0` is not an int value (SPEC 5.10).
- **`image` reads only a header.** The compiler checks the extension, then the
  file's signature and its dimensions, reading at most 64 KiB and never decoding
  pixels. `*` means "any": `image(png 1024x*)`. Supported formats are `png`,
  `jpg`, `gif`, `bmp` and `webp`; `jpeg` canonicalises to `jpg`.
- **`ref` emits a string.** The referenced object is not inlined. The target
  must exist and must use the named schema (E431, E432).
- **`$(Schema)` is a nested object**, validated recursively. It carries no
  `template` and no `id`, because those belong to root objects only.

### 4.2 Lists and cardinality

SPEC 4.6.

```abstract
tags[]: enum(core, public, internal)      // any number, including none
reviewers[1..]: text(1..40)               // at least one
labels[2..4]: enum(core, public, internal)
```

Every element is validated independently. A single value assigned to a list
field is wrapped into a one-element array *after* it validates. Nested lists do
not exist: `name[][]` is E315 in a schema and `[[a]]` is E441 in an instance.

An `@optional` list that is absent is **omitted**, not emitted as `[]`. An
explicitly written `tags: []` is a present, empty list.

### 4.3 Groups and list groups

SPEC 4.7. Example project:
[`examples/features/groups-and-lists`](../examples/features/groups-and-lists).

A **group** is a nested object declared in place. A **list group** is an array
of them:

```abstract
schema Product {
    title: text(1..60)
    owner {
        team: text(1..50)
        contact: text(1..80) @optional
    }
    tags[1..3]: enum(core, public, internal)
    capabilities[] {
        id: enum(search, sync, export) @tag
        availability: enum(alpha, beta, stable) = stable
    }
    notes[]: text @optional
}
```

Use `$(Schema)` when the same shape appears in more than one schema, and a group
when the shape exists only here. They validate identically.

A group is **present** when something wrote a value at or under it. An absent
optional group is omitted entirely, and its children's defaults are not filled.
An absent required group is E411.

### 4.4 `@tag`: keyed lists

SPEC 4.8. `@tag` marks the one field of a group that receives the `#value`
shorthand in instances:

```abstract
capabilities[] {
    id: enum(search, sync, export) @tag
    availability: enum(alpha, beta, stable) = stable
}
```

```abstract
capabilities: [#search, #export(availability: alpha)]
```

```json
"capabilities": [
  { "id": "search", "availability": "stable" },
  { "id": "export", "availability": "alpha" }
]
```

`@tag` is allowed only on a field **inside** a group (E311), at most once per
group (E310), and only on a non-list `text`, `int`, `bool` or `enum` field
(E322). The field need not be called `id`.

A list whose element group declares `@tag` is a **keyed list**: two elements
with the same key are an error (E444), and clone merging folds such lists by key
rather than replacing them.

### 4.5 Defaults and `@optional`

SPEC 4.10.

```abstract
availability: enum(alpha, beta, stable) = stable
price: float(0..9999) = 0.0
featured: bool = false
tags[]: enum(core, public) = [core, public]
```

- No `@optional` and no default means **required**: the field must have a value
  after logic runs, or E411.
- A default is filled in before logic runs, and is validated against its own
  type at schema-validation time, not lazily.
- `@optional` and absent means **omitted** from the output. There is no `null`
  in Abstract, in any format, for any type.
- A field must not carry both `@optional` and a default (E320); the default
  would make the field always present.

Modifiers go after the type and **before** the `=`:

```abstract
glow: bool @since(2) = false      // correct
```

### 4.6 Numbered keys

SPEC 4.9. Example project: [`examples/features/numbered-keys`](../examples/features/numbered-keys).

A field name may be all digits, which is the idiom for fixed positional slots:

```abstract
schema Slot {
    mode: enum(fixed, free) = free
    capacity: int(0..64) @optional
}

schema Pack {
    title: text(1..40)
    slots {
        1: $(Slot)
        2: $(Slot) @optional
        3: $(Slot) @optional
    }
}
```

```abstract
Pack :: @id.winter
    title: Winter
    slots.1.mode: fixed
    slots.1.capacity: 16
    slots.2.capacity: 8
```

```json
"slots": {
  "1": { "mode": "fixed", "capacity": 16 },
  "2": { "mode": "free", "capacity": 8 }
}
```

A numbered key is an ordinary field name, not an array index. In JSON and YAML
it is a string key, so no consumer mistakes it for a number.

### 4.7 `template` and `id`

SPEC 4.11. `template` and `id` are envelope keys. A schema must never declare
`template` (E314) and no statement may assign either (E410).

Every schema has an implicit `id: text(1..64)`. You may declare it explicitly,
at root level, in exactly two spellings, to constrain or document the id:

```abstract
schema Pack {
    id: text(3..24)
    title: text(1..40)
}
```

Anything else on `id`; another type, a list head, any modifier, a default; is
E314. Declaring `id` never changes what the id *is*, and never changes the
output: the id is always emitted once, as the second key.

Inside a group, `id` and `template` are ordinary field names with no envelope
meaning, which is why `id: enum(...) @tag` is the common spelling of a keyed
list.

---

## 5. Writing instances

SPEC 5.

```abstract
Product :: @id.atlas, @status.active
&base_product.*
    title: Atlas Search
    owner.team: Knowledge Systems
```

An instance is a header, then zero or more clone statements, then body
statements and body blocks. Indentation is conventional; it carries no meaning.

### 5.1 The header and header tags

SPEC 5.1, 5.2. A logical line is an instance header **if and only if** its first
token is a schema name and the next token is `::`. Nothing else starts an
instance, so `window: 12::30` is an ordinary assignment.

| Form | Meaning |
|---|---|
| `@name.value` | assigns `value` to the root field `name` |
| `@name` | assigns `true` to the root field `name`, which must be `bool` |

A header tag value runs to the next depth-0 `,` or `@`. Quote it to include one:
`@label."hi, there @world"`. The value is interpreted by the field's declared
type exactly like a body value, so `@count.12` is the number 12 on an `int`
field and the string `"12"` on a `text` field.

A header tag can never address a nested field (E438) and can never be a list, a
`#tag` object or a brace pattern. Write those as body statements.

A long header may wrap after a comma; the one place a trailing comma continues
a line:

```abstract
Product :: @id.atlas,
           @status.active
```

### 5.2 Identity

SPEC 5.3. Every instance has an id:

1. `@id.<value>`, or
2. the **file stem** when the header carries no `@id`.

The id must be a valid identifier and is normalised, so `@id.Winter-Pack` is
`winter_pack` and `@id.42` is the string `"42"`. Ids must be unique across the
whole project, across all templates (E402). One file may declare several
instances, but the stem can serve only one of them, so all but one need `@id`.

### 5.3 Paths, multi-paths and blocks

SPEC 5.4. Example project:
[`examples/features/paths-and-blocks`](../examples/features/paths-and-blocks).

```abstract
Service :: @id.api
    title: API
    limits.{soft, hard}: 10
    owner {
        team: Platform
        contact: platform@example.com
    }
```

```json
{
  "template": "Service",
  "id": "api",
  "title": "API",
  "limits": { "soft": 10, "hard": 10 },
  "owner": { "team": "Platform", "contact": "platform@example.com" }
}
```

- The left side of a statement ends at the first `:` at bracket depth 0; a later
  `:` belongs to the value (`link: https://example.com` is intact).
- A dotted path creates intermediate objects as needed. It may not traverse a
  list field (E443) and there is no index syntax in an assignment.
- `prefix.{a, b}: value` assigns the same value to both paths.
- A **body block** is sugar for repeating a prefix. Its `{` must end its line
  and its `}` must start one; it joins no lines and carries no annotation.
- Writing the same path twice is an error (E429), not last-wins.

### 5.4 Values

SPEC 5.5, 5.10. The parser infers no types. It produces a *syntax value*, and
the declared type of the target field decides how it is read.

**Commas separate list items, always.** At bracket depth 0 a `,` is a list
separator whatever the field's type, so a text value containing a comma must be
quoted:

```abstract
caption: "Hello, world"       // one string
caption: Hello, world         // a two-item list; E412 on a non-list field
```

These three spellings are the same list:

```abstract
tags: [core, public]
tags: core, public
tags: [
    core,
    public
]
```

A trailing comma is allowed **inside** brackets and is an error at depth 0
(E442); a statement never continues onto the next line on a comma. An empty
list is `[]`; an empty item (`a,,b`) is E441.

Three characters are reserved at the start of an unquoted value: `[` always
starts a list, `#` always starts a tagged object, `"` always starts a quoted
string. Quote the value to write one literally: `color: "#ff0000"`.

Quoting never coerces. `count: "5"` on an `int` field is E412, and
`caption: "19.5"` on a `text` field is the string `19.5`. Unquoted
version-like text stays text: `1.21.5` is not a number.

Quoted strings support exactly five escapes; `\"`, `\\`, `\n`, `\r`, `\t`; and
must open and close on the same line. There is no `\u`: write the character
itself. Windows paths are written with `/` or `\\`.

An **unquoted** value takes every Unicode scalar value except a control one: a
C0 control, `U+007F`, or a C1 control such as `U+0085` is E210 naming the
character. Non-ASCII whitespace is not a control character and is ordinary text,
so a non-breaking space inside a caption is part of the caption. A quoted string
takes anything that closes on its own line, and output escapes the whole control
set in all three formats, so what a consumer reads never depends on which
spelling the author chose.

### 5.5 Tagged objects

SPEC 5.5. `#name` fills the group's `@tag` field; `#name(...)` fills more:

```abstract
capabilities: [#search, #export(availability: alpha), #sync(limits.soft: 10)]
```

Arguments are `key: value` or a bare identifier, which sets a `bool` field to
`true`. A key may be a dotted path into the group's own nested groups, and a
value may be a bracketed list or another `#tag` object. A bare comma-separated
list is not available inside an argument list, because the comma separates
arguments. Text after the closing `)` is an error (E210).

### 5.6 Tuple arrays

SPEC 5.5. Example project:
[`examples/features/tables-and-wildcards`](../examples/features/tables-and-wildcards).

A tuple array names its columns once and then lists rows:

```abstract
copy(key, value): (en_us, Welcome), (es_es, Bienvenido)
```

```json
"copy": [
  { "key": "en_us", "value": "Welcome" },
  { "key": "es_es", "value": "Bienvenido" }
]
```

Every row must have exactly as many cells as there are columns (E416). The
target must be a list group or a list of `$(Schema)`. Rows are separated by
depth-0 commas, so the whole assignment is one logical line; wrap the row list in
brackets to spread it over several physical lines:

```abstract
copy(key, value): [
    (en_us, Welcome),
    (es_es, Bienvenido),
]
```

### 5.7 Enum wildcards

SPEC 5.6. A value ending in `*` is a prefix wildcard **when it is interpreted
against an `enum` type**, and expands in the enum's declared order:

```abstract
flags: hat_*                          // an enum list field
copy(key, value): (es_*, Bienvenido)  // a tuple cell whose column is an enum
capabilities: [#s*]                   // a #tag whose tag field is an enum
```

Those three positions are the only ones, because each replicates a whole list
element. A wildcard that matches nothing is an error (E415); it never silently
expands to nothing. Everywhere else a trailing `*` is an ordinary character.

```abstract
schema Page {
    copy[] {
        key: enum(en_us, es_es, es_mx) @tag
        value: text(1..80)
    }
}
```

```abstract
Page :: @id.home
    copy(key, value): (es_*, Bienvenido)
```

```json
"copy": [
  { "key": "es_es", "value": "Bienvenido" },
  { "key": "es_mx", "value": "Bienvenido" }
]
```

### 5.8 Brace file patterns

SPEC 5.8. A value of a `file` or `image` **list** field that contains `{`
expands:

```abstract
images: ./art/{hero,thumb}.png
```

```json
"images": ["./art/hero.png", "./art/thumb.png"]
```

Several groups produce the cartesian product, with the leftmost group varying
slowest. Groups do not nest. Interpolation runs first, so a `{` that arrives
from a variable is a literal character. On any other field type, and in any
quoted value, braces are ordinary characters. A brace pattern on a non-list
asset field is E434.

### 5.9 Interpolation

SPEC 5.11. Example project:
[`examples/features/interpolation`](../examples/features/interpolation).

After clones are merged and the instance's own statements are applied, the
instance's **root scalar fields** form a variable table. `id` is always in it.

| Form | Meaning |
|---|---|
| `$name` | the value of `name`; the name is the maximal run of identifier characters |
| `${name}` | the same, with explicit boundaries |
| `$$` | a literal `$` |

```abstract
Product :: @id.atlas
    title: Atlas
    image: ./textures/$id.png
    label: ${id}_display
    note: "$$99 special"
```

```json
{
  "template": "Product",
  "id": "atlas",
  "title": "Atlas",
  "image": "./textures/atlas.png",
  "label": "atlas_display",
  "note": "$99 special"
}
```

Substitution is a single left-to-right pass; substituted content is never
rescanned, so chained interpolation does not work. An unknown variable is an
error (E425); a misspelling never reaches the output. A `$` followed by
anything but an identifier character, `{` or `$` is E426.

### 5.10 Clones

SPEC 5.7. Example projects: [`examples/features/clones`](../examples/features/clones) and
[`examples/features/clone-merge`](../examples/features/clone-merge).

A clone copies another instance's authored data. Clone statements must appear
**immediately after the header**, before any body statement (E404).

```abstract
Product :: @id.beacon
&atlas.*
    title: Beacon Export
```

| Form | Meaning |
|---|---|
| `&id` / `&id.*` | full clone |
| `&id.path` | partial clone of the subtree at `path` |

Rules worth memorising:

- The source must exist (E405) and must use the **same template** (E407).
- Cycles are an error (E406); chains deeper than 64 are E209.
- A clone copies the source's **authored data after its own clones**; before
  defaults, before interpolation and before logic. A value the source only gets
  from a default or a `derive` is therefore not visible to a partial clone.
- `template` and `id` are never cloned.
- Interpolation runs after cloning, so a cloned `./textures/$id.png` resolves to
  the **cloning** instance's id.

**Merging.** Clones fold into the object in source order. An object merges
recursively, a keyed list merges by key, and anything else replaces. The
instance's own header tags and body statements are then applied by plain
assignment: they replace whatever the clones left there, with no merging, even
for a keyed list. To extend a cloned keyed list rather than replace it, restate
the whole list.

Keyed-list merging in practice:

```abstract
Product :: @id.base_a
    title: Base A
    capabilities: [#search(availability: alpha), #sync]
```

```abstract
Product :: @id.base_b
    title: Base B
    capabilities: [#search(preview), #export]
```

```abstract
Product :: @id.merged
&base_a.*
&base_b.*
```

```json
{
  "template": "Product",
  "id": "merged",
  "title": "Base B",
  "capabilities": [
    { "id": "search", "availability": "alpha", "preview": true },
    { "id": "sync", "availability": "stable" },
    { "id": "export", "availability": "stable" }
  ]
}
```

`title` is a scalar, so the second clone replaces it. `capabilities` is a keyed
list, so `search` merges field by field, `sync` keeps its place, and `export` is
appended.

### 5.11 Strictness

SPEC 5.12. An instance may assign only fields its schema declares. An unknown
field is an error with a suggestion when the edit distance is at most 2, and a
note listing the declared fields. There is no lenient mode and no
`--allow-unknown` in 1.0.

### 5.12 Comments

SPEC 3.2. A comment runs from `//` to the end of the physical line, and starts
only at the start of a line or after a space or tab, and never inside a quoted
string:

```abstract
// a whole-line comment
link: https://example.com          // a trailing comment; the URL is intact
note: "// not a comment"
```

A value that must keep a leading `//` or a spaced ` // ` is quoted. There are no
block comments, and comments never appear in output.

---

## 6. Logic

SPEC 6. Example projects: [`examples/features/logic-basics`](../examples/features/logic-basics) and
[`examples/features/logic-version`](../examples/features/logic-version).

A `logic` block attaches rules to a schema. It runs for every instance of that
schema, and for every nested object validated against it.

```abstract
logic Product {
    derive .shipping_class = standard
    derive? .release_wave = 1

    if .status == "active" {
        require .owner.contact exists
            else throw "Active products need an owner contact."
    } else if .status == "retired" {
        derive .visibility = hidden
    } else {
        derive .visibility = internal
    }

    derive .flag_count = length(.flags)

    require not .flags contains "banned"
        else throw "Banned products cannot ship."
}
```

### 6.1 Statements

| Statement | Effect |
|---|---|
| `derive <path> = <expr>` | writes a value onto the object |
| `derive? <path> = <expr>` | writes only when the path has no value yet |
| `require <cond> else throw "<msg>"` | fails compilation with that message |
| `if … { } else if … { } else { }` | branches |
| `for $x in <iterable> { }` | runs the body once per element |

Only `derive` and `derive?` write. Statements run in source order.

`derive?` is for required fields not yet supplied and for optional fields with
no default. Because defaults are filled **before** logic, a `derive?` on a
defaulted field could never fire, and is rejected (E518).

### 6.2 Paths

SPEC 6.5. `.a.b` starts at the object being evaluated; `$x.a` starts at a loop
variable. A segment may carry a read-only index (`.items[0].id`), and a loop
variable may be a path segment (`.slots.$slot.mode`).

Every literal segment must name a declared field (E503). A field that does not
exist in the version being compiled, an out-of-range index and an absent
optional field all resolve to **no value**, which is never an error.

**Projection.** A path that crosses a list field without an index resolves to
one value per element. A projected path is a legal operand only of `contains`
and `exists`:

```abstract
require not .capabilities.id contains "banned"
    else throw "Banned capabilities are not allowed."
```

`==`, `!=`, `<`, `<=`, `>` and `>=` require operands that resolve to at most one
value; a projecting operand is E519. That is deliberate:
`.caps.id != "banned"` is rejected rather than silently meaning "some element is
not banned".

### 6.3 Conditions

SPEC 6.6, 6.8. Precedence, tightest first: parentheses and calls, postfix
`exists`, the comparison operators, prefix `not` / `!`, `and` / `&&`, `or` /
`||`. So `a or b and c` is `a or (b and c)`, and `not .a == .b` is
`not (.a == .b)`.

- A condition must be **boolean**. There is no truthiness: a bare `text` field
  or a bare `length(...)` is E513. A bare path is a condition only when it names
  a `bool` field.
- Comparisons do not chain: `a < b < c` is E512.
- String comparison is **exact**; case and `-`/`_` matter, and nothing is
  folded. Enum values are stored normalised, so `.status == "active"` is the
  correct spelling and `"Active"` is simply false.
- An operand that resolves to no value makes every comparison false, including
  `!=`. Only `exists` distinguishes absence.
- `<`, `<=`, `>`, `>=` require numbers on both sides. There is no lexicographic
  string ordering and no numeric reading of strings.
- `contains` means membership in a list, or substring in a string.

`exists` is presence in the **compiled data**. It never touches the filesystem,
for any type, in any mode. On-disk asset checks belong to the schema, which is
what makes compiled data independent of `--skip-assets`.

### 6.4 `length` and `version`

`length(<path>)` counts elements for a list, Unicode scalar
values for a string, the number of projected values for a projection, and `0`
for a path with no value. Its argument's *declared* type must be a list or
`text`, checked before any instance is read (E514).

`version` is the only built-in value: the integer version being compiled. It is
spelled bare, with no `.` and no `$`, so it never collides with a field named
`version` (which is `.version`) or a loop variable (`$version`).

```abstract
logic Item {
    derive .schema_version = version

    if version >= 2 {
        derive .glow = true
    }
}
```

### 6.5 Loops and variables

The only variables are loop variables, always written `$name`. They are in scope
inside their own block, may not shadow an enclosing loop (E516), and may be used
as operands, as dynamic path segments, in `derive` values and in `throw`
messages. A quoted string is never a variable: `"$kind"` is five characters.

`for` iterates a list field or a literal list. Iterating an absent or empty list
runs the body zero times and is not an error.

### 6.6 What `derive` may and may not do

SPEC 6.13.

| Target | Allowed |
|---|---|
| a declared scalar field, or one inside a group or `$(Schema)` | yes |
| a list field, as a whole value | yes |
| one element of a list (`.items[0].x`) | no (E506) |
| `.template`, `.id` | no (E504) |
| a field the schema does not declare | no (E505) |
| a field absent in the version being compiled | no (E521, when the statement executes) |
| a path containing a loop variable | no (E520) |

The right-hand side may be a literal, a string with interpolation, another path
of the same object, `length(...)`, `version`, a loop variable, or an explicit
`calc(...)` calculation. See [arithmetic](reference/arithmetic.md) and the
[invoice exercise](../examples/exercises/06-invoice-arithmetic/README.md).
A path keeps
the **type** of the value it resolved to. A path that projects several values is
E519.

Abstract has no accumulation operator and no list append: a list is assigned in
the instance, or derived once from a single expression.

### 6.7 Order of evaluation

SPEC 6.11. For one object and one version:

1. authored values are type-checked;
2. defaults are filled;
3. logic runs; nested schema logic first, then this schema's block;
4. required fields are checked (so a `derive` may satisfy one);
5. the whole object is re-validated from scratch, so nothing logic wrote escapes
   validation.

A failed `require` aborts that instance immediately with E515 and your own
message, and compilation fails.

---

## 7. Versions and overlays

SPEC 4.12, 5.13, 5.14, 7.4, 7.5. Example projects:
[`examples/features/versions-overlays`](../examples/features/versions-overlays) and
[`examples/features/instance-windows`](../examples/features/instance-windows).

One project carries every version of its data. A project declares its range once,
in any `.abt` file:

```abstract
versions 1..3
```

Absent, the range is `1..1`.

### 7.1 Scoping fields, statements and instances

`@since(n)` means "exists in versions `v >= n`". `@removed(n)` means "exists in
versions `v < n`". They may be combined, and they attach to exactly three
places:

```abstract
schema Item {
    name: text(1..40)
    glow: bool @since(2) = false          // a field
    legacy_tint: int(0..255) @removed(3) @optional
}
```

```abstract
Item :: @id.torch
    name: Torch
    glow: true               @since(2)    // a statement
    legacy_tint: 200
```

```abstract
Item :: @id.lantern @since(2)             // a whole instance
    name: Lantern
```

A statement needs an annotation only when it must differ from its field's own
existence set: the assignment to `legacy_tint` above needs none, because the
field itself exists only in versions 1 and 2. Two statements writing one path
must have disjoint applicability sets; that is the supported way to give a field
different values in different versions.

Annotations are never allowed on a header tag, a clone statement or a body block
(E437).

### 7.2 Every version must compile

An instance is compiled once for every version in its window, and every check
runs in every version. A required field must have a value in every version in
which it exists. Logic that reads a version-scoped field must guard it, because
an operand resolving to no value makes every comparison false:

```abstract
if .glow exists {
    require .glow == false else throw "glow must start off."
}
```

Logic that *writes* one must guard it too, because a `derive` that executes
against a field absent in the version being compiled is E521:

```abstract
if version < 3 {
    derive? .legacy_tint = 0
}
```

Assets are not versioned. Versions scope schema fields, statements and whole
instances, and nothing else.

### 7.3 What the output looks like

The compiled document carries the **maximum** version in `data`, and every
earlier version that differs as an entry in `overlays`. Entries are whole
replacement objects and removed ids; there is no field-level delta and no
`null`.

```abstract
versions 1..3

schema Item {
    name: text(1..40)
    glow: bool @since(2) = false
    legacy_tint: int(0..255) @removed(3) @optional
}
```

```abstract
Item :: @id.torch
    name: Torch
    legacy_tint: 200
```

```json
"data": [
  { "template": "Item", "id": "torch", "name": "Torch", "glow": false }
],
"overlays": [
  {
    "versions": { "min": 1, "max": 1 },
    "data": [
      { "template": "Item", "id": "torch", "name": "Torch", "legacy_tint": 200 }
    ],
    "removed": []
  },
  {
    "versions": { "min": 2, "max": 2 },
    "data": [
      {
        "template": "Item",
        "id": "torch",
        "name": "Torch",
        "glow": false,
        "legacy_tint": 200
      }
    ],
    "removed": []
  }
]
```

To read version `v`, a consumer starts from `data` and applies **every** overlay
whose range contains `v`: replace or add each object by `id`, then delete the
ids in `removed`. The ranges of the overlays that mention any one id are
disjoint, so nothing is touched twice and the order does not matter. See
[compiled data](reference/compiled-data.md) for the full contract.

---

## 8. Output

SPEC 8. Full detail is in [compiled data](reference/compiled-data.md).

Every document has three top-level keys in this order: `abstract`, `data`,
`overlays`. Instance objects begin with `template` and `id`, then the schema's
fields in **declaration order**. Instances are ordered by `(template, id)`, so
the output never depends on file layout or command-line order.

Three formats:

- **JSON**; two-space indent, one member per line, exactly one trailing `LF`.
- **YAML**; the same tree in block style, with **every mapping key quoted**.
- **RAW** (`.abraw`); a review format with four-space indent and bare keys, for
  diffs and code review. It is not machine-parsed and no Abstract tool reads it
  back.

Absent optional fields are omitted in every format, including lists. Abstract
has no null.

---

## 9. The command line

SPEC 9.

```text
abstract compile <path>… [JSON|YML|YAML|RAW] [--out <file>] [--skip-assets] [--max-errors <n>]
abstract lint <path>… [--skip-assets] [--max-errors <n>]
abstract templates <path>
abstract init <directory>
abstract --help
abstract --version
```

- `compile` takes exactly one directory, or one or more `.ab` files. Mixing them
  is E807. The default format is JSON.
- `lint` does exactly the same work and renders nothing; on success it writes
  `abstract: ok` to stderr.
- `templates` lists the schema names of a project, one per line, sorted.
- `--out` writes the document to a file instead of stdout, and refuses to
  overwrite a source file or to write inside the data directory (E808).
- `--skip-assets` skips exactly the on-disk asset checks. The compiled bytes are
  identical with and without it.
- `--max-errors` bounds how many diagnostics are reported. It takes an integer
  from 1 to 10000 and defaults to 20; there is no "no limit" spelling, and any
  other value is E812. It never changes which diagnostic comes first.

The argument parser is strict: unknown commands, flags, positionals and format
keywords are all errors. stdout carries only the document; every diagnostic goes
to stderr. Exit codes are 0 success, 1 compilation failure, 2 usage error, 3
output could not be written.

Diagnostics look like this:

```text
data/templates/Collection.abt:32:9: error[E515]: Option logic: Premium options must carry the promo tag.
  note: data/options/ember.ab:1:1: instance 'ember' declared here.
  note: compiling version 1.
```

Paths are project-relative with `/` separators; line and column are 1-based and
the column counts Unicode scalar values. Every identifier is listed in SPEC 10
and is stable within its wire contract.

---

## 10. Where to go next

- [Specification](reference/specification.md); the normative specification, including the complete
  diagnostics catalogue (section 10) and the worked example (Appendix C).
- [Grammar](reference/grammar.ebnf); the complete grammar.
- [Syntax reference](reference/syntax.md); a lookup-oriented
  reference for `.ab` and `.abt` files.
- [Compiled data](reference/compiled-data.md); the compiled-document contract for consumers.
- [AI authoring guide](ai/README.md); rules and examples for generating and
  validating `.ab` and `.abt` files.
- [Feature examples](../examples/features/README.md); every example in these documents as a
  compilable project.
