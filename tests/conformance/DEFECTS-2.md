# DEFECTS-2 — adversarial verification of the Abstract 1.0 compiler

Stage: VERIFY 2 (adversarial, read-only for `project/src`).
Compiler under test: `project/target/release/abstract.exe`, 1.0.0.
Normative source: `docs/SPEC.md` (and `docs/GRAMMAR.ebnf`). Every expectation below was
re-derived by hand from the spec section named in the entry, not from the compiler.

Conformance cases for every entry live in `conformance/adversarial-2/<id>/`.
Seven of them are `status = ready` and fail today; two are `status = pending`
because the resolution is a specification decision, not an implementation change.

`conformance/adversarial-1` and `conformance/DEFECTS-1.md` were written by the
VERIFY 1 stage while this one was running. The two sets are disjoint except for
the clone-chain limit, which both stages found independently; D2 below defers to
ADV-03 / ADV-03b for that and files only the boundary case they do not cover.

`cargo build` is warning-free, in both the dev and the release profile.
`cargo test`, as of this run: 508 unit, binary, CLI and compiler tests pass and
21 are ignored, none fail; the corpus
runner reports 262 cases, 228 passed, 3 pending, 12 without an expectation and 19
failed — 12 of those failures are VERIFY 1's ready cases and 7 are this stage's.
Every one of the 239 corpus cases that predate the two adversarial stages still
passes; nothing outside `conformance/adversarial-2/` was edited.

---

## D1 — Version annotations on a body statement are never range-checked (E603 missing)

Cases `adversarial-2/adv2-01`, `adv2-02`, `adv2-03`. Severity: major.

### Input

```abstract
versions 1..3

schema Item {
    name: text(1..40)
    v: text(1..40) @optional
}
```

```abstract
Item :: @id.x
    name: N
    v: a  @since(9)        // adv2-01
    v: a  @removed(9)      // adv2-02, written on its own in that case
    v: a  @since(0)        // adv2-03, written on its own in that case
```

### Expected per spec

SPEC §4.12 states the rule for every version annotation: "`n` MUST be within the
project range (`min <= n <= max`), otherwise **E603**". SPEC §5.14 restates it for
instance windows. SPEC §11.1 then fixes the precedence explicitly — "**E603 before
E430 and E440**" — and that pair can arise together on nothing but a body
statement, because E430 is defined for no other construct (SPEC §10.4). All three
inputs are therefore E603, reported at the annotation:
`Version 9 is outside the project range 1..3.`

### Actual

| Input | Reported |
|---|---|
| `@since(9)` | `E430`: `This statement can never apply: v exists in versions 1..3, the statement is annotated none.` |
| `@removed(9)` | **nothing — exit 0**, document identical to the unannotated statement |
| `@since(0)` | `E430`, same message |

E603 *is* implemented for schema field annotations (`src/schema.rs`) and for
instance version windows (`src/resolve.rs`, `check_instance_window`), so the gap is
specific to body statements: `check_statements` resolves `statement.window` against
the project range and never validates the numbers inside it.
`VersionWindow::resolve` clips to the project range, which turns an out-of-range
`@since` into the empty set (misreported as E430) and makes an out-of-range
`@removed` disappear entirely (reported as nothing at all).

The `@removed(9)` shape is the damaging one: a typo for `@removed(2)` ships a
document in which the statement applies to every version, with no diagnostic. That
is the class of silent behaviour SPEC §1.2 and decision R4-5 ("nothing in the
language is silent") exist to exclude.

Two secondary observations on the same code path, not filed as separate cases:

- The E430 message template (SPEC §10.4) is `… the statement is annotated {b}.`
  The implementation substitutes the annotation range **after** intersection with
  the project range, so `{b}` renders `none` whenever the annotation is out of
  range — the diagnostic never names the annotation the author actually wrote.
- `v: a @since(3) @removed(2)` (reversed window, both numbers in range) is
  reported as E430, where the identical window on an instance header is correctly
  **E604** (`@removed(2) must be greater than @since(3).`). SPEC §5.13 does not
  restate E604 for statements, so this one is arguable; it is noted here because a
  fix for D1 should decide it deliberately rather than by omission.

---

## D2 — The clone-chain depth limit: independently reproduced, plus the at-limit half

Case `adversarial-2/adv2-04`. Severity: minor (the major half is already filed as
`adversarial-1/adv-03` and `adv-03b`).

VERIFY 1 landed `conformance/adversarial-1` while this stage was running and its
ADV-03 / ADV-03b pair covers the same defect this stage found independently: the
§3.7 clone-chain limit is enforced against the recursion depth of the *memoised*
clone resolver rather than against chain length, and resolution order is
`(template, id)` (SPEC §2.7), so whether E209 fires depends on how the ids sort
relative to the direction of the chain. Reproduced here at greater scale — with
zero-padded ids so that lexicographic and numeric order agree — to bound the
consequence:

| Chain length (clone statements) | 63 | 64 | 65 | 70 | 1000 | 5000 |
|---|---|---|---|---|---|---|
| ids ascend along the chain (deep end resolved last) | ok | ok | ok | **ok** | **ok** | **ok** |
| ids descend along the chain (deep end resolved first) | ok | **E209** | E209 | E209 | E209 | E209 |

A chain of 5000 compiles and emits 5001 objects when the ids ascend, so the limit
that SPEC §3.7 justifies by "a hostile or corrupted source can never exhaust the
stack" is bypassable by identifier spelling alone. The stack is still bounded in
the recursive direction, so this is a conformance defect rather than a crash.
No new case is filed for that half; `adversarial-1/adv-03` already pins it.

`adversarial-2/adv2-04` files the part the pair does not cover: the **boundary**.
The bottom row above turns red at 64, not 65. SPEC §3.7 requires E209 only "when
one is exceeded" and SPEC §5.7 says "Chains deeper than 64 are E209", so a chain
of exactly 64 clone statements is legal and must compile. The case declares
`q000` … `q064`, each cloning its successor — 64 clone statements, transitive
clone depth 64, ids running against the chain so that the check is actually
reached — and expects a successful compile:

```text
data/items/chain.ab:257:1: error[E209]: Clone chain exceeds the limit of 64.
```

One statement shorter (63 edges) it compiles, so only the boundary is wrong. This
is the same off-by-one family as `adversarial-1/adv-04` (64 nested brackets) and
`adv-06` (64 nested logic blocks), and it is filed separately so that a structural
fix for ADV-03 cannot land while still rejecting a legal depth-64 chain.

---

## D3 — `&other.id` reports E408 before the E410 the spec prescribes

Case `adversarial-2/adv2-05`. Severity: minor.

### Input

```abstract
Item :: @id.src
    name: Src

Item :: @id.dst
&src.id
    name: Dst
```

### Expected per spec

SPEC §5.7, *Identity is never cloned*: "`&other.id` and `&other.template` are
**E410**: the path exists on the source, but it is a reserved envelope key and
cannot be written." The same sentence both names the identifier and denies the
E408 precondition (that the path is absent on the source), so E410 is the only
diagnostic this construct may raise, and by SPEC §11.1 it is the one a compiler
reporting a single diagnostic must report.

### Actual

```text
data/items/x.ab:5:1: error[E408]: Clone path 'id' does not exist on instance 'src' in any version.
  note: versions checked: 1.
data/items/x.ab:5:6: error[E410]: 'id' is a reserved envelope key and cannot be assigned.
```

E408 wins on source position (5:1 before 5:6) within the same phase, and its
message is untrue of the source: `src` has an id in every version.
`&src.template` behaves identically. The envelope keys are not part of the
authored object the clone resolver searches, so the absence test fires before the
reserved-key test.

---

## D4 — `schema` / `logic` / `versions` in a `.ab` file do not report E210

Cases `adversarial-2/adv2-06` (after a header), `adv2-07` (before the first header).
Severity: minor.

### Input

```abstract
Item :: @id.x
    name: N
schema Other {
}
```

```abstract
versions 1..2

Item :: @id.x
    name: N
```

### Expected per spec

SPEC §2.1: "Encountering `schema`, `logic` or `versions` at statement position in a
`.ab` file, or an `instance_header` in a `.abt` file, is **E210**." Both inputs are
E210. The mirror direction is implemented correctly — an instance header inside a
`.abt` is E210 (`Unexpected 'T' here; expected 'schema', 'logic' or 'versions'.`).

### Actual

| Placement | Reported |
|---|---|
| after an instance header | `E316`: `Invalid field name 'schema Other {'.` (same for `logic …` and `versions …`) |
| before the first header | `E403`: `A statement cannot appear before the first instance header.` |

Neither identifier is E210, and both messages misdescribe the input. E316 is the
schema-side *Invalid field name* identifier (SPEC §10.3) and the `{text}` it
substitutes is a whole physical line including the block brace, which is not a
field name. E403 is defined by SPEC §5.1 for a body statement or clone statement
before the first header; `versions 1..2` is neither, so the diagnostic points the
author at a missing header rather than at the file kind. One construct that the
spec assigns a single identifier thus produces two different wrong ones depending
on where in the file it sits.

Root cause: a `.ab` body line with no `:` falls through to the field-name path, so
*any* non-statement line inside an instance body is reported as an invalid field
name. Only the three keywords SPEC §2.1 names are pinned by the spec.

---

## Specification issues (implementation follows the text; the text is inconsistent)

### S1 — U+007F in a text value makes YAML output unparseable

Case `adversarial-2/adv2-08`, `status = pending`. Severity: major.

Source `caption: "a<U+007F>b"` — legal under SPEC §3.5, which restricts a quoted
string only by requiring it to close on its own physical line.

SPEC §8.8 lists the escapes exhaustively: `"`, `\`, U+0008, U+0009, U+000A,
U+000C, U+000D, and "any other **C0 or C1** control character" as a lowercase
`\u00XX` escape. U+007F DELETE is in neither range (C0 is U+0000..U+001F, C1 is
U+0080..U+009F), so §8.8 as written says to emit it literally — which the compiler
does, in all three formats.

SPEC §11.3 requires the conformance runner to "parse `out.json` as JSON and
`out.yml` as YAML in addition to comparing bytes, so that a syntactically invalid
document cannot pass". YAML 1.2 excludes U+007F from `c-printable`, so a literal
DELETE inside a double-quoted scalar makes the document unreadable. Verified with
PyYAML 6.0.2:

```text
yaml.reader.ReaderError: unacceptable character #x007f: special characters are not allowed
```

JSON is unaffected (JSON permits an unescaped U+007F) and RAW is never parsed, so
the conflict is specific to YAML. The neighbouring C1 control U+0085 *is* escaped
correctly as `\u0085`, which is what makes the omission read as a boundary error
rather than a design choice.

Recommended resolution: extend §8.8's escaped set to U+007F — or, more robustly,
to every scalar value outside YAML's `c-printable` — and then change the
implementation to match. `conformance/adversarial-2/adv2-08/expected.yml` carries
the escaped rendering, which is the only one under which §8.8 and §11.3 can both
hold; it parses and round-trips to the same string.

Note for the conformance suite: a runner that only compares bytes cannot catch
this. Only the YAML-parse leg of §11.3 does.

### S2 — A header tag with an empty applicability set is a silent no-op

Case `adversarial-2/adv2-09`, `status = pending`. Severity: minor.

```abstract
versions 1..3

schema Item {
    name: text(1..40)
    glow: bool @since(2) @optional
}
```

```abstract
Item :: @id.x, @glow @removed(2)
    name: N
```

`glow` exists in versions 2..3; the instance window is version 1 only. The header
tag assigns a field that exists in no version in which the instance exists, so it
writes nothing anywhere. Exit 0, no diagnostic, document identical to the one
produced with the tag deleted.

Written as a body statement instead, the same assignment is an error:

```text
data/items/x.ab:3:5: error[E440]: This statement applies to versions 2..3, but instance 'x' exists only in versions 1.
```

SPEC §5.2 calls a header tag "a compact assignment to a root field" and §7.3 step 1
applies header tags and body statements by the same plain assignment, but §5.13 and
§7.2 phrase the applicability requirement for *body statements* only, so the text
does not reach this construct. The implementation documents the silent behaviour
deliberately in `check_statements`.

Recommended resolution: either extend the §7.2 / §5.13 applicability requirement to
header tags (E440 here, E430 where the tag targets a field that exists in no version
at all), or say in §5.2 that a header tag whose applicability set is empty writes
nothing. Leaving it unstated makes one assignment an error and the other a no-op on
the basis of syntax alone, against decision R4-5.

---

## Attack surface covered with no discrepancy found

Recorded so a later stage knows what has already been re-derived by hand and
matched. Everything in this list was checked against the spec section named and
agreed byte for byte or diagnostic for diagnostic.

**Clones (§5.7).** Keyed-list merge across two sources (order of `t` preserved,
same-key elements merged recursively, new keys appended in `s` order); an instance's
own list assignment replacing a cloned list wholesale; a clone and a statement
writing one path not being E429; partial clone of a subtree; interpolation after
cloning resolving to the *cloning* instance's id; cloned root scalars entering the
variable table; cloning a source whose statements are version-scoped (content
follows the version being compiled, and the cloner acquires the source's overlays);
E405/E406/E407/E408 with the `versions checked` note, E437 on a clone, E440 when the
source window is narrower than the cloner's; wildcard element keys expanded before
folding; a keyless element plus a defaulted tag surfacing as E444 at re-validation;
memoisation not changing results.

**Defaults (§4.10, §7.3 step 4).** Defaults filled before logic; defaults not filled
inside an absent optional group; a required group with fully defaulted children
still E411; an empty body block leaving a group absent; default interpolation
against the *authored* table only (a default referencing another defaulted field is
E425, and one referencing a nested field is E425); list defaults in both spellings;
E313 for a list default on a non-list field; E320 before E313.

**Interpolation (§5.11).** Single pass with no rescanning (`$$id` inserted verbatim);
`$$`, `${name}`, maximal identifier run including `-`; E425/E426/E427 and `${}`;
interpolation at any depth — tag arguments, tuple cells, nested groups; interpolation
before brace expansion, with braces arriving from a variable staying literal; the
per-version variable table (a value referencing a version-scoped field is E425 in the
versions where that field is unset).

**Type-directed interpretation (§5.10).** Quoted numerics staying text; `"5"` on
`int` E412; a float literal on `int` E412; an integer literal on `float` accepted;
`1.21.5` staying text; a bare comma list and a bracketed value on a non-list field
E412; single-value-to-list coercion after validation; enum normalisation; enum
wildcards in all three legal positions and E414/E415 outside them; tuple arrays
(E416 arity, E416 empty columns, E417, E409); `#tag` arguments with dotted paths,
nested lists and bare flags, E210 after the closing paren, E418, E419, E444; text
ranges counting Unicode scalar values (skin-tone emoji = 2).

**Versions and overlays (§4.12, §5.13, §5.14, §7.4, §7.5).** Both worked examples of
the spec reproduced byte for byte; per-instance run splitting with alternating values
over five versions; overlapping overlay ranges; a window strictly inside the project
range; `removed` lists and their ascending order; overlay ordering by (min, max);
signed-zero structural inequality; a value gap ("removed then re-added" at statement
level); `@since`/`@removed` on groups and list groups; nested `$(Schema)` windows
intersecting (R3-8); E605 when a child outlives its group; E411 in the versions an
annotation excludes; E429 over intersecting applicability sets and its `for
version(s)` rendering; E430 before E440; single-file mode producing the same objects
and only the named instances' overlays.

**Logic (§6).** Evaluation order (authored checks, defaults, nested logic, own logic,
required check, full re-validation); nested `$(Schema)` logic through inline groups
and through chains, and *not* running for an object the parent's logic created;
`$name` in a derive value resolving against the current object at the moment the
statement runs; `exists` on an empty list (false), an empty string (true) and an
absent field (false); absent operands making every comparison false including `!=`;
`contains` on lists and substrings; projections (`contains` allowed, `==`/`!=`
E519 statically); indices and out-of-range reads; dynamic path segments with E503
after them; `length()` on lists, text and projections, E514 statically; the `version`
built-in in conditions, in `derive`, and against a declared field named `version`;
E521 for a write to a field absent in the version; E518, E509, E516, E517, E520,
E513, E512, E515 with its instance/version/index notes; loop-variable interpolation
in `derive` values and `throw` messages; derived values re-validated (E413, E445);
first failing `require` reported for the lowest version.

**Output (§8).** Float rendering across the whole §8.7 decision tree (`0.0`, `-0.0`,
`3.0`, `1.0e+21`, `1.5e-7`, `0.000001`, `1.0e-7`, `1e20`, i64 extremes); §8.8
escapes including `\b`, `\f`, `\u000b`, `\u0085`; key order from declaration order in
all three formats, with a declared `id` emitted once in envelope position; YAML
mapping keys always quoted (numbered keys, `yes`/`no`/`on`/`off`); nested sequences
under sequence items; RAW inline scalar arrays versus multiline object arrays;
`(template, id)` ordering compared as Unicode scalar values; exactly one trailing LF
and no trailing whitespace in all three formats.

**Lexing and structure (§2, §3).** BOM at offset 0 stripped in both file kinds and
E210 elsewhere; CRLF accepted, lone CR E210; E102 on invalid UTF-8; E206/E207;
comments only after whitespace or at line start, with URLs and `12::30` intact;
E210 for a value emptied by comment stripping and for text after a closing quote;
E442 at depth 0 and header-tag continuation on a trailing comma; E436, E439;
annotation adjacency (`@since (2)`, `@x(1)`, bare `@since`, quoted `"@since(2)"`);
every keyword of Appendix B.3 used as a field name, including `version`, `not`,
`true` and `text`; depth limits for values, paths, documents and logic blocks;
discovery skipping dot-, `node_modules`, `target`, `build` and `out` directories and
matching the `data` marker case-insensitively.

**Assets (§4.4.6, §4.4.7, §5.9).** Header probing for png/gif/bmp/webp-VP8/VP8L/VP8X/jpg
at their exact byte gates, truncation and malformed-width notes, JPEG fill-byte
skipping and the 65 536-byte budget, `E422` for an unrecognised signature naming the
first bytes; E420/E421/E424 and the assets note; `--skip-assets` skipping exactly
E421/E422/E423 and nothing else, with identical compiled bytes.

**CLI (§9).** Exit codes 0/1/2; E801, E803, E805, E808, E811, E812, E806 for a named
`.abt`; `--max-errors` default 20, an explicit small value, and `0` meaning no limit,
with the `stopping after {n} errors` line; quiet exit 0 on a broken pipe; determinism
across project root, data directory and both file orders.
