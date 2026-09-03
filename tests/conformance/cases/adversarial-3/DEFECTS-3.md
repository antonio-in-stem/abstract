# DEFECTS-3 — independent verification of the Abstract 1.0.1 rulings

Stage: VERIFY 3 (adversarial, read-only for `src/` and `docs/`).
Compiler under test: `target/release/abstract.exe`, 1.0.1, built from
`antonio-in-stem/abstract-1-0-1` at the "settle the remaining specification notes" commit.
Normative source: `docs/SPEC.md` and `docs/GRAMMAR.ebnf` as that commit left them.

Scope: the five rulings of the Revision 7 section of `docs/revision-log.md` — **S1** (an empty
`data` array), **S4** (E511's reachable reasons), **S5** (blank and comment lines before `else`),
**S7** (the logic work limit and E523) and **S8** (E413's `{value}` and `{ranges}`). Every
expectation below was re-derived from the specification text as it now stands, then put to the
built binary; nothing was taken from the previous stage's report.

Conformance cases live in `adversarial-3/adv3-01` … `adv3-19`, beside this file. Four of them are
`status = ready` and fail today (`adv3-01`, `adv3-02`, `adv3-05`, `adv3-06`); one is `pending`
because its resolution is a specification decision (`adv3-16`); the other fourteen pass and pin
edges the corpus did not previously reach.

Test state with these cases present: 439 library unit tests, 26 binary unit tests, 43 CLI tests,
28 compiler tests (21 still `#[ignore]`d), 1 doc test and the documentation-example runner all
pass, unchanged. The corpus runner reports **301 cases: 287 passed, 1 pending, 9 without an
expectation, 4 failed** — the four failures are this stage's ready cases and nothing that predates
it changed verdict. `cargo build --release` is clean.

Mechanical checks over `docs/`, run with the revision-6/7 spec-check script: catalogue **123 rows,
no duplicates, no orphans, nothing used but uncatalogued**, the only family gap the reserved E809;
90 distinct section references, all resolving; 63 grammar rule names cited on `Grammar:` lines, all
defined; 13 JSON examples parse; no `abstract` block mixes `.abt` and `.ab` constructs. The script
exits clean, so E523 is fully wired into the catalogue and the prose.

---

## D1 — A malformed condition with a bare right-hand side never reaches E511: the third `=` puts the lexer into value mode and swallows the block brace

Cases `adversarial-3/adv3-01`, `adv3-02`, `adv3-03`. Severity: major.

### Input

```abstract
logic T {
    if .name === b {
        derive .ready = true
    }
}
```

### Expected per spec

SPEC §6.6, as ruling S4 rewrote it, names the two reachable reasons of E511 and gives this exact
shape as the first example of the first reason: "a token that **cannot begin an operand** where an
operand is due — `.a === "b"`, whose third `=` lands in operand position". SPEC §3.6 makes the `{`
that opens a logic block a block brace, and `GRAMMAR.ebnf` gives `=` to `derive_statement` alone —
`if_statement`, `require_statement` and `for_statement` have no `=` and no value, so there is no
bare value run on this line in which a brace could be an ordinary character. The one diagnostic
this file may raise is **E511** at the third `=`.

### Actual

```text
data/templates/T.abt:8:20: error[E203]: Unclosed '{' opened at 8:20.
data/templates/T.abt:11:1: error[E205]: Unexpected '}'.
```

No E511 at all, and both diagnostics name braces the author balanced correctly. The second one is
positioned at the `}` that closes the `logic` block, three lines below the fault.

### What decides it

`src/lexer.rs::scan_logic_line` tests `at_value_introducing_equals()` on **every** logic statement
line. That predicate's own doc comment scopes it to "a schema default (SPEC §4.3) or a `derive`
right-hand side (SPEC §6.4)", but the caller does not: a lone `=` anywhere in a logic head starts
`scan_derive_value()`, which consumes the rest of the logical line as a value. Inside a value run
SPEC §3.6 makes `{` and `}` ordinary characters that must balance, so the block's `{` becomes an
unbalanced literal brace (E203) and the block's own `}` is then consumed as its match, leaving the
`logic` block's `}` with nothing to close (E205).

The fault is invisible whenever the right-hand side is quoted, which is why it survived the S4
ruling: the value run ends at the closing quote, the block brace survives, and `.name === "b"` —
the spec's own example — does report E511. The shapes that reach E203/E205 instead:

| Condition, inside `if … {` | Reported |
|---|---|
| `.name === "a"` | E511 (quoted right-hand side) |
| `length(.name) === "a"` | E511 |
| `.name == abc` | E511 (one `=`, no value mode) |
| `.name === b` | **E203, E205** |
| `.name === 3` | **E203, E205** |
| `length(.name) === 3` | **E203, E205** |
| `.name == = 3` | **E203, E205** |
| `.count === 3` | **E203, E205** |
| `for $v in === 3 {` | **E203, E205** |

`require` shows the same mis-lexing without the brace damage, because a `require` line ends in no
block: `require .name === 3 else throw "no"` does report E511, but its quoted condition is
`.name == = 3 else throw "no"` — the value run ran to the end of the logical line and took the
`else throw` clause with it (D2, `adv3-03`).

Restricting the `at_value_introducing_equals()` call in `scan_logic_line` to a `derive` / `derive?`
line leaves the block brace a block brace and lets the condition parser reach the token, which is
where SPEC §6.6 puts the diagnostic.

---

## D2 — E511's `{text}` is not the condition as written: it is re-spelled from the token stream

Cases `adversarial-3/adv3-03`, `adv3-04`. Severity: minor.

### Expected per spec

SPEC §6.6: "A malformed condition is E511, whose message **quotes the condition as written** and
names the reason." SPEC §11.2 compares a case's `err.txt` byte for byte, so the rendering is
normative.

### Actual

| Written | Quoted in the message |
|---|---|
| `and .name == "a"` | `and.name == "a"` |
| `or .name == "a"` | `or.name == "a"` |
| `.name === "a"` | `.name == = "a"` |
| `.name == "a" .other` | `.name == "a".other` |
| `length(.name) zzz 3` | `length (.name) zzz 3` |
| `() == .name` | `() ==.name` |
| `.name === 3 else throw "no"` (in a `require`) | `.name == = 3 else throw "no"` |

`src/logic.rs::spell_condition` walks the tokens and `append_spelling` decides the spacing from the
token kind alone: `.` `[` `]` `)` `,` `$` are always tight to their left and everything else takes
a space. So the renderer inserts spaces the source did not have and removes spaces the source did,
and the two faults meet in `and.name`, which reads as a path segment of a field named `and`. The
last row is D1's swallowed clause.

Slicing the source between the first and last token of the condition would quote it as written and
would need no spacing table at all. The identifier and the reason are right in every row, so this
is a message defect, not a classification one, and the cases pin it in `expected_note` rather than
in `expected-error.txt`, which compares identifiers only.

---

## D3 — An `else` after the closing `}` of a `for` block is E210, not the E517 SPEC §6.3 prescribes

Cases `adversarial-3/adv3-05`, `adv3-06`. Severity: major.

### Input

```abstract
logic T {
    for $v in [1, 2] {
        derive .count = $v
    }
    else {
        derive .ready = false
    }
}
```

### Expected per spec

SPEC §6.3, as ruling S5 rewrote it: "An `else` that does not immediately follow a closing `}` of an
`if` is E517: an `else` that begins a logical line of its own — at the top of a block, or **after a
block that is not an `if`** — has no `if` to attach to." The same bullet reserves E210 for the
other spelling: "An `else` that follows some **other** statement is a different fault: rule (b)
joins it to that statement rather than ending the line, so the malformed logical line is reported
where it is malformed, as E210". A `for` block is a block, and the sentence names that case
explicitly, so this file is **E517** at the `else`.

### Actual

```text
data/templates/T.abt:11:5: error[E210]: Unexpected 'else' here; expected end of line.
data/templates/T.abt:14:1: error[E210]: Unexpected '}' here; expected 'schema', 'logic' or 'versions'.
```

`adv3-06` writes the same program with a blank line and a comment line between the `}` and the
`else` — the spelling S5 legalised for an `if` — and gets the identical pair three lines lower, so
rule (b) does join the lines and the classification is what differs, not the continuation.

The other two shapes the same bullet governs are right: an `else` at the top of a block is E517,
and an `else` after a `derive` is E210. Only the non-`if` block case is misclassified, and it is
the one the S5 ruling added to the text.

The parser reports E517 only where it is looking for a statement head and finds `else`; after a
`for` block it is still finishing that statement's logical line, so the generic end-of-line check
fires first. Testing for `else` there, and raising E517 when the block just closed is not an `if`,
puts the two spellings of the bullet on the same identifier.

---

## D4 — SPEC §8.1 counts the ways to an empty `data` array as exactly two; a third is reachable

Cases `adversarial-3/adv3-09`, `adv3-10`. Severity: minor. This is a defect of the text, not of
the compiler: both cases compile, and both pass.

### Expected per spec

SPEC §8.1, as ruling S1 rewrote it: "An empty `data` array is reachable in **exactly two ways**,
and both of them are successful compiles" — single-file mode over files that declare no instances,
and "a whole-project compile of a project whose sources **declare schemas** but no instances" —
followed by "A project with **no source files at all** is neither: discovery collects nothing,
which is E103."

### Actual

A whole-project compile of a project whose one source file declares **neither** a schema nor an
instance is a fourth thing, outside all three sentences, and it succeeds with the same empty
document:

| Project | Result |
|---|---|
| one zero-byte `.abt` (`adv3-09`) | exit 0, `data` and `overlays` empty |
| one `.abt` holding `versions 1..3` (`adv3-10`) | exit 0, `data` empty, envelope `min` 1 `max` 3 |
| one `.abt` holding only a `//` comment | exit 0, `data` empty |
| one `.ab` holding only a `//` comment | exit 0, `data` empty |
| a directory with one `README.txt` and no source | E103 |
| an empty directory | E103 |

The behaviour is right and agrees with SPEC §2.4 — discovery collected a file, so E103 cannot
apply — and with SPEC §7.2, which makes P3 depend on no instance. What is wrong is the count and
the second bullet's condition. Widening it to "a whole-project compile that discovers source files
and declares no instance" covers all four rows and leaves the E103 sentence exactly as it is.

`adv3-10` also fixes a consequence worth having pinned: the envelope's `versions` object is taken
from a file that declares nothing else, so an author can set the project range in a file that has
no schema in it.

---

## D5 — E523 carries one positionless loop note per enclosing `for`, which SPEC §10.5 does not describe

Case `adversarial-3/adv3-16`, `status = pending`. Severity: minor.

### Expected per spec

SPEC §10.5's E523 row prescribes the message and "**a note at the instance header**". SPEC §9.8
permits "zero or more `note:` lines", so a longer tail is not illegal; but SPEC §11.2 compares
`err.txt` byte for byte, which makes whatever is emitted normative for every conforming suite.

### Actual

Nine five-level chains (999 990) followed by a tenth, so that the crossing charge falls four levels
down rather than at the top of a block:

```text
data/templates/T.abt:110:21: error[E523]: Logic work limit exceeded at x: the logic executed more than 1000000 loop iterations in version 1.
  data/items/x.ab:1:1: note: instance 'x'.
  note: at index 0, item 1.
  note: at index 0, item 1.
  note: at index 0, item 1.
  note: at index 0, item 1.
```

Four identical, positionless notes, one per enclosing `for`. The count follows the depth: the
corpus's `revision-7/r7-01`, whose crossing falls one level down, carries one such note, and at
SPEC §3.7's 64-deep block limit the tail would be 63.

`src/logic.rs::loop_notes` appends one note per live loop binding to every diagnostic the evaluator
raises. That reads well on E522, where the offending value *is* the loop item, and on E515, where
the failing `require` may be inside a loop. On E523 the position already names the `for` whose
iteration crossed the bound, and the enclosing loops are all on the iteration the position implies.
The case is `pending` because the choice — name the tail in SPEC §10.5, or drop it from E523 — is a
specification decision rather than an implementation one.

---

## Re-derived and confirmed: what the rulings got right

Recorded because each was checked against the built binary rather than assumed, and because the
cases that pin them are the corpus's first coverage of these edges.

**S7, the arithmetic and the boundary.** `C(k) = 10 * (1 + C(k+1))` with `C(5) = 10` gives 111 110
for a five-level chain over ten-element lists. Nine chains and a flat loop of ten cost exactly
1 000 000 and **compile** (`adv3-11`); the same program with an eleven-element tail costs 1 000 001
and is E523 **at the tail loop**, on its eleventh iteration (`adv3-12`). The corpus's previous
under-the-bound case was 111 110, an order of magnitude short of the boundary, and SPEC §11.3 asks
for the limit "from both sides".

**S7, where the counter lives.** A nested `$(Schema)` block spends the same budget as the
instance's own block: 555 550 in each is 1 111 100 and is E523 inside the outer block, where a
per-evaluator budget would have compiled the file (`adv3-13`). The budget is fresh for the next
instance (`adv3-14`, two instances of 555 550) and for the next version (`adv3-15`, one instance
over `versions 1..2`). Nothing but a `for` body is charged: a `for` over a list field that is
absent runs zero times and costs nothing, and a program at 999 990 plus such a loop still compiles.
An instance stopped by E523 does not stop the compile — a second, independently broken instance is
still reported.

**S8, `{value}` and `{ranges}`.** Against a `text` range `{value}` is the scalar count and the text
never appears. A value of U+1F600, U+0065, U+0301, U+1F600, U+0061 — five scalar values, four
grapheme clusters, seven UTF-16 code units, eleven UTF-8 bytes — reports `5 is not in 1..3`
(`adv3-17`), so all three wrong counts are excluded. The same holds inside a group
(`atlas.owner.tier`), at an element of a list group with its 0-based index
(`atlas.copy[2].tier`, `adv3-19`), on a value written by `derive`, and on an id — `8 is not in
1..4` against a declared `id: text(1..4)`, `70 is not in 1..64` against the implicit range. An
empty string is `0 is not in 1..3`. `{ranges}` collapses an equal-bounded integer part to the bare
number and keeps both bounds of a float part: `text(1..3, 8, 12..14, 20..20)` renders
`1..3, 8, 12..14, 20` (`adv3-18`), `text(2..2)` and `text(2)` both render `2`, and
`float(0.5..0.5, 2.0..3.0)` renders `0.5..0.5, 2.0..3.0`. An `int` range renders the number itself.
Nothing here disagrees with SPEC §9.8; S8's claim that the fix was text-only holds.

**S5, rule (b) where it applies.** An `else`, an `else if` and a `require`'s `else throw` are each
reached across two blank lines and a comment line (`adv3-07`, `adv3-08`), and a trailing comment on
the `}` line does not break the join either. An `else` at the top of a block is E517; an `else`
after a `derive` is E210, with or without blank and comment lines in between; an `else` after the
`logic` block's own closing `}` is E210 at file level, so rule (b) does not reach outside a logic
block. The one spelling that disagrees with the text is D3.

**S4, the withdrawn reason.** No parenthesis fault reaches E511. An unclosed `(` in a condition is
E203, a `)` with nothing open is E205 (followed by an E210 as the parser recovers), and a `]`
closing a `(` is E204 — each from the lexer, before any condition is parsed. A condition may be
written across physical lines while a `(` is open and compiles. So the branch the ruling removed
is genuinely unreachable through the lexer, and the two reasons the ruling kept are the only two
the implementation can produce — subject to D1, which is about conditions that never reach the
condition parser at all.

**S1, the behaviour.** A project of `.abt` files that declare schemas and logic and no instance
compiles to the empty document, `abstract lint` prints `abstract: ok`, `abstract templates` lists
the schemas, and a defect inside such a logic block is still reported (E505 on a `derive` target
that names no field), which is SPEC §7.2's "P3 depends on no instance". Single-file mode over an
`.ab` that declares no instance emits the same empty document. Only the count in SPEC §8.1 is
wrong (D4).
