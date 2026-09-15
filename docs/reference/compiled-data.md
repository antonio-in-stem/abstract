# Compiled data: the consumer contract

This document describes what `abstract compile` produces and what a program
reading that output may rely on. It covers the document envelope, instance
objects, key ordering, the three renderings (JSON, YAML, RAW) and the version
overlays.

The normative text is [the specification](specification.md) sections 2.7, 7.5 and 8. Where this
page and the specification disagree, the specification wins.

Runnable projects for every example here are in
[`examples/features/`](../../examples/features/README.md): [`formats`](../../examples/features/formats) for the three
renderings, [`versions-overlays`](../../examples/features/versions-overlays) and
[`instance-windows`](../../examples/features/instance-windows) for the overlay rules.

---

## 1. The envelope

SPEC 8.1. Every compiled document has exactly three top-level keys, in this
order:

| Key | Value |
|---|---|
| `abstract` | metadata about the document itself |
| `data` | the compiled instance objects for the **maximum** version |
| `overlays` | the differences for every earlier version, possibly empty |

`abstract` has exactly three keys, in this order:

| Key | Value |
|---|---|
| `format` | the document format number, exactly `1`, as a JSON number |
| `compiler` | the semantic version of the implementation, as a string |
| `versions` | an object with `min` and `max`, in that order |

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
  "data": [],
  "overlays": []
}
```

`format` changes only when the shape of the document changes: a consumer that
understands format 1 understands every document a 1.x compiler produces.
`compiler` identifies the producer and is the only part of a document that is
not a function of the source bytes; which is why the conformance suite
normalises it before comparing (SPEC 11.2).

An empty `data` array is valid in both project and single-file mode when the
sources declare no instances. A project with no source files at all is E103.

---

## 2. Instance objects

SPEC 8.2, 8.3. Every object in `data` begins with:

| Key | Value |
|---|---|
| `template` | the schema name from the header, exactly as declared |
| `id` | the normalised instance id |

then the schema's fields **in declaration order**, skipping absent fields and
skipping a declared `id` field, which was already emitted as the second key.
Groups and `$(Schema)` values order their own keys by their own declaration
order, recursively.

There are no other keys. Unknown fields cannot reach the output, because an
instance may only assign fields its schema declares (SPEC 5.12).

Key order therefore never depends on assignment order, clone order, or the order
in which logic wrote values. It is a property of the schema alone.

### Document order

SPEC 2.7. `data` and every `overlays[].data` are ordered by the pair
`(template, id)`:

1. by `template`, comparing the schema name as a sequence of Unicode scalar
   values;
2. then by `id`, comparing the normalised id the same way.

Ids are unique project-wide, so the order is total. It does not depend on file
layout, directory names, command-line order or filesystem enumeration order.

---

## 3. Absent values

SPEC 8.9. **Abstract has no null.** No format ever emits `null`, `~` or an empty
value for a field.

- An absent optional field is **omitted**, in every format, for every type,
  including list types. An optional list that was never assigned does not become
  `[]`.
- An empty list is a present value spelled `[]`.
- A group or `$(Schema)` value is present when at least one of its own fields
  has a value; otherwise it is omitted (or E411 when it is required).

A consumer distinguishes "absent" from "empty" by key presence. Because a group
with no values is omitted rather than emitted as `{}`, an empty object never
appears anywhere in a conforming document.

---

## 4. The three renderings

The same tree is rendered three ways. Below is the project
[`examples/features/formats`](../../examples/features/formats):

```abstract
schema Product {
    price: float(0..9999)
    featured: bool
    tags[]: enum(core, public, internal)
    capabilities[] {
        id: enum(search, sync) @tag
        availability: enum(alpha, beta, stable) = stable
    }
}
```

```abstract
Product :: @id.atlas
    price: 19.5
    featured: true
    tags: [core, public]
    capabilities: [#search]
```

### 4.1 JSON

SPEC 8.4. The default format, and the one machines should read.

- Two spaces of indentation per level.
- A non-empty object or array puts each member on its own line; the closing
  bracket sits at the parent's indentation.
- An empty object renders `{}` and an empty array `[]`, on one line.
- The separator between a key and its value is `": "`.
- The document ends with exactly one `LF`. No BOM, no trailing whitespace.

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
      "template": "Product",
      "id": "atlas",
      "price": 19.5,
      "featured": true,
      "tags": [
        "core",
        "public"
      ],
      "capabilities": [
        {
          "id": "search",
          "availability": "stable"
        }
      ]
    }
  ],
  "overlays": []
}
```

### 4.2 YAML

SPEC 8.5. A block-style rendering of the same tree.

- Two spaces of indentation per level.
- **Every mapping key is double-quoted**, always; including `data`, `id` and
  numeric field names. This is what makes a field named `1`, `yes`, `no`, `on`
  or `null` unambiguous.
- Scalar strings are double-quoted and escaped as in section 5; numbers and
  booleans are bare.
- A non-empty sequence is written as block items indented two spaces past its
  key, each beginning with `- `. For an object item the first key follows `- `
  on the same line and the rest align under it.
- An empty sequence is `[]` and an empty mapping is `{}` on the key's line.
- No `---` marker; exactly one trailing `LF`.

```yaml
"abstract":
  "format": 1
  "compiler": "1.0.0"
  "versions":
    "min": 1
    "max": 1
"data":
  - "template": "Product"
    "id": "atlas"
    "price": 19.5
    "featured": true
    "tags":
      - "core"
      - "public"
    "capabilities":
      - "id": "search"
        "availability": "stable"
"overlays": []
```

### 4.3 RAW

SPEC 8.6. RAW (`.abraw`) is a **review** format: the same data with unquoted
keys and wider indentation, meant for diffs and code review. It is not intended
to be machine-parsed, and no Abstract tool reads it back.

- Four spaces of indentation per level.
- The envelope object is rendered without its outer braces: one `key: value`
  entry per line at indentation 0, in envelope order, separated by `,` at end of
  line.
- Keys are bare. Every key is an identifier or an envelope key, so no escaping
  is needed.
- An object is **always** multi-line.
- An array whose elements are all scalars is inline; an array with at least one
  object or array element is multi-line.
- Scalars use the same spelling and escaping as JSON.
- Exactly one trailing `LF`.

```text
abstract: {
    format: 1,
    compiler: "1.0.0",
    versions: {
        min: 1,
        max: 1
    }
},
data: [
    {
        template: "Product",
        id: "atlas",
        price: 19.5,
        featured: true,
        tags: ["core", "public"],
        capabilities: [
            {
                id: "search",
                availability: "stable"
            }
        ]
    }
],
overlays: []
```

---

## 5. Scalars

### Numbers

SPEC 8.7.

**Integers** render in base ten with no leading zeros (except `0` itself), with
`-` for negative values and never `+`. The full signed 64-bit range is
representable.

**Floats** render as the shortest decimal string that round-trips to the same
binary64 value, then:

- `0` renders `0.0`, and negative zero renders `-0.0`;
- otherwise, when `1e-6 <= |value| < 1e21`, plain positional notation, with
  `.0` appended when the shortest form has no fractional digits; so a float is
  always distinguishable from an integer, and `3` renders as `3.0`;
- otherwise scientific notation: one digit, `.`, at least one fractional digit,
  `e`, an explicit sign, and the exponent with no leading zeros (`1.0e+21`,
  `1.5e-7`).

Non-finite values cannot occur: literals that are not finite are rejected at
parse time and ranges must be finite.

### Strings

SPEC 8.8. In all three formats a string is emitted between double quotes with
exactly these escapes:

| Character | Escape |
|---|---|
| `"` | `\"` |
| `\` | `\\` |
| `U+0008` | `\b` |
| `U+0009` | `\t` |
| `U+000A` | `\n` |
| `U+000C` | `\f` |
| `U+000D` | `\r` |
| any other C0 or C1 control character | `\u00XX`, lowercase hexadecimal |

Every other Unicode scalar value, including all non-ASCII text, is emitted
literally as UTF-8. `/` is never escaped.

### Types a consumer can rely on

| Schema type | JSON kind |
|---|---|
| `text`, `enum`, `file`, `image`, `ref` | string |
| `int` | number with no fractional part |
| `float` | number, always with a fractional part or an exponent |
| `bool` | boolean |
| group, `$(Schema)` | object |
| any list field | array |

Enum values are always the **normalised** member name, so a consumer compares
against lowercase, `_`-separated text. A `ref` value is the referenced instance's
id as a string; the referenced object is never inlined.

---

## 6. Versions and overlays

SPEC 7.5. A project carries every model version of its data in one document.

`data` is the document for the **maximum** version; the base. Every earlier
version that differs is expressed as overlay entries: whole replacement objects
and removed ids. There is no field-level delta, and no `null` anywhere.

Each overlay has exactly three keys, in this order:

| Key | Value |
|---|---|
| `versions` | an object with `min` and `max`, the closed range this overlay covers |
| `data` | complete instance objects that replace or add to the base over that range, ordered by `(template, id)` |
| `removed` | the ids of base objects that do not exist over that range, ascending as sequences of Unicode scalar values |

All three keys are always present. Either array may be empty, but not both: a
range with no entry produces no overlay. Overlays are ordered by
`versions.min`, then `versions.max`.

### 6.1 The consumer algorithm

To obtain the document for version `v`:

1. start from `data`;
2. for **every** overlay whose range contains `v`:
   - for each object in that overlay's `data`, replace the object with the same
     `id`, or add it when the base has none;
   - delete every object whose `id` appears in that overlay's `removed`.

There may be no matching overlay, one, or several. Nothing is merged and no
field is deleted individually, so a consumer needs only a lookup by `id`.

The ranges of two overlays **may overlap**; one instance may change at version
2 while another exists only up to version 1; but for any single id the ranges
of the overlays that mention it are **disjoint**. No id is therefore touched
twice, and the order in which the matching overlays are applied does not matter.

A consumer that wants the canonical order sorts the result by `(template, id)`.
A consumer that only looks objects up by `id` need not.

Do not assume that at most one overlay matches a version, and do not assume that
every id in an overlay is already present in the base. The project
[`examples/features/overlays-multi-match`](../../examples/features/overlays-multi-match) exercises both:
one version matched by two overlays, and an instance that reaches a consumer only
through an overlay because it exists in no base document.

### 6.2 A field that appears and a field that disappears

Project [`examples/features/versions-overlays`](../../examples/features/versions-overlays):

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

`glow` exists from version 2; `legacy_tint` exists up to version 2. The base is
version 3.

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 3
    }
  },
  "data": [
    {
      "template": "Item",
      "id": "torch",
      "name": "Torch",
      "glow": false
    }
  ],
  "overlays": [
    {
      "versions": {
        "min": 1,
        "max": 1
      },
      "data": [
        {
          "template": "Item",
          "id": "torch",
          "name": "Torch",
          "legacy_tint": 200
        }
      ],
      "removed": []
    },
    {
      "versions": {
        "min": 2,
        "max": 2
      },
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
}
```

A consumer running version 3 matches no overlay and uses `data` unchanged. One
running version 1 applies the first overlay and replaces `torch`.

### 6.3 Instances that come and go

Project [`examples/features/instance-windows`](../../examples/features/instance-windows), with the same
schema:

```abstract
Item :: @id.lantern @since(2)
    name: Lantern

Item :: @id.candle @removed(3)
    name: Candle
```

`lantern` exists in versions 2 and 3; `candle` in versions 1 and 2. The base is
version 3, so it carries `lantern` and not `candle`.

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 3
    }
  },
  "data": [
    {
      "template": "Item",
      "id": "lantern",
      "name": "Lantern",
      "glow": false
    }
  ],
  "overlays": [
    {
      "versions": {
        "min": 1,
        "max": 1
      },
      "data": [
        {
          "template": "Item",
          "id": "candle",
          "name": "Candle"
        }
      ],
      "removed": [
        "lantern"
      ]
    },
    {
      "versions": {
        "min": 2,
        "max": 2
      },
      "data": [
        {
          "template": "Item",
          "id": "candle",
          "name": "Candle",
          "glow": false
        }
      ],
      "removed": []
    }
  ]
}
```

A consumer running version 1 applies the first overlay: it adds `candle`; which
the base does not carry at all; and deletes `lantern`.

---

## 7. Determinism

SPEC 7.6. For a fixed set of source bytes one compiler emits byte-identical
output regardless of:

- the order in which the filesystem enumerates directories;
- the order and spelling of the command-line roots that resolve to the same
  project;
- the platform, path separator convention, locale, time zone or environment;
- whether `--skip-assets` was passed;
- whether the compile was whole-project or single-file, for the instances common
  to both.

Two different implementations, or two releases of one implementation, differ
only in the `abstract.compiler` string. Every other byte is fixed by the
specification.

Compilation reads no file other than the discovered sources and the assets that
validated values reference, writes no file other than the one named by `--out`,
and performs no network access.

---

## 8. Practical notes for consumers

- **Look up by `id`.** Ids are unique project-wide and normalised, so they are
  stable join keys. `template` tells you which schema the object satisfies.
- **Treat key presence as the only absence signal.** There is no `null` and no
  sentinel.
- **Do not infer a type from a value.** A `text` field holding `19.5` is the
  string `"19.5"`; a `float` field holding 3 renders `3.0`. The schema is the
  contract.
- **Pin `abstract.format`.** Refuse a document whose `format` you do not
  understand rather than guessing.
- **Read `abstract.versions`** before applying overlays: `max` is the version
  `data` already represents, and `min` is the oldest version the document can
  produce.
- **Do not parse RAW.** It is for humans reading diffs. Read JSON, or YAML if
  your pipeline prefers it.
