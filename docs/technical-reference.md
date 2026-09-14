# Abstract 1.2 technical reference

Numeric expressions and function contracts are listed in
[ARITHMETIC.md](ARITHMETIC.md). Use `calc(...)` in a derive value or condition
operand; ordinary instance values remain data.

A lookup table for people editing `.ab` and `.abt` files. Every entry names the
section of [`SPEC.md`](SPEC.md) that defines it; the specification is normative
and this page is not. The grammar rule names in the right-hand columns are
defined in [`GRAMMAR.ebnf`](GRAMMAR.ebnf).

For a narrative introduction read [`abstract-language.md`](abstract-language.md)
instead. Runnable versions of the examples below live in
[`examples/`](examples/README.md).

---

## 1. Files and projects

| Question | Answer | SPEC |
|---|---|---|
| What is a template file? | `.abt`: `schema`, `logic`, at most one `versions` | 2.1 |
| What is an instance file? | `.ab`: `instance` declarations only | 2.1 |
| Are extensions case-sensitive? | No. `Item.ABT` is a template file | 2.1 |
| What encoding? | UTF-8. A BOM at offset 0 is stripped; anywhere else it is E210 | 2.2 |
| Line endings? | `LF` and `CRLF`. A lone `CR` is E210 | 2.2 |
| Where is the project root? | The parent of the nearest ancestor directory named `data`; otherwise the compiled directory | 2.3 |
| Where do assets live? | `<project root>/assets` | 2.3, 5.9 |
| Which directories are skipped? | any name starting with `.`, plus `node_modules`, `target`, `build`, `out` | 2.4 |
| What if there are no sources? | E103 | 2.4 |
| What does naming one `.ab` file do? | Compiles the whole project, emits only that file's instances | 2.5 |

### Layout

```text
m-project/
  assets/
    textures/atlas.png
  data/
    templates/Product.abt
    products/atlas.ab
```

`abstract compile m-project`, `abstract compile m-project/data` and
`abstract compile m-project/data/products/atlas.ab` all resolve to the same
project. A directory *inside* `data/` is E806 (SPEC 2.3).

---

## 2. Lexical rules

| Construct | Rule | SPEC |
|---|---|---|
| Whitespace | space and tab separate tokens; indentation is meaningless | 3.1 |
| Comment | `//` to end of physical line, only at line start or after a space/tab, never inside a string | 3.2 |
| Identifier | `[A-Za-z0-9_][A-Za-z0-9_-]*`, ASCII only. A character outside the set is E206, a trailing `-` is E207, a leading `-` is E210 | 3.3 |
| Normalisation | ASCII-lowercase, then `-` becomes `_` | 3.3 |
| Schema name | `[A-Za-z][A-Za-z0-9_]*`, compared **exactly** | 3.4 |
| Integer literal | optional `-`, digits; must fit in signed 64 bits (E211); no `+`, no `_` | 3.5 |
| Float literal | digits on both sides of the `.`, or an exponent; `.5` and `5.` are not floats | 3.5 |
| Boolean literal | `true` / `false`, lowercase | 3.5 |
| Quoted string | `"…"` on one physical line; escapes `\"` `\\` `\n` `\r` `\t` only | 3.5 |
| Bare text | any other unquoted run, kept verbatim after trimming. Every Unicode scalar value except a C0 control, `U+007F` and a C1 control (E210); non-ASCII whitespace is literal. It may not begin with `//`, wherever it begins, and its braces and brackets must balance | 3.2, 3.5, 3.6 |
| Line joining | only while `(`/`[`/multi-path `{` are open, after a header-tag comma, and before `else` in logic — the `else` may sit past any number of blank lines and comment lines | 3.6, 6.3 |
| Longest match | except `..`, which is never part of a float: `1..3` is `1`, `..`, `3` | 3.1 |

### What is normalised, and what is not

| Normalised | Not normalised |
|---|---|
| field names, enum members, path segments | schema names |
| header tag names, `#tag` names, `tag_arg` keys | quoted string contents |
| multi-path keys, tuple column names | bare text values |
| instance ids, clone targets, `ref` values | asset path text |
| logic path segments, loop variable names | — |

### Limits (E209, and the two that are not)

| Subject | Limit | SPEC |
|---|---|---|
| bracket nesting in a value | 64 | 3.7 |
| segments in an assignment, clone or logic path | 64 | 3.7 |
| schema group nesting, `$(Schema)` nesting in one object | 64 | 3.7 |
| `if` / `for` nesting in one logic block | 64 | 3.7 |
| clone chain length | 64 | 3.7 |
| instance nesting depth (the instance object is level 1; object or list = one level) | 64 | 3.7 |
| versions in the project range (`max - min + 1`) | 4096, reported as E602 | 3.7, 4.12 |
| loop iterations executed by the logic of one instance for one version | 1 000 000, reported as E523 | 3.7, 6.2 |

Every row is E209 except the last two. One unit of logic work is one execution of
a `for` body; `derive`, `require` and `if` cost nothing, the budget is spent by
every block that runs for one instance in one version — nested `$(Schema)` blocks
included — and it is fresh for the next instance and the next version.

---

## 3. Schema syntax

```abstract
schema <Name> {
    <name>[ "[" [cardinality] "]" ] : <type> [modifiers] [= default]
    <name>[ "[" [cardinality] "]" ] [modifiers] {
        <fields>
    }
}
```

| Element | Rule | SPEC |
|---|---|---|
| Schema names | unique project-wide; a repeat is E301 | 4.2 |
| Field order | declaration order is output key order | 4.2, 8.3 |
| Duplicate field names | E302, regardless of version annotations | 4.3 |
| Modifiers | `@optional`, `@tag`, `@since(n)`, `@removed(n)`; each at most once (E303) | 4.3 |
| Modifier position | after the type (or the group head), before `=` | 4.3 |
| Doubled list head | `name[][]` is E315 | 4.6 |

### 3.1 Types

| Type | Grammar rule | Arguments | SPEC |
|---|---|---|---|
| `text` | `text_type` | none, or an int range list counting Unicode scalar values | 4.4.1 |
| `int` | `int_type` | none, or an int range list | 4.4.2 |
| `float` | `float_type` | none, or a float range list; the value must be finite | 4.4.3 |
| `bool` | `bool_type` | none | 4.4.4 |
| `enum` | `enum_type` | one or more identifiers, normalised | 4.4.5 |
| `file` | `file_type` | one or more extensions, canonicalised | 4.4.6 |
| `image` | `image_type` | one or more `ext [WIDTHxHEIGHT]` alternatives | 4.4.7 |
| `ref` | `ref_type` | one schema name | 4.4.8 |
| `$(Schema)` | `nested_schema_type` | one schema name | 4.4.9 |

Whitespace before a type's `(` is E304. An empty argument entry is E306
(`enum`) or E319 (`file`, `image`). `enum()`, `file()`, `image()` and `ref()`
are E317; `text()`, `int()` and `float()` are valid and unconstrained.

Extension canonicalisation (SPEC 4.4.6): a single leading `.` is stripped, the
text is ASCII-lowercased, and `jpeg` becomes `jpg`. Nothing else is folded, so
`file(jpg, jpeg)` is E319.

Image formats and what the probe reads (SPEC 4.4.7.1): `png`, `jpg`, `gif`,
`bmp`, `webp`. At most 65 536 bytes from the head of the file; pixels are never
decoded. A file that matches a signature but is too short is E422 with
`note: file is truncated.`

### 3.2 Ranges

| Form | Meaning | SPEC |
|---|---|---|
| `int(7)` | exactly 7 | 4.5 |
| `int(2..15)` | a closed interval | 4.5 |
| `int(1, 3..12)` | satisfied by at least one part | 4.5 |
| `int(5..)`, `int(..5)` | E305 — both bounds are required | 4.5 |
| `int(10..5)` | E305 — reversed, rejected at the schema | 4.5 |
| `text(-5..-1)` | E318 — unsatisfiable | 4.5 |

### 3.3 List cardinality

| Head | Meaning | SPEC |
|---|---|---|
| `tags[]` | any number of elements | 4.6 |
| `tags[2..]` | at least 2 | 4.6 |
| `tags[2..4]` | 2 to 4 | 4.6 |
| `tags[3]`, `tags[..4]`, `tags[4..2]`, `tags[-1..]` | E323 | 4.6 |

A value whose element count is outside the cardinality is E445. Cardinality
constrains a value, never presence: an absent optional list is omitted, not
E445.

### 3.4 `@tag`

| Rule | Error | SPEC |
|---|---|---|
| must be inside a group or list group | E311 | 4.8 |
| at most one per group | E310 | 4.8 |
| non-list `text`, `int`, `bool` or `enum` only | E322 | 4.8 |
| may carry a default **or** `@optional`, not both | E320 | 4.10 |

### 3.5 Defaults and presence

| Field | Behaviour when absent | SPEC |
|---|---|---|
| required (no `@optional`, no default) | E411 after logic | 4.10 |
| has a default | filled before logic, interpolated at fill time | 4.10 |
| `@optional` | omitted from output; never `null`, never `[]` | 4.10, 8.9 |
| `@optional` group | omitted; children's defaults are not filled | 4.7 |
| required group | E411 at the group | 4.7 |

A default is validated at schema-validation time (E313). On a non-list field a
list default is E313 with `note: this field is not a list.` A group field may
not carry a default (E312).

### 3.6 `template` and `id`

| Spelling | Verdict | SPEC |
|---|---|---|
| `template: <anything>` | E314 | 4.11 |
| `id: text` | valid | 4.11 |
| `id: text(3..24)` | valid | 4.11 |
| `id: int`, `id[]: text`, `id: text @optional`, `id: text = x` | E314 | 4.11 |
| `id` inside a group | an ordinary field name | 4.11 |

The implicit declaration is `id: text(1..64)`. An id outside the applicable
range is E413.

---

## 4. Instance syntax

```abstract
<Schema> :: [@tag[.value], …] [@since(n)] [@removed(n)]
&<clone> …
    <path>: <value> [@since(n)] [@removed(n)]
    <path> {
        <path>: <value>
    }
```

### 4.1 Header

| Rule | SPEC |
|---|---|
| A line is a header iff its first token is a schema name and the next is `::` | 5.1 |
| Unknown schema name: E401, with a suggestion | 5.1 |
| A second `::` in a header: E436 | 5.1 |
| `::` on a line that is not a header: E439 | 5.1 |
| A statement before the first header: E403 | 5.1 |
| A header may end with an instance version window | 5.14 |

### 4.2 Header tags

| Form | Meaning | SPEC |
|---|---|---|
| `@name.value` | assigns `value` to root field `name` | 5.2 |
| `@name` | assigns `true`; the field must be `bool`, else E412 | 5.2 |
| `@label."hi, there @world"` | quote to include a `,` or `@` | 5.2 |
| `@icon../textures/a.png` | assigns `./textures/a.png` — the first `.` is the separator | 5.2 |

| Condition | Error | SPEC |
|---|---|---|
| tag names no root field | E409 | 5.2 |
| tag targets a group, list group or `$(Schema)` | E438 | 5.2 |
| `@id` written as a bare flag | E428 | 5.2 |
| `@since` / `@removed` on a tag | E437 | 5.2 |
| two tags with the same normalised name, or a tag and a statement on one path | E429 | 5.2 |
| trailing `,` with no following tag | E442 | 5.2 |

`@since(2)` (with parentheses, no dot) is an instance window; `@since.2` is a
header tag; a bare `@since` is a boolean flag on a field named `since`
(SPEC 5.2).

### 4.3 Identity

| Source | Rule | SPEC |
|---|---|---|
| `@id.<value>` | must be an identifier (E428), normalised, never type-inferred | 5.3 |
| file stem | used when the header has no `@id`; must itself be an identifier | 5.3 |
| uniqueness | project-wide across all templates (E402), including across version windows | 5.3 |
| assignment | `id:` in the body is E410 | 5.3 |

### 4.4 Paths

| Form | Meaning | SPEC |
|---|---|---|
| `a: v` | root field | 5.4 |
| `a.b: v` | nested field; intermediate objects are created | 5.4 |
| `a.{b, c}: v` | multi-path; the prefix may be dotted, keys are single identifiers | 5.4 |
| `a { … }` | body block: sugar for the prefix `a.` | 5.4 |
| `a[0].b: v` | E316 — there is no index syntax in an assignment | 5.4 |

| Condition | Error | SPEC |
|---|---|---|
| empty path segment (`a..b`, `.a`, `a.`) | E316 | 5.4 |
| empty value after the `:` | E210 | 5.4 |
| path crosses a scalar, list or `#tag` object | E443 | 5.4 |
| the same path written twice in one applicable version | E429 | 5.4, 5.13 |
| empty multi-path key list | E433 | 5.4 |
| clone statement inside a body block | E404 | 5.4 |
| annotation on a body block | E437 | 5.4 |

### 4.5 Values

| Syntax value | Written | SPEC |
|---|---|---|
| `Quoted` | `"…"` | 5.10 |
| `Bare` | anything else unquoted | 5.10 |
| `List` | `[a, b]` or `a, b` | 5.5 |
| `Tag` | `#name`, `#name(k: v, flag)` | 5.5 |
| `Object` | dotted paths, multi-paths, tuple rows | 5.10 |

Interpretation against the declared type (SPEC 5.10) is one deterministic
function performing, in order: brace expansion, enum wildcard expansion, `#tag`
expansion and normalisation, per-element type/range/enum checks, then
single-value-to-list coercion and the cardinality check.

| Declared type | `Bare` | `Quoted` |
|---|---|---|
| `text` | the lexeme verbatim | the decoded string |
| `int` | must be an integer literal, else E412 | **E412** |
| `float` | must be a float or integer literal | **E412** |
| `bool` | must be exactly `true` or `false` | **E412** |
| `enum` | normalised; must be a member (E414); `*` expands | normalised; must be a member |
| `file` / `image` | path text; braces expand on list fields | path text, never expanded |
| `ref` | normalised as an id; must resolve (E431) | same as `Bare` |
| `$(Schema)` / group | E412 | E412 |

Quoting never coerces, and it never un-coerces: `caption: "19.5"` on a `text`
field is the string `19.5`.

| Value rule | Consequence | SPEC |
|---|---|---|
| a depth-0 `,` always separates list items | quote a text value that contains one | 5.5 |
| a leading `[` always starts a list | quote to write a literal `[` | 5.5 |
| a leading `#` always starts a tag object | quote to write a literal `#` | 5.5 |
| a leading `"` always starts a quoted string | trailing text after the close quote is E210 | 5.5 |
| a trailing `,` at depth 0 | E442 | 3.6 |
| an empty item (`a,,b`) or a nested list | E441 | 5.5 |
| two keyed-list elements with one key | E444 | 5.5 |

### 4.6 Tagged objects and tuple arrays

```abstract
capabilities: [#search, #export(availability: alpha), #sync(limits.soft: 10)]
copy(key, value): (en_us, Welcome), (es_es, Bienvenido)
```

| Condition | Error | SPEC |
|---|---|---|
| `#tag` where the group declares no `@tag`, or the target is not a group | E418 | 5.5 |
| argument key names no field of the group | E409 | 5.5 |
| argument is neither `key: value` nor a bare identifier | E419 | 5.5 |
| a bare flag on a non-`bool` field | E412 | 5.5 |
| text after the closing `)` | E210 | 5.5 |
| duplicate tuple column | E417 | 5.5 |
| row arity mismatch | E416 | 5.5 |
| no columns, or an empty row | E416 | 5.5 |
| the target is not a list group or list of `$(Schema)` | E412 | 5.5 |
| anything but one comma between rows | E210 | 5.5 |

### 4.7 Wildcards, brace patterns and interpolation

| Construct | Where it is legal | SPEC |
|---|---|---|
| `prefix*` | an element of an `enum` **list** field; a tuple cell whose column is an `enum`; the name of a `#tag` whose tag field is an `enum` | 5.6 |
| `{a,b}` | a value of a `file` or `image` **list** field | 5.8 |
| `$name`, `${name}`, `$$` | inside every `Bare` and `Quoted` value | 5.11 |

| Condition | Error | SPEC |
|---|---|---|
| a wildcard matches no member | E415 | 5.6 |
| a brace pattern on a non-list asset field | E434 | 5.8 |
| an empty or nested brace group | E435 | 5.8 |
| an unknown variable | E425 | 5.11 |
| `$` followed by anything but an identifier character, `{` or `$` | E426 | 5.11 |
| an unterminated `${` | E427 | 5.11 |

Interpolation runs **before** brace expansion, is a single left-to-right pass,
and never rescans what it substituted (SPEC 5.8, 5.11).

### 4.8 Clones

| Form | Meaning | SPEC |
|---|---|---|
| `&id`, `&id.*` | full clone | 5.7 |
| `&id.path` | partial clone of that subtree | 5.7 |

| Rule | Error | SPEC |
|---|---|---|
| must appear directly after the header | E404 | 5.7 |
| target must exist | E405 | 5.7 |
| target must use the same template | E407 | 5.7 |
| target's window must contain the cloner's | E440 | 5.7 |
| a partial path that can never exist on the source | E408 | 5.7 |
| cycles | E406 | 5.7 |
| `&other.id`, `&other.template` | E410 | 5.7 |
| an annotation on a clone statement | E437 | 5.7 |

Merge table for folding a source value `s` onto an existing value `t`
(SPEC 5.7):

| `t` | `s` | result |
|---|---|---|
| absent | any | `s` |
| object | object | recursive merge, key by key |
| keyed list | keyed list | merge by tag value; `t`'s order is kept, new keys are appended |
| anything else | any | `s` replaces `t` |

The instance's own header tags and body statements are applied afterwards by
plain assignment, with no merging.

### 4.9 Asset paths

One rule (SPEC 5.9): strip one leading `./` or `.\`, then resolve relative to
`<project root>/assets`.

| Condition | Error |
|---|---|
| absolute, drive-prefixed or UNC path, or a leading `/` or `\` | E424 |
| a `..` segment | E424 |
| empty, or containing NUL | E424 |
| extension not declared by the field | E420 |
| file missing on disk (unless `--skip-assets`) | E421 |
| image content does not match the extension | E422 |
| image dimensions match no declared alternative | E423 |

---

## 5. Versions

| Construct | Meaning | SPEC |
|---|---|---|
| `versions 1..3` | the project range; at most one per project (E601) | 4.12 |
| no declaration | the range is `1..1` | 4.12 |
| `@since(n)` on a field | the field exists for `v >= n` | 4.12 |
| `@removed(n)` on a field | the field exists for `v < n` | 4.12 |
| `@since(n)` / `@removed(n)` on a body statement | the statement applies to those versions | 5.13 |
| `@since(n)` / `@removed(n)` at the end of a header | the whole instance exists in those versions | 5.14 |

| Condition | Error | SPEC |
|---|---|---|
| `n` outside the project range | E603 | 4.12 |
| `@removed(n)` not greater than the effective `@since` | E604 | 4.12 |
| an empty existence set (a child outliving its group) | E605 | 4.12 |
| a statement that can never apply to its field | E430 | 5.13 |
| a statement or clone outside its instance's window | E440 | 5.13, 5.14 |
| a `derive` executing against a field absent in this version | E521 | 6.4 |
| an invalid `versions` range | E602 | 4.12 |

A field's existence set is always one contiguous interval: a field is never
removed and re-added (SPEC 4.12). Annotations are allowed on exactly three
constructs — a field, a body statement, and an instance header — and nowhere
else (E437).

---

## 6. Logic

| Statement | Grammar rule | SPEC |
|---|---|---|
| `derive <path> = <expr>` | `derive_statement` | 6.4 |
| `derive? <path> = <expr>` | `derive_statement` | 6.4 |
| `require <cond> else throw "<msg>"` | `require_statement` | 6.2 |
| `if … { } else if … { } else { }` | `if_statement` | 6.3 |

A `}` and its `else`, and a `require` condition and its `else throw`, may be
written on two physical lines with any number of blank lines and comment lines
between them: the continuation rule tests the next token, not the next line
(SPEC 3.6 rule (b), 6.3). Any other token in between ends the statement, and the
`else` after it is E517.
| `for $x in <iterable> { }` | `for_statement` | 6.2 |

### 6.1 Precedence

| Level | Construct | SPEC |
|---|---|---|
| 1 | `( … )`, `length( … )`, path, loop variable, `version`, literal | 6.6 |
| 2 | postfix `exists` | 6.6 |
| 3 | `==` `!=` `<` `<=` `>` `>=` `contains` (non-associative) | 6.6 |
| 4 | prefix `not`, `!` | 6.6 |
| 5 | `and`, `&&` | 6.6 |
| 6 | `or`, `\|\|` | 6.6 |

### 6.2 Comparison semantics

| Situation | Result | SPEC |
|---|---|---|
| an operand resolves to no value | every comparison is false, including `!=` | 6.8 |
| two strings | equal iff the same sequence of Unicode scalar values; no folding | 6.8 |
| a number and a string | false, not an error | 6.8 |
| `<` `<=` `>` `>=` on non-numbers | E513 | 6.8 |
| `contains` on list / on string | membership / substring | 6.8 |
| any other `contains` combination | E513 | 6.8 |
| comparing a list or object with `==` | E513 | 6.8 |
| an operand that can project several values | E519 | 6.5 |
| a chained comparison | E512 | 6.6 |
| a non-boolean condition | E513 | 6.6 |

`exists` is true when the path resolves to at least one value and that value is
not a single empty list. An empty string is true; an absent field is false. It
never touches the filesystem (SPEC 6.7).

### 6.3 `length` and `version`

| Argument | `length` yields | SPEC |
|---|---|---|
| a list | element count | 6.10 |
| a string | Unicode scalar count | 6.10 |
| a projection | the number of projected values | 6.10 |
| no value | `0` | 6.10 |
| a declared `int`, `float`, `bool`, group, `$(Schema)` or `ref` | E514, statically | 6.10 |

`version` is the integer version being compiled. It is a bare word with no `.`
and no `$`, is never bound or shadowed, and is available in every condition and
every `derive` expression (SPEC 6.6).

### 6.4 Logic error index

| Error | Condition | SPEC |
|---|---|---|
| E501 | logic block for an unknown schema | 6.1 |
| E502 | second logic block for one schema | 6.1 |
| E503 | a path segment names no declared field | 6.5 |
| E504 | `derive` targets `template` or `id` | 6.4 |
| E505 | `derive` target is not a declared field, or is loop-relative | 6.4 |
| E506 | `derive` traverses or indexes a list | 6.4 |
| E507 | `require` without `else throw` | 6.2 |
| E508 | throw message is not a quoted string | 6.2 |
| E509 | `for` over a non-list | 6.2 |
| E510 | unbound loop variable | 6.9 |
| E511 | malformed condition | 6.6 |
| E512 | chained comparison | 6.6 |
| E513 | operand type not allowed | 6.6, 6.8 |
| E514 | invalid `length()` argument | 6.10 |
| E515 | a `require` failed | 6.12 |
| E516 | loop variable shadowing | 6.9 |
| E517 | misplaced `else` | 6.3 |
| E518 | `derive?` on a defaulted field | 6.4 |
| E519 | an operand can project several values | 6.5 |
| E520 | loop variable in a `derive` target | 6.4 |
| E521 | a `derive` executed against a field absent in this version | 6.4 |
| E522 | an interpolated `$name` bound to a value that is not a scalar | 6.9 |
| E523 | the logic of one instance executed more than 1 000 000 loop iterations for one version | 3.7, 6.2 |

---

## 7. Pipeline

| Phase | Work | SPEC |
|---|---|---|
| P0 | discovery: collect and sort source files | 2.4 |
| P1 | lex and parse, one AST per source, with positions | 3–6 |
| P2 | project tables: schemas, logic, versions, instances | 7.2 |
| P3 | schema and logic validation, independent of any instance | 7.2 |
| P4 | compile every instance for every version in which it exists | 7.3, 7.4 |
| P5 | overlay reduction: base document plus changed objects | 7.5 |
| P6 | rendering | 8 |

Per instance and version (SPEC 7.3): authored object from clones, header tags
and statements → interpolation → authored type checks → defaults → logic →
required-field check → full re-validation → key ordering. Steps 3, 4, 6 and 7
recurse into every present nested object.

Diagnostic precedence when one input satisfies several conditions (SPEC 11.1):
earliest phase, then earliest step, then earliest source position, then the
lower identifier. The pairs fixed explicitly are E443 before E412; E211 before
E412 and E413; E603 before E430 and E440; E430 before E440; E320 before E313;
E807 before E804; E415 before E414; E445 before E411.

---

## 8. Output

See [`raw-data.md`](raw-data.md) for the consumer contract, and SPEC 8 for the
byte-exact rules.

| Rule | Value | SPEC |
|---|---|---|
| top-level keys | `abstract`, `data`, `overlays`, in that order | 8.1 |
| `abstract` keys | `format`, `compiler`, `versions` | 8.1 |
| instance keys | `template`, `id`, then declared fields in declaration order | 8.2, 8.3 |
| instance order | by `(template, id)` | 2.7 |
| JSON indent | two spaces | 8.4 |
| YAML keys | always double-quoted | 8.5 |
| RAW indent | four spaces, bare keys | 8.6 |
| floats | shortest round-trip; exponent outside `1e-6 … 1e21`; always a fractional part | 8.7 |
| absent optional fields | omitted, in every format, including lists | 8.9 |
| null | does not exist | 8.9 |

---

## 9. Command line

| Command | Synopsis | SPEC |
|---|---|---|
| `compile` | `abstract compile <path>… [FORMAT] [--out <file>] [--skip-assets] [--max-errors <n>]` | 9.2 |
| `lint` | `abstract lint <path>… [--skip-assets] [--max-errors <n>]` | 9.2 |
| `templates` | `abstract templates <path>` | 9.2 |
| `init` | `abstract init <directory>` | 9.2 |
| help / version | `--help`, `-h`, `help`; `--version`, `-V`, `version` | 9.2 |

| Flag | Meaning | SPEC |
|---|---|---|
| `--out <file>`, `--out=<file>` | write the document to a file; nothing goes to stdout | 9.4 |
| `--skip-assets` | skip only E421, E422 and E423; the emitted bytes never change | 9.5 |
| `--max-errors <n>` | report at most `n` diagnostics; `n` is 1 to 10000, default 20, anything else E812 | 9.3 |

| Exit code | Meaning | SPEC |
|---|---|---|
| 0 | success | 9.6 |
| 1 | compilation failed (an `E1xx`–`E7xx` diagnostic) | 9.6 |
| 2 | usage error (an `E8xx` other than E810) | 9.6 |
| 3 | output could not be written (E810) | 9.6 |

Diagnostic format (SPEC 9.8):

```text
<path>:<line>:<col>: error[<ID>]: <message>
  note: <note text>
```

Paths are project-relative with `/` separators; columns count Unicode scalar
values. The full catalogue of identifiers, messages and substitutions is
SPEC 10.

---

## 10. Reserved words

| Position | Words | SPEC |
|---|---|---|
| top level of a `.abt` file | `schema`, `logic`, `versions` | B.3 |
| type position | `text`, `int`, `float`, `bool`, `enum`, `file`, `image`, `ref` | B.3 |
| modifier position | `@optional`, `@tag`, `@since`, `@removed` | B.3 |
| logic statement position | `derive`, `derive?`, `require`, `if`, `else`, `for` | B.3 |
| logic expression position | `in`, `not`, `and`, `or`, `contains`, `exists`, `length`, `version`, `true`, `false` | B.3 |

Keywords are recognised only where they are meaningful; none of them is
reserved as a field name, an enum member or an instance id. The document keys
`abstract`, `format`, `compiler`, `versions`, `data`, `overlays`, `removed`,
`min` and `max` are not reserved in the source language either. Only `template`
and `id` are genuinely reserved, and only at the root of an instance object
(SPEC B.1, B.2).
