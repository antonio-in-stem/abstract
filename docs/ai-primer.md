# Writing correct Abstract: a primer for AI collaborators

This page is written for an assistant or agent that must produce Abstract 1.0
source that compiles on the first attempt. It is a rule sheet, not a tutorial.
For the narrative version read [`abstract-language.md`](abstract-language.md);
the normative text is [`SPEC.md`](SPEC.md) and [`GRAMMAR.ebnf`](GRAMMAR.ebnf).

Everything here is checkable: the worked examples are complete projects under
[`examples/`](examples/README.md), stored with the exact bytes the compiler
emits.

---

## 1. Before writing anything

1. **Read the schemas first.** Abstract is strict: an instance may assign only
   fields its schema declares (E409), and there is no lenient mode. Never invent
   a field, an enum member or a schema name.
2. **Find the project root.** It is the parent of the nearest ancestor directory
   named `data`, or the compiled directory when the project has no `data/`.
   Assets live in `<project root>/assets`.
3. **Check the version range.** If any `.abt` declares `versions min..max`, every
   version in that range must compile, not just the newest.
4. **Never write `template:` or `id:` as a field or an assignment.** They are
   envelope keys the compiler writes (E314, E410). The id comes from `@id.x` in
   the header, or from the file name.
5. **Validate before claiming success.** Run `abstract lint <path>`; run
   `abstract compile <path> JSON` when the output matters. Report the exact
   diagnostic if it fails; do not paraphrase it. To see more than the first
   twenty diagnostics, pass `--max-errors <n>` with `n` between 1 and 10000;
   there is no "no limit" spelling, and any other value is E812.

---

## 2. The rules that cause most first-attempt failures

### 2.1 Commas always separate list items

At bracket depth 0 a `,` is a list separator, whatever the field's declared type.
The parser has no type information at that point.

```abstract
caption: "Hello, world"      // correct: one string
caption: Hello, world        // a two-item list; E412 on a non-list field
```

A trailing comma at depth 0 is E442 — a statement never continues onto the next
line on a comma. Wrap the list in brackets to span lines:

```abstract
tags: [
    core,
    public
]
```

### 2.2 Quoting never coerces

| Field type | Correct | Wrong |
|---|---|---|
| `int` | `count: 5` | `count: "5"` → E412 |
| `float` | `price: 19.5` | `price: "19.5"` → E412 |
| `bool` | `featured: true` | `featured: "true"` → E412 |
| `text` | `caption: "19.5"` | — the string `19.5` is what you get |

There is no coercion in either direction, and no `0`/`1`/`yes`/`no` for `bool`.

### 2.3 `int` and `float` are different types

`2` is not a `float` value and `2.0` is not an `int` value. A float literal needs
digits on both sides of the `.`: `.5` and `5.` are not literals. Unquoted
version-like text such as `1.21.5` is not a number at all, and on a `float` field
it is E412.

### 2.4 Three characters are reserved at the start of a value

| First character | Meaning | To write it literally |
|---|---|---|
| `[` | starts a list | quote the value |
| `#` | starts a tagged object | `color: "#ff0000"` |
| `"` | starts a quoted string | quote the whole value |

Text after a closing quote is E210. Nothing is silently discarded.

### 2.5 Escapes and paths

A quoted string supports exactly five escapes: `\"`, `\\`, `\n`, `\r`, `\t`.
Anything else after `\` is E202, and there is no `\u`. Write a Windows path with
`/` or with `\\`. Prefer `/` everywhere.

Asset values are relative to `assets/`, must not contain `..`, and must not
start with a drive letter, a UNC prefix or a slash (E424). Do **not** write the
`assets/` prefix: `./textures/a.png` already means `assets/textures/a.png`,
while `./assets/textures/a.png` means `assets/assets/textures/a.png`.

### 2.6 Comments can eat a value

`//` starts a comment at the start of a line or after a space or tab, and never
inside a quoted string.

```abstract
link: https://example.com    // fine: the URL has no spaced //
cdn: "//cdn.example.com/x"   // must be quoted: a value cannot begin with //
ratio: "50 // 2"             // must be quoted: a spaced // would truncate it
```

The leading-`//` rule holds wherever the value begins, and not only where a
comment would have started. `cdn://cdn.example.com/x` — no space after the `:` —
is E210 too, so that moving a value onto its own line or adding a space never
changes what the file means.

### 2.6b Bare values take every character except a control character

An unquoted value keeps its exact characters and admits every Unicode scalar
value except a C0 control, `U+007F` and a C1 control (`U+0085` included). One of
those inside a value is E210 naming it, and **non-ASCII whitespace is ordinary
text**: `U+00A0` inside a caption is part of the caption, not a separator. A
control character an author really needs goes inside a quoted string, which
takes anything that closes on its own line; `\t`, `\n` and `\r` are the three
that have escapes. Output escapes the whole set in every format, so what a
consumer reads is the same either way.

### 2.7 Every path is written once

Writing the same path twice in versions that overlap is E429, not last-wins.
A header tag and a body statement writing the same field also collide. Two
statements with **disjoint** version annotations are the supported way to give
one field different values in different versions.

### 2.8 Clones go directly under the header

```abstract
Product :: @id.beacon
&atlas.*
    title: Beacon Export
```

Anywhere else, a `&` line is E404. The source must exist (E405) and must use the
same template (E407). A clone copies the source's **authored** data — before
defaults, before interpolation, before logic — so a value the source only gets
from a default or a `derive` is not visible to a partial clone.

The instance's own statements replace what a clone left, with no merging, even
for a keyed list. To extend a cloned keyed list, restate the whole list.

### 2.9 Logic paths start with `.`

A bare word in a condition is not a field. `.status == "active"` is a field
comparison; `status == "active"` is E511/E503.

String comparison is **exact**: no case folding, no `-`/`_` folding. Enum values
are stored normalised, so compare against lowercase, `_`-separated text.
`.status == "Active"` is simply false.

There is no truthiness. A condition must be boolean; a bare `text` path or a
bare `length(...)` is E513.

### 2.10 Absent values make comparisons false

An absent optional field, a field that does not exist in the version being
compiled, an out-of-range index, or a path through an empty list all resolve to
**no value**, and every comparison involving one is false — including `!=`. Only
`exists` distinguishes absence:

```abstract
if .glow exists {
    require .glow == false else throw "glow must start off."
}
```

### 2.11 A projected path only works with `contains` and `exists`

A path that crosses a list field without an index resolves to one value per
element. Using it with `==`, `!=` or an ordering operator is E519.

```abstract
require not .capabilities.id contains "banned"     // correct
require .capabilities.id != "banned"               // E519
```

### 2.12 `derive?` never applies to a defaulted field

Defaults are filled **before** logic runs, so a `derive?` on a field that
declares a default could never fire, and is rejected outright (E518). Use
`derive?` for required fields not yet supplied and for optional fields with no
default.

### 2.13 Version-scoped writes must be guarded

A `derive` that executes against a field which does not exist in the version
being compiled is E521. Guard it:

```abstract
if version >= 2 {
    derive .glow = true
}
```

A statement in a branch that is not taken never executes and never raises E521.

### 2.13b Every annotation obeys the same two rules, on every construct

`@since(n)` and `@removed(n)` follow one rule wherever they appear — on a field,
on a body statement, at the end of an instance header. `n` must lie inside the
project's `versions` range (**E603**), and `@removed(n)` must be greater than the
effective `@since` (**E604**). Both are checked before anything asks which
versions the annotated thing applies to, so `glow: true @since(3) @removed(2)`
is E604 and never a confusing E430.

An assignment that ends up applying to **no** version is an error rather than a
no-op, and that includes a header tag, which is an assignment written compactly:

```abstract
Item :: @id.x, @glow @removed(2)     // E440 if glow is @since(2)
```

The tag targets a field that exists in versions 2..3 while the instance exists
only in version 1, so it would write nothing anywhere. Writing the same thing as
the body statement `glow: true` gets exactly the same E440. Nothing in the
language is silent.

### 2.14 Optional lists are omitted, not empty

An `@optional` list that is absent does not appear in the output at all. It does
not become `[]`. An explicit `tags: []` is a present, empty list. There is no
`null` in Abstract, in any format, for any type.

### 2.15 Wildcards live in exactly three places

`prefix*` expands against an `enum` vocabulary only as: an element of an enum
**list** field, a tuple cell whose column is an enum, or the name of a `#tag`
whose tag field is an enum. Everywhere else a trailing `*` is an ordinary
character. A wildcard that matches nothing is E415.

### 2.16 Brace patterns live in exactly one place

`{a,b}` expands only in a value of a `file` or `image` **list** field. On a
non-list asset field it is E434; on any other type the braces are literal
characters. Interpolation runs first, so a `{` that came from a variable never
delimits a group.

Braces still have to **balance**, whatever the field's type. `icon: a}.{b` is
E205 at the `}` and E203 at the `{` even on a `text` field, because the balance
rule is lexical and runs before any type is consulted. Write `icon: "a}.{b"` for
a literal, unbalanced brace. One pattern holds at most **eight** groups; a ninth
is E435, as are an empty alternative (`a{b,}.png`) and a nested group.

---

## 3. Every reserved word

Keywords are recognised only in the position where they are meaningful. None of
them is reserved as a field name, an enum member or an instance id (SPEC B.3).

| Position | Words |
|---|---|
| top level of a `.abt` file | `schema`, `logic`, `versions` |
| type position | `text`, `int`, `float`, `bool`, `enum`, `file`, `image`, `ref` |
| modifier position | `@optional`, `@tag`, `@since`, `@removed` |
| logic statement position | `derive`, `derive?`, `require`, `if`, `else`, `for` |
| logic expression position | `in`, `not`, `and`, `or`, `contains`, `exists`, `length`, `version`, `true`, `false` |

Operator spellings: `==` `!=` `<` `<=` `>` `>=` `&&` `||` `!`

Punctuation with meaning: `:` `::` `.` `,` `=` `(` `)` `[` `]` `{` `}` `[]` `@`
`#` `&` `$` `${` `$$` `*` `?` `..` `//`

**Genuinely reserved names.** Only two, and only at the root of an instance
object (SPEC B.1):

| Name | Rule |
|---|---|
| `template` | never declarable as a field (E314); never assignable (E410) |
| `id` | declarable only as `id: text` or `id: text(a..b)` with no modifiers and no default; never assignable (E410); implicit declaration is `id: text(1..64)` |

Inside a group or a `$(Schema)` object both are ordinary field names, because a
nested object carries no envelope.

**Document keys are not reserved** in the source language (SPEC B.2). A schema
may declare a field named `abstract`, `format`, `compiler`, `versions`, `data`,
`overlays`, `removed`, `min` or `max`: instance objects are nested inside `data`
and never collide with the envelope.

**Image formats:** `png`, `jpg`, `gif`, `bmp`, `webp`. `jpeg` canonicalises to
`jpg`; every diagnostic spells it `jpg`.

**Output format keywords on the command line:** `JSON`, `YML`, `YAML`, `RAW`.

---

## 4. Identifier rules

| Kind | Grammar | Normalised | Case-sensitive |
|---|---|---|---|
| field name, enum member, tag name, path segment, tuple column, multi-path key | `identifier` | yes | no |
| instance id, clone target, `ref` value | `identifier` | yes | no |
| schema name | `[A-Za-z][A-Za-z0-9_]*` | no | **yes** |
| loop variable | `$` + `identifier` | yes | no |
| file extension | letters and digits | canonicalised | no |

An `identifier` is `[A-Za-z0-9_][A-Za-z0-9_-]*` that does not end with `-`.
Normalisation lowercases ASCII letters and maps `-` to `_`, so `Max-Count`,
`max_count` and `MAX_COUNT` are one name. Purely numeric identifiers are legal
and are ordinary names, not indices.

The three rules carry three different diagnostics, and they are not
interchangeable. A character outside `[A-Za-z0-9_-]` is **E206** — non-ASCII
characters are never identifier characters. A name that *ends* with `-` is
**E207**. A name that *begins* with `-` is **E210** at the `-`: a hyphen is a
legal identifier character standing in a position it may not stand in, which is
a different fault from a character that may not appear at all.

Schema names are compared **exactly**: `Product` and `product` are two different
names.

---

## 5. Checklist before submitting a change

**Schemas (`.abt`)**

- [ ] Every schema name is unique project-wide and matches
      `[A-Za-z][A-Za-z0-9_]*`.
- [ ] No field is named `template`; `id`, if declared, is exactly `id: text` or
      `id: text(a..b)` with no modifiers and no default.
- [ ] No two fields in one block normalise to the same name.
- [ ] Every range has both bounds and the lower bound is not greater than the
      upper.
- [ ] Modifiers come after the type and before any `=`.
- [ ] No field carries both `@optional` and a default.
- [ ] `@tag` is inside a group, at most once per group, on a non-list `text`,
      `int`, `bool` or `enum` field.
- [ ] Every `$(Schema)` and `ref(Schema)` names a declared schema.
- [ ] Every default is valid for its own field's type.
- [ ] Every list cardinality is `[]`, `[min..]` or `[min..max]` with
      `0 <= min <= max`.

**Instances (`.ab`)**

- [ ] Every header names a declared schema and has exactly one `::`.
- [ ] Every id is a valid identifier and is unique project-wide, after
      normalisation.
- [ ] Clone statements sit directly under the header, before any assignment.
- [ ] No path is written twice in overlapping versions.
- [ ] Text values containing `,`, a leading `[`, `#` or `"`, a leading `//` or a
      spaced ` // ` are quoted.
- [ ] No value is quoted for an `int`, `float` or `bool` field.
- [ ] Asset paths are relative, contain no `..`, and do not begin with
      `assets/`.
- [ ] Every `$name` names a root scalar field of the same instance, or `id`.
- [ ] Every required field has a value in **every** version in which it exists.

**Logic (`.abt`)**

- [ ] Every `logic` block names a declared schema, and no schema has two.
- [ ] Every path operand starts with `.` or with a loop variable.
- [ ] Every condition is boolean; no bare paths except `bool` fields.
- [ ] Projected paths are used only with `contains` and `exists`.
- [ ] Every `require` has an `else throw "…"` with a quoted message. It may sit
      on its own line, and blank lines and comments may come between; only
      another token in between breaks the pair.
- [ ] `derive?` targets no field that declares a default.
- [ ] Every `derive` on a version-scoped field is guarded by `version` or by a
      condition that cannot be true in a version where the field is absent.
- [ ] `length()` is applied only to list or `text` values.
- [ ] No nested loop reuses an enclosing loop variable name.

**Then**

- [ ] `abstract lint <project>` exits 0.
- [ ] `abstract compile <project> JSON` produces the expected document.

---

## 6. Worked example: read the schema, then write the instance

Project [`examples/logic-basics`](examples/logic-basics).

`data/templates/Product.abt`

```abstract
schema Product {
    title: text(1..60)
    status: enum(draft, active, retired)
    visibility: enum(public, hidden, internal) @optional
    shipping_class: enum(standard, express) @optional
    release_wave: int(1..9) @optional
    flag_count: int(0..99) @optional
    flags[]: enum(core, promo, banned) @optional
    owner {
        team: text(1..50)
        contact: text(1..80) @optional
    }
}

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

What the schema tells you before you write a line:

- `title`, `status` and the `owner` group are **required**; everything else is
  optional and will be omitted if absent.
- `owner` is a required group, so at least `owner.team` must be written.
- `status` is an enum: only `draft`, `active` or `retired`, in any case, emitted
  lowercase.
- Logic will fill `shipping_class`, `release_wave` and `flag_count`; do not
  write them yourself unless you mean to override.
- If you write `status: active` you **must** also supply `owner.contact`, or the
  `require` fails.

`data/products/atlas.ab`

```abstract
Product :: @id.atlas, @status.active
    title: Atlas Search
    flags: core, promo
    owner.team: Knowledge Systems
    owner.contact: systems@example.com
```

`data/products/relic.ab`

```abstract
Product :: @id.relic, @status.retired
    title: Relic
    owner.team: Knowledge Systems
```

Compiled:

```json
"data": [
  {
    "template": "Product",
    "id": "atlas",
    "title": "Atlas Search",
    "status": "active",
    "shipping_class": "standard",
    "release_wave": 1,
    "flag_count": 2,
    "flags": ["core", "promo"],
    "owner": {
      "team": "Knowledge Systems",
      "contact": "systems@example.com"
    }
  },
  {
    "template": "Product",
    "id": "relic",
    "title": "Relic",
    "status": "retired",
    "visibility": "hidden",
    "shipping_class": "standard",
    "release_wave": 1,
    "flag_count": 0,
    "owner": { "team": "Knowledge Systems" }
  }
]
```

Read the output carefully: `visibility` is absent from `atlas` because the
`active` branch does not derive it, `flags` is absent from `relic` because it was
never written, and key order follows the **schema**, not the order the
assignments were made.

---

## 7. Worked example: a versioned project

Project [`examples/versions-overlays`](examples/versions-overlays).

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

Reasoning an agent should do here:

- The project compiles versions 1, 2 and 3. Every check runs in each.
- `glow` exists in versions 2 and 3; it has a default, so no statement is needed.
- `legacy_tint` exists in versions 1 and 2. The statement that assigns it needs
  **no** annotation: a statement's applicability set is clipped to the field's
  existence set automatically.
- `data` is version 3. Versions 1 and 2 each differ from it, so each becomes its
  own overlay.

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

If you had instead written `legacy_tint: 200 @since(3)`, that would be E430:
the statement contradicts the field, which does not exist in version 3.

---

## 8. Worked example: the full feature set

Project [`examples/sticker-pack`](examples/sticker-pack), which is SPEC
Appendix C. It exercises an explicit `id` declaration, groups, a `ref`, a keyed
list, defaults, versions, an instance window, a clone, interpolation, wildcards,
tuple arrays, logic and overlays.

`data/templates/Pack.abt`

```abstract
versions 1..2

schema Pack {
    id: text(3..24)
    title: text(1..40)
    tier: enum(free, plus) = free
    slot_count: int(1..9) @optional
}

schema Sticker {
    pack: ref(Pack)
    title: text(1..60)
    icon: image(png 128x128)
    rarity: enum(common, rare, epic) = common
    glow: bool @since(2) = false
    tint: int(0..255) @optional @removed(2)
    owner {
        team: text(1..40)
        contact: text(1..60) @optional
    }
    tags[]: enum(core_ui, core_game, promo) @optional
    copy[] {
        key: enum(en_us, es_es, es_mx) @tag
        value: text(1..80)
    }
}

logic Sticker {
    derive? .owner.contact = support@example.com

    if .rarity == "epic" {
        require .tags contains "promo"
            else throw "Epic stickers must carry the promo tag."
    }

    for $lang in [es_es, es_mx] {
        require .copy.key contains $lang
            else throw "Missing $lang copy."
    }
}
```

`data/stickers/frost.ab`

```abstract
Sticker :: @id.frost, @rarity.rare
    pack: winter_2026
    title: Frost
    icon: ./textures/$id.png
    tint: 200
    owner.team: Studio A
    tags: core_*
    copy(key, value): (en_us, Frost), (es_*, Escarcha)
```

`data/stickers/ember.ab`

```abstract
Sticker :: @id.ember, @rarity.epic
&frost.*
    title: Ember
    tags: [core_ui, promo]
    copy(key, value): (en_us, Ember), (es_*, Brasa)
```

Every non-obvious step, in order:

1. `winter_2026` and `spring_2026` take their ids from their file stems, and
   both satisfy the declared `id: text(3..24)`.
2. `@since(2)` at the end of `spring_2026`'s header is an instance **window**,
   not a header tag, because the identifier is followed immediately by `(`.
3. `frost`'s `icon` interpolates `$id` to `frost`, resolving to
   `assets/textures/frost.png`, which must be a 128x128 PNG.
4. `core_*` expands to `core_ui, core_game` in the enum's declared order.
5. `es_*` clones the tuple row into `es_es` and `es_mx`.
6. `tint` exists only in version 1, so the statement that assigns it applies
   only there — no annotation needed.
7. `ember` clones frost's **authored** data, including the raw text
   `./textures/$id.png`. Interpolation runs after cloning, so ember's icon
   becomes `./textures/ember.png`.
8. ember's own header and statements then override `rarity`, `title`, `tags`
   and `copy`.
9. `derive? .owner.contact` fires for both stickers, because `contact` is
   optional with no default.
10. The base is version 2. `frost` and `ember` differ in version 1 (they carry
    `tint` and lack `glow`), and `spring_2026` does not exist in version 1 while
    the base carries it — so all three share one overlay over `1..1`, with
    `spring_2026` in its `removed` list.

The complete expected output is in
[`examples/sticker-pack/expected.json`](examples/sticker-pack/expected.json).

---

## 9. Diagnostic to fix

The full catalogue is SPEC 10. These are the ones an agent meets most often.

| Error | What it means | Fix |
|---|---|---|
| E210 | a token or construct is not valid here | look at the exact column; usually a stray character or an empty value |
| E401 | unknown template in a header | use the declared schema name, matching case exactly |
| E402 | duplicate instance id | ids are normalised and project-wide; rename one |
| E404 | a clone is not directly under the header | move every `&` line up |
| E409 | unknown field | read the `declared fields` note; do not invent fields |
| E411 | required field missing | assign it, give it a default, or mark it `@optional` |
| E412 | value shape does not match the type | usually a quoted number, or a comma that made a list |
| E413 | value outside the declared range | check the schema's range, or the id length |
| E414 | not an enum member | use a declared member; case does not matter, spelling does |
| E415 | wildcard matched nothing | check the prefix against the enum's members |
| E424 | asset path escapes `assets/` | make it relative, drop `..`, drop the `assets/` prefix |
| E425 | unknown interpolation variable | the variables are this instance's root scalar fields, plus `id` |
| E429 | one path written twice | delete one, or give the two statements disjoint version ranges |
| E430 | a statement can never apply | the annotation contradicts the field's own existence set |
| E440 | a statement or clone falls outside the instance's window | widen the window or drop the annotation |
| E442 | trailing comma at depth 0 | wrap the list in `[ … ]` |
| E445 | list cardinality violated | count the elements against the declared bounds |
| E503 | a logic path names no declared field | paths start with `.`; check the spelling |
| E513 | operand type not allowed | there is no truthiness and no string ordering |
| E518 | `derive?` on a defaulted field | use `derive`, or remove the default |
| E519 | an operand can project several values | use `contains` |
| E521 | a `derive` wrote a field absent in this version | guard it with `if version >= n` |
| E523 | the logic asked for more than 1 000 000 loop iterations for one instance in one version | flatten the nested `for` blocks, or iterate a shorter list |

---

## 10. Things Abstract deliberately does not have

Do not reach for these; they do not exist and never will in 1.x.

- imports, includes, `use`
- free-key maps, dynamic object keys
- user-defined functions, arithmetic, string concatenation operators
- nested lists
- a null literal
- comments in the compiled output
- output key renaming (`@as`)
- a lenient mode (`--allow-unknown`)
- per-schema version axes
- non-ASCII identifiers
- list append or accumulation in logic

When a task seems to need one of them, the answer is a different schema shape:
a keyed list instead of a map, two named fields instead of a dynamic key, a
clone instead of an import, a `derive` from a single expression instead of an
accumulator.
