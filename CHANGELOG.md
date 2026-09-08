# Changelog

All notable changes to Abstract are recorded here. Versions follow semantic
versioning; the language itself is versioned by this document's `1.0.0` entry
and by the `abstract.format` number in every compiled document.

---

## 1.2.0

- Added the negotiated `publicInventory: 1` editor capability and
  `analyze <project> --stdio --public`. The existing compilation pipeline supplies
  nominated declarations, exact source provenance and the complete unbound scalar
  export. Unused nominations remain visible; export failures retain declarations
  and separate diagnostics without a partial admitted fragment. Private compiled
  documents are not sent through this editor response.
- Fixed unannotated existence windows at version `4294967295`. An omitted removal
  bound now includes the project's maximum directly; it no longer loses the last
  version through saturating addition. An explicit `@removed(n)` remains exclusive.
- VS Code extension 1.4.0 adds **Abstract: Inspect Public Fields**, with a native
  list, dirty-source analysis, exact declaration/root navigation and separate
  export diagnostics. Source changes invalidate results before navigation.
  The feature requires its advertised capability and Workspace Trust.

## 1.1.0

- Added argument-free `@public` field nominations and the opt-in
  `public-contract` compilation envelope. Its independent scalar profile preserves
  exact types, domains, defaults, optional absence and version windows; unsupported
  dependencies reject the export. Nominations do not grant runtime authorization.
- VS Code extension 1.3.0 adds nomination highlighting, completion and field hover
  explanations alongside existing compiler-backed schema references and rename.

## 1.0.1

Five specification questions that the 1.0 freeze recorded as open are settled.
Four of them only make the specification say what the compiler already did; one
adds a limit, an error identifier and the code that enforces it. An independent
stage then re-derived all five from the settled text and put them to the built
compiler, and five more things are closed with them: three diagnostics the
compiler did not yet report the way the text prescribes, and two places where
the text did not yet describe what the compiler does.

- **A project that declares no instance compiles to an empty document.**
  §8.1 named single-file mode as the only way to reach an empty `data` array,
  and §7.2 blesses a whole-project compile of a project with zero instances at
  the same time. §8.1 now states both, and keeps them apart from the third
  case: a project with no source files at all is E103 and emits no document.
  The whole-project bullet asks only that discovery found source files and that
  none of them declares an instance — an empty file, a file of comments and a
  file holding nothing but `versions 1..3` each reach the same document, and
  the envelope takes its range from that last one like any other. No behaviour
  changed.

- **E511's unreachable condition is removed.** §6.6 said an unbalanced
  parenthesis was E511, which no input could produce: `(` and `)` ride the
  value-bracket stack of §3.6, so an unclosed `(` is E203, a stray `)` is E205
  and a `]` closing a `(` is E204, each reported while the file is lexed. §6.6
  now names the two reasons the compiler does report — a token that cannot
  begin an operand, and a token that is not an operator where one is due — and
  §10.5's row and the reference site say the same. E511 stays in the catalogue
  and stays reachable; no message changed.

- **Blank lines and comment lines may sit between `}` and `else`.** §3.6 rule
  (b) suppresses the line terminator when the next *token* is `else`, and
  neither a blank line nor a comment line carries one. The strict reading of
  the rule as "the next physical line" made a commented `else` E517. §3.6, §6.2
  and §6.3 now state it, with an example, and the grammar's L5 says it too. The
  parser already accepted it; there are now tests that keep it accepted, for
  `} else`, `} else if` and `require` / `else throw` alike.

- **Logic work is bounded, and E523 reports a program that crosses the bound.**
  Nothing bounded the work a legal program could demand: §3.7's limits are
  about nesting, and seven `for` blocks nested over ten-element lists sit
  inside every one of them while asking for eleven million iterations. §3.7
  gains a row — **the logic of one instance MUST NOT execute more than
  1 000 000 loop iterations for one version** — counted as one unit per
  execution of a `for` body, spent by every block that runs for that instance
  in that version including nested `$(Schema)` blocks, and fresh again for the
  next instance and the next version. Crossing it stops that instance and is
  the new **E523**, positioned at the `for` whose iteration crossed the bound
  and naming the bound. It is the one change that refuses a project 1.0.0
  compiled: one that demanded more than a million iterations for a single
  instance in a single version. §10.5 also describes the diagnostic's notes —
  the instance header, then one `at index …, item ….` note per enclosing `for`,
  outermost first — which §11.2 makes normative for a conforming suite.

- **E413's `{value}` is defined for a `text` range.** A `text` range constrains
  the value's length, so E413 substitutes the number of Unicode scalar values
  in the value, never the text; §9.8 said `{value}` renders a scalar as JSON
  would, which was right for `int` and `float` and wrong here. §9.8 now defines
  both, and defines `{ranges}`: an integer part whose bounds are equal renders
  as the bare number, so `text(2..2)` reads `2`, while a float part always
  renders both bounds. The compiler already rendered exactly this, and the
  catalogue's message template already matched it, so no message changed.

Three diagnostics now read the way the settled text prescribes:

- **A malformed condition reaches E511.** A lone `=` anywhere in a `logic`
  statement began a bare value, so `if .name === b {` read the block's `{` as
  an ordinary character of that value and reported an unclosed brace (E203) and
  a stray one (E205) instead — three lines away from the fault, and about
  braces the author had balanced. The value only ever belonged to a `derive`,
  which is where the grammar puts `=`, so that is where the compiler now reads
  one; an `if`, `require` or `for` head keeps its block brace and its malformed
  condition reaches **E511**, where §6.6 puts it. Quoted right-hand sides were
  never affected, which is why the fault survived the freeze.

- **E511 quotes the condition as written.** The message rebuilt the text from
  the token stream, which inserted spaces the author did not write and removed
  ones they did: `and .name == "a"` was quoted `and.name == "a"`, and
  `.name === "a"` was quoted `.name == = "a"`. It is now the author's own text,
  taken from the source between the condition's first and last token.

- **An `else` after a block that is not an `if` is E517.** §6.3 gives one
  identifier to an `else` with no `if` to attach to, whether it stands at the
  top of a block or follows the closing `}` of a `for` block; the second
  spelling reported the generic end-of-line **E210**. An `else` that follows a
  statement opening no block is a different fault and stays E210.

Also: the conformance corpus gains a `revision-7` area with four cases — the
work limit from both sides, the schemas-only project and the commented `else` —
and an `adversarial-3` area with nineteen, which pin the five findings above
together with the work limit at exactly 1 000 000 iterations and one past it,
the budget shared with a nested `$(Schema)` block, and E413's scalar count
against astral and combining characters. 301 cases in all, and
`tests/conformance/DEFECTS-3.md` records the verification that filed them.

---

## 1.0.0

- Plain (unencrypted) `.abx` containers now carry a keyless checksum in the header bytes a sealed container uses for its nonce, so a sealed container whose encryption flag was cleared is refused with or without a key. Plain containers written by 0.2.0 no longer open; sealed containers are unaffected. The Rust and Java readers agree byte for byte.

The first stable definition of the language. Abstract 1.0 is specified by
[`docs/SPEC.md`](docs/SPEC.md) and [`docs/GRAMMAR.ebnf`](docs/GRAMMAR.ebnf); the
compiler front end and semantics were rewritten against that specification
rather than patched.

The surface syntax of 0.2.0 is kept. What changed is that every construct now
has a defined meaning, a stable error identifier and a source position, and that
nothing is silently ignored, coerced, truncated or dropped. A large number of
0.2.0 behaviours that "worked" did so by accident; those are the breaking
changes below.

Two headline additions: **versions and overlays**, which let one project carry
every version of its data in a single compiled document, and **`ref(Schema)`**,
a checked reference between instances.

The items below are the complete list from SPEC Appendix A. Each carries its
Appendix A number so a migration can be tracked item by item.

---

### Authoring rules settled at the freeze

The last specification questions the adversarial passes left open are decided,
and the compiler follows them. Each one closes a place where an author's file
could mean two things, or where a diagnostic was chosen by the implementation
rather than by the specification.

- **A bare value takes every character except a control character.** An
  unquoted value admits every Unicode scalar value except a C0 control,
  `U+007F` and a C1 control; one of those is E210 naming the character. Non-ASCII
  whitespace is **literal**, so `U+00A0` inside a caption is part of the caption
  and never a separator. Previously C0 controls and `U+007F` were refused while
  every C1 control was accepted, and `U+00A0` was refused outside a value but
  accepted inside one.
- **A bare value cannot begin with `//`.** The rule now holds wherever the value
  begins, including immediately after the `:`, where no comment rule fires:
  `cdn://cdn.example.com/x.png` is E210 and is written `cdn: "//cdn.example.com/x.png"`.
  It used to compile to the text `//cdn.example.com/x.png`.
- **Braces in a bare value must balance.** `icon: a}.{b` is E205 at the `}` and
  E203 at the `{` whatever the field's type, because the rule is lexical. Write
  a literal unbalanced brace inside quotes. One brace pattern holds at most
  eight groups; a ninth is E435, alongside an empty alternative and a nested
  group.
- **An identifier that begins with `-` is E210**, not E206. E206's message lists
  `-` among the characters an identifier may contain, so it could never be the
  diagnostic for one standing in the wrong place. `name-:` is still E207.
- **Version annotations obey the same two rules on every construct.** A body
  statement's own `@since` / `@removed` are now range-checked (E603) and
  order-checked (E604) exactly as a field's and an instance header's are, before
  anything asks which versions the statement applies to. `glow: true @since(3)
  @removed(2)` is E604 rather than a confusing E430.
- **A header tag is an assignment.** A tag whose field exists in no version in
  which its instance exists is **E440**, the same diagnostic the equivalent body
  statement gets. It used to write nothing and say nothing.
- **Interpolating a non-scalar is E522** (new identifier). `derive .label =
  cap-$c` inside `for $c in .caps`, where `caps` is a list of groups, has no text
  to insert; so does `throw "bad $c"`. The variable written as the whole
  right-hand side keeps its own type and is unaffected.
- **`--max-errors` takes an integer from 1 to 10000.** `0` no longer means "no
  limit", and every other value — including one too large to represent — is E812
  with a message that names the bound. The default is still 20.
- **A project version range covers at most 4096 versions**, reported as E602
  naming the bound. The limit is on the count, so `versions 100..163` is as legal
  as `versions 1..64`.
- **The nesting limit is counted from the instance**, not from the emitted
  document: the instance object is level 1 and every object and list below it
  adds one, with a limit of 64. It is checked once, in validation, and the
  diagnostic carries a file, line and column. The output stage no longer checks
  depth, and the positionless `abstract: error[E209]` it used to produce is gone.
- **U+007F is escaped in output**, in all three formats. The escaped set is now
  exactly the set excluded from bare text, so a control character never reaches a
  consumer unescaped. A `.yml` document carrying a DELETE used to be unreadable
  to conforming YAML parsers.
- **The CLI accepts Windows extended-length input paths.** A `\\?\` prefix on an
  input path is accepted and stripped, so it never appears in a diagnostic.
- **E412's "quote a literal comma" note** is now shown only when the list was
  written with bare commas, where it is the actual remedy, and not for `[a, b]`,
  a tuple array, or a list a default or a `derive` produced.

---

### Breaking changes: source language

#### A1 — the `data:` sentinel is gone

A statement beginning with `data:` used to stop the parser reading the rest of
the file. `data` is now an ordinary field name and every statement is parsed.

```abstract
// before: everything after this line was silently discarded
data: anything
title: Atlas          // never reached the output

// after: both statements are parsed; 'data' must be a declared field
data: anything
title: Atlas
```

#### A2 — trailing-comma continuation removed

```abstract
// before: the statement swallowed the next line
tags: core,
public

// after: E442. Wrap the list in brackets.
tags: [
    core,
    public
]
```

#### A3 — clone position

```abstract
// before
&base.*
Product :: @id.beacon

// after: clones follow the header (E404 otherwise)
Product :: @id.beacon
&base.*
```

#### A4 — cross-template clones

Cloning an instance of another schema used to be allowed, and smuggled foreign
fields into the object. It is now E407.

#### A5 — keyed lists in clones

A clone used to replace a tagged list wholesale. Tagged (keyed) lists now merge
by tag value; every other list still replaces.

```abstract
// before: capabilities became exactly [#export]
// after:  #search is kept and #export is appended
Product :: @id.merged
&base_a.*      // capabilities: [#search(availability: alpha)]
&base_b.*      // capabilities: [#export]
```

#### A6 — the `lang` to `lang_values` rename

A field named `lang` used to be emitted as `lang_values`. The shim is removed;
every field is emitted under its declared name.

#### A7 — `template` / `id` as schema fields

```abstract
// before: both were accepted and silently overwrote the envelope
schema Pack {
    template: text
    id: int
}

// after: 'template' is always E314. 'id' may be declared only as:
schema Pack {
    id: text            // or: id: text(3..24)
}
```

The declared range is honoured against the instance id (E413). Deleting the line
falls back to the implicit `id: text(1..64)`. Any other declaration of `id` —
another type, a modifier, a default, a list head — is E314.

#### A8 — `template:` / `id:` assignments

Both used to be accepted and forged the envelope. Both are now E410.

```abstract
// before
Product :: @id.atlas
    id: something_else

// after: E410; the id comes from the header or the file name
```

#### A9 — `@tag` at root

`@tag` on a root field used to consume the instance id and delete `id` from the
output. It is now E311: `@tag` belongs on a field inside a group.

#### A10 — several `@tag` fields per group

The extra ones silently became ordinary fields. A second `@tag` is now E310.

#### A11 — quoted numbers for `int` / `float` / `bool`

```abstract
// before: silently ignored, then a confusing error
count: "5"

// after: E412 — quoting never coerces
count: 5
```

#### A12 — absent optional list fields

```json
// before
"tags": []

// after: the key is absent entirely, like every other absent optional field
```

#### A13 — duplicate assignment to one path

Last writer won, silently. Two statements writing one path in overlapping
versions are now E429.

#### A14 — nested lists

Flattened into a string. Now E441.

#### A15 — an enum wildcard that matches nothing

Expanded to nothing and left the field as `[]`. Now E415.

#### A16 — `#prefix*` with a non-`id` tag field

Used to fail with an enum mismatch; now expands correctly.

#### A17 — unknown escapes in strings

```abstract
// before: "C:\pack" kept the literal \p
path: "C:\pack"

// after: E202. Write the path with '/' or '\\'
path: "C:/pack"
```

#### A18 — unknown `$variable`

Left as literal text. Now E425.

#### A19 — the `$name` boundary

Substring replacement meant `$named` became `<value of name>d`. A variable name
is now the maximal run of identifier characters; use `${name}` for explicit
boundaries.

```abstract
// before: $id_large substituted 'id' and left '_large'
// after:  $id_large names the field 'id_large'; write ${id}_large for the other
label: ${id}_large
```

#### A20 — multi-path prefix

```abstract
// before: a.b.{c,d} built a field literally named 'a.b'
// after: the prefix may be dotted and resolves normally
limits.tier.{soft, hard}: 10
```

#### A21 — text after `#tag(...)` or between tuple rows

Silently discarded. Now E210.

#### A22 — tuple rows without separating commas

Accepted. Now E210.

#### A23 — header detection

Any statement containing `::` could start an instance, so `window: 12::30`
silently began a new broken object. A line is now a header only when its first
token is a schema name and the next token is `::`.

#### A24 — asset paths

`..`, absolute paths and UNC paths escaped the project. All are now E424, and
there is exactly one resolution rule, relative to `assets/`.

#### A25 — `./assets/x`

```abstract
// before: resolved to <project>/assets/x
icon: ./assets/textures/a.png

// after: resolves to <project>/assets/assets/textures/a.png — drop the prefix
icon: ./textures/a.png
```

#### A26 — UTF-8 BOM

A BOM used to delete the first schema or break the first header. It is now
stripped at offset 0 and ignored.

#### A27 — `.AB` / `.ABT` files

Invisible to discovery. Extensions are now compared case-insensitively.

#### A28 — a directory named `Data`

Did not trigger the project-root rule. The `data` marker is now
case-insensitive.

#### A29 — instance ids

Case-sensitive and unnormalised. Ids are now normalised like every other
identifier, so `Atlas` and `atlas` collide (E402).

#### A30 — `@id.42`

Became an integer and defeated duplicate detection. Ids are identifiers and are
never type-inferred; `@id.42` is the string `"42"` (E428 when the value is not
an identifier).

#### A31 — reversed ranges and bad defaults

Accepted at schema time and failed later at some instance. Both are now checked
at the schema: E305 and E313.

#### A32 — non-ASCII identifiers

Half-normalised and silently distinct. Now E206.

#### A33 — the empty project

Compiled to `{"data": []}`. Now E103.

#### A66 — bare tuple multi-assignment

```abstract
// before: undocumented, accepted, duplicate columns silently overwrote
(a, b): (1, 2)

// after: removed. A tuple array requires a field path before the '('
copy(key, value): (en_us, Welcome)
```

#### A67 — header tag values

Type-inferred without the schema, so `@count.12` was always an integer. Header
tag values are now type-directed like every other value: `@count.12` is the
number 12 on an `int` field and the string `"12"` on a `text` field.

#### A68 — keyed lists

Duplicate tag values were accepted inside one list. Now E444.

#### A69 — values containing `,`

A comma was sometimes text and sometimes a separator. A depth-0 comma is now
**always** a list separator; quote the value to include one.

```abstract
// before: sometimes the string "Hello, world"
// after:  a two-item list, and E412 on a non-list field
caption: Hello, world

// write this instead
caption: "Hello, world"
```

#### A70 — values containing `//`

A spaced `//` truncated the value as a comment, silently. It is still a comment;
the value must be quoted.

```abstract
ratio: "50 // 2"
cdn: "//cdn.example.com/x.png"
```

---

### Breaking changes: logic

#### A34 — unknown paths in conditions

Silently false, so guards stopped guarding. Now E503.

#### A35 — `==` on text

Case- and `-`/`_`-insensitive. Comparison is now **exact**. Enum values are
stored normalised, so compare against lowercase, `_`-separated text.

```abstract
// before: matched "Active", "active" and "ACTIVE"
// after:  matches only "active"
if .status == "active" { }
```

#### A36 — numeric comparison of strings

`"10" > 9` compared numerically. Now E513: `<`, `<=`, `>`, `>=` require numbers.

#### A37 — truthiness

Any value could be a condition. A condition must now be boolean (E513).

#### A38 — `length()` on a scalar

Returned 1. Now E514 for numbers and booleans; on `text` it yields the Unicode
scalar count.

#### A39 — a bare `length(...)` as a condition

Meant `> 0`. Now E513; write the comparison.

```abstract
// before
if length(.tags) { }

// after
if length(.tags) > 0 { }
```

#### A40 — `exists` on a `text` field

Probed the filesystem when the value looked like a path. `exists` now never
touches the filesystem, for any type, in any mode.

#### A41 — bare field names as operands

```abstract
// before: compared the literal text "status"
if status == "x" { }

// after: paths start with '.'
if .status == "x" { }
```

#### A42 — `derive?` on a defaulted field

Silently never fired. Now E518.

#### A43 — `derive` into an array

Silently dropped, or crashed. Now E506.

#### A44 — loop variables without `$`

Allowed for exact-value derive only. Loop variables are always `$name`.

```abstract
// before
for item in .items { }

// after
for $item in .items { }
```

#### A45 — loop variables in messages

Only `$`-prefixed ones were substituted. Substitution is now uniform.

---

### Breaking changes: output and CLI

#### A46 — the envelope

```json
// before
{ "data": [ … ] }

// after
{
  "abstract": { "format": 1, "compiler": "1.0.0", "versions": { "min": 1, "max": 1 } },
  "data": [ … ],
  "overlays": []
}
```

#### A47 — instance order

Lexicographic source-path order, undocumented. Instances are now sorted by
`(template, id)`.

#### A48 — YAML keys

Unquoted, so `1`, `yes` and `no` changed type on the way out. Every mapping key
is now double-quoted.

```yaml
# before
data:
  - id: atlas

# after
"data":
  - "id": "atlas"
```

#### A49 — a YAML empty object inside an array

Rendered as `- ` (null). Now `- {}` — and unreachable in a conforming document,
because an empty group is omitted.

#### A50 — RAW

Arrays of objects were printed on one line and the top-level `{` was
unindented. RAW is now fully specified: four-space indentation, bare keys, an
object always multi-line, an array of scalars inline.

#### A51 — floats

Never used exponent notation, so `1e300` became a 300-digit literal. Floats are
now the shortest decimal string that round-trips, in exponent form outside
`1e-6 … 1e21`, and always carry a fractional part or an exponent.

#### A52 — `--allow-unknown`

Accepted undeclared fields. Removed; there is no lenient mode.

#### A53 — the direct form

```sh
# before
abstract a.ab T.abt JSON

# after
abstract compile a.ab JSON
```

#### A54 — single-file compile

Only loaded siblings of the named file, so `data/templates/*.abt` was invisible.
The whole project is now discovered, and only the named files' instances are
emitted.

#### A55 — unknown flags, positionals and formats

Silently ignored. Now E802, E804 and E805.

#### A56 — `--out=file`

Silently did nothing. Now supported.

#### A57 — `--out`

Overwrote any path, including sources, and still printed to stdout. It now
refuses source files, anything inside the data directory, and any `.ab`/`.abt`
file inside the discovery root (E808), and it suppresses stdout.

#### A58 — the trailing `true` write flag

Wrote a sibling file, sometimes inside the sources. Removed; use `--out`.

```sh
# before
abstract compile pack JSON true

# after
abstract compile pack JSON --out pack.json
```

#### A59 — `--skip-assets`

Did not skip absolute paths, changed `exists`, and therefore changed the
compiled data. It now skips exactly the on-disk checks (E421, E422, E423) and
the compiled bytes are identical with and without it.

#### A60 — piping into a reader that closes stdout

Panicked with exit 101. Now exits 0 quietly.

#### A61 — `templates`

Ignored duplicate-schema errors and printed a list anyway. It now runs phases
P0–P3 and fails on any schema error.

#### A62 — hidden and vendor directories

Walked, producing confusing duplicate-id errors. Directories whose name starts
with `.`, and `node_modules`, `target`, `build` and `out`, are now skipped.

#### A63 — symlink cycles

Walked forever. The walker now keeps a canonical-path visited set.

#### A64 — deep input

Overflowed the stack and aborted the process. Every limit in SPEC 3.7 is now
enforced and reported as E209.

#### A65 — diagnostic format

```text
# before
abstract: path: message

# after
data/products/atlas.ab:3:5: error[E412]: Type mismatch at atlas.count: expected int, found text.
```

#### A71 — `templates` flags

`--skip-assets` and `--allow-unknown` were accepted and ignored. Both are now
E802 for this command.

---

### New in 1.0

- **Versions and overlays.** `versions min..max` declares the project range;
  `@since(n)` and `@removed(n)` scope a field, a body statement or a whole
  instance; the compiled document carries the maximum version in `data` and
  every earlier difference in `overlays` (SPEC 4.12, 5.13, 5.14, 7.5).
- **Instance version windows** at the end of an instance header, and the
  `removed` list of an overlay, which names the base objects that do not exist
  in that range (SPEC 5.14, 7.5).
- **The built-in `version`** in logic conditions and `derive` expressions
  (SPEC 6.6).

  ```abstract
  logic Item {
      derive .schema_version = version
      if version >= 2 { derive .glow = true }
  }
  ```

- **The `ref(Schema)` type**: a value that is the id of another instance,
  checked to exist and to use the named schema, emitted as a string (SPEC
  4.4.8).

  ```abstract
  schema Sticker { pack: ref(Pack) }
  ```

- **List cardinality in a field head**: `tags[1..]`, `tags[2..4]` (SPEC 4.6).
- **Body blocks in instances**: `owner { … }` as sugar for a dotted prefix
  (SPEC 5.4).
- **Richer `#tag` arguments and tuple arrays**: dotted keys, nested `#tag`
  objects and bracketed lists as arguments, and a bracketed row list so a tuple
  array can span physical lines (SPEC 5.5).
- **`--max-errors`** (an integer from 1 to 10000; default 20) (SPEC 9.3).
- **Stable error identifiers** and the complete diagnostics catalogue
  (SPEC 10).
- **The golden-test conformance format** (SPEC 11.2).
- **An explicit `id` declaration** in the two spellings `id: text` and
  `id: text(a..b)`, which constrain the instance id (SPEC 4.11).

---

### Removed in 1.0

| Removed | Replacement |
|---|---|
| `--allow-unknown` and any lenient mode | declare the field, or delete the assignment |
| the direct form `abstract a.ab T.abt JSON` | `abstract compile a.ab JSON` |
| the trailing `true` write flag | `--out <file>` |
| the `data:` parser sentinel | nothing; `data` is an ordinary field name |
| the `lang` to `lang_values` rename | nothing; fields keep their declared names |
| bare tuple multi-assignment `(a, b): (1, 2)` | `path(a, b): (1, 2)` |
| trailing-comma line continuation | bracketed lists |
| cross-template clones | clone within one template |
| `@tag` at root, and more than one `@tag` per group | one `@tag` inside a group |
| nested lists | a list group, or a `$(Schema)` list |
| `null` in any form, in any format | key absence |
| non-ASCII identifiers | ASCII identifiers; text values may be any Unicode |

Deliberately still absent, and out of scope for 1.x: imports and includes, free-key
maps, user-defined functions, arithmetic, doc comments in output, output key
renaming, a migration DSL, per-schema version axes and list accumulation in
logic.

---

### Migration checklist

From SPEC Appendix A.5, in the order that causes the fewest re-runs:

1. Delete every `template:` field declaration from `.abt` files. Keep an `id:`
   declaration only when it reads `id: text` or `id: text(a..b)`; rewrite or
   delete any other, and check that every id fits the range you keep.
2. Move every `&clone` line to directly under its instance header.
3. Replace every trailing-comma line continuation with a bracketed list.
4. Unquote every value that feeds an `int`, `float` or `bool` field, and quote
   every text value that contains a `,`, a leading `[`, `#` or `"`, a leading
   `//` or a spaced ` // `.
5. Replace `./assets/x` asset values with `x`.
6. Replace `\` in quoted paths with `/`.
7. Add a leading `.` to every bare path operand in logic, and lowercase every
   string compared with `==` against an enum.
8. Remove `--allow-unknown` from build scripts and fix the fields it was hiding.
9. Replace `abstract file.ab T.abt JSON true` with
   `abstract compile file.ab JSON --out <file>`.
10. Re-read consumers: the envelope gained the `abstract` object and
    `overlays`, and absent optional lists are no longer `[]`. A consumer that
    wants a version other than `max` applies **every** overlay whose range
    contains that version, replacing or adding whole objects by `id` from the
    overlay's `data` and deleting the ids in its `removed`.

---

## 0.2.0

The previous release, superseded by 1.0.0. Its behaviour is described only where
1.0 differs from it, in the tables above and in SPEC Appendix A. Projects
written for 0.2.0 do not compile unchanged under 1.0; work through the migration
checklist.
