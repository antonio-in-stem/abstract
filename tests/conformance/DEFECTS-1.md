# DEFECTS-1 — adversarial verification of the Abstract 1.0 compiler

Scope: lexer and parser robustness, resource limits, determinism. Every finding
below was reproduced against `abstract-1.0/project` at the state of this run,
with the debug binary (`target/debug/abstract.exe`) unless a defect is noted as
profile-dependent, and re-checked against the release binary. Each defect has a
case under `conformance/adversarial-1/`; the case id is given with the entry.

The compiler was not modified.

## Method and coverage

| Probe | Cases run | Panics / crashes / hangs |
|---|---|---|
| Depth and size limits (SPEC §3.7): brackets, path segments, groups, `$(Schema)`, logic blocks, clone chains, document depth | 29 | 1 (D2) |
| Lexical edge cases: quotes, every escape, bracket and brace combinations, comments, BOM, CRLF, lone CR, Unicode whitespace, control characters, numeric literals at the i64/f64 boundaries | 91 | 0 |
| Byte-level: invalid UTF-8 (truncated, overlong, surrogate, Latin-1, UTF-16), NUL, noncharacters, 4 MB values, 200 000-element lists | 51 | 0 |
| Schema-side: type keywords, ranges, image size tokens, list cardinalities, modifiers, reserved names, defaults, `versions` | 101 | 1 (D1) |
| Logic-side: nested parentheses, `not` / `!` chains, 100 000-term `and`, nested `for`, every statement and operator form | 61 | 1 (D2), 1 hang (S7) |
| Image header probing (SPEC §4.4.7.1): truncated, zero, extreme and malformed headers for all five formats, JPEG fill runs and zero-length segments | 44 | 0 |
| Asset path confinement (SPEC §5.9): `..`, absolute, UNC, drive-relative, extended-length, alternate data streams, device names, trailing dot and space, 300-segment paths | 34 | 0 |
| Projects and CLI: empty and vendor-only trees, junction loops, aliasing junctions, junctions out of the project, uppercase extensions and `DATA`, every command and flag combination of SPEC §9 | 45 | 0 |
| Determinism: repeated runs, root spellings, argument orders, file creation orders, `--skip-assets`, single-file vs whole-project, `--max-errors` | 30 | 0 |
| Mutation fuzzing of a valid multi-version project (5 400 runs, three output formats) | 5 400 | 0 |

Determinism held everywhere it was probed. Output is byte-identical across
repeated runs, across the eight root spellings of one project (`.`, `./`,
`data`, `data/`, `./data`, `..` from inside, an absolute path, and a named
`.ab`), across command-line argument orders, and across two creation orders of
200 source files. Source-path ordering, `(template, id)` output ordering and
`templates` output are all by Unicode scalar value, including the cases where a
case-insensitive or locale-aware sort would differ (`Zebra` before `apple`,
`data/Z.ab` before `data/a.ab`). Diagnostic columns count Unicode scalar
values, verified with astral-plane, CJK and combining-mark text before the
error position. Single-file and whole-project compiles produce identical bytes
for the instances common to both, and `--skip-assets` changes only which errors
are reported. A junction cycle inside the discovery root terminates, a file
reachable by two paths is collected once, and a source reached through a
junction is reported under its walk path, never a canonical or extended-length
path.

The one exception to that record is D4, where a diagnostic depends on the
spelling of instance ids rather than on the program's structure.

---

## D1 — A wide `versions` range aborts the process (critical)

Case: `adversarial-1/adv-01-version-range-abort`

**Input** — a four-line project:

```abstract
versions 1..2000000000

schema Thing {
    name: text(1..40)
}
```

**Expected per SPEC §3.7** — "An implementation MUST NOT abort, panic or crash
on any input. Every failure MUST be a diagnostic with an identifier and, where
a source position is known, a position." SPEC §11.1 repeats it as conformance
requirement 5, and SPEC §9.6 defines exit codes 0, 1, 2 and 3 only.

**Actual** — the process terminates with the Windows abort status 3221226505
(`0xC0000409`) and the single stderr line `memory allocation of 48000000000
bytes failed`. No identifier, no position, an exit code outside SPEC §9.6.
Reproduces identically in the debug and release profiles, for `compile` in all
three formats and for `lint`; `templates` survives because it stops after P3.

The allocation is `versions.rs::materialise`:

```rust
let mut documents: Vec<Vec<Value>> = vec![Vec::new(); project.count() as usize];
```

24 bytes per version, so the abort threshold is around `1..1000000000`. Below
it the range is still unbounded work rather than a diagnostic: `1..1000000`
takes about 21 s and the cost is linear in the number of versions, so
`1..100000000` is a hang. `versions 1..4294967296` is rejected as E602 because
the version numbers are `u32`, which puts the accepted-but-fatal window at
roughly `1..10^6` to `1..4·10^9`.

The identifier a conforming implementation must report is a spec decision that
has not been taken; see S2. The case pins E602 because that is the identifier
the implementation already uses for a range it refuses.

## D2 — Nested parentheses in a logic condition overflow the stack (major)

Case: `adversarial-1/adv-02-logic-paren-stack-overflow`

**Input**

```abstract
logic Thing {
    require ((((… 62 deep …)))) else throw "guarded"
}
```

**Expected per SPEC §3.7** — the limit for bracket nesting in a value is 64 and
E209 is reported only when a limit is exceeded; the same table's rationale is
that the limits "exist so that a hostile or corrupted source can never exhaust
the stack". 62 is inside the limit, so the condition must be evaluated: `.flag`
is absent, every comparison in which it appears is false (SPEC §6.8), and the
`require` fails with E515 and the author's message.

**Actual (debug profile)** — exit 3221225725 (`0xC00000FD`,
`STATUS_STACK_OVERFLOW`), stderr `thread 'main' has overflowed its stack`, no
diagnostic. The threshold is 59 nested parentheses; 58 is reported correctly.

**Actual (release profile)** — E515, correct, up to 64 parentheses, with 65
correctly E209.

The defect is therefore profile-dependent, but the profile it fails in is the
default one: `cargo build`, `cargo run` and `cargo test` all produce and
execute the debug binary, and the conformance runner invokes
`CARGO_BIN_EXE_abstract`, which is the debug binary. §3.7's guarantee is stated
about the language, not about an optimisation level, and the margin it leaves
is currently negative — the documented limit is 64 and the default build
survives 58.

The same recursion is reached only through the logic condition parser. The
value scanner, the instance body-block parser and the schema group parser were
all probed to depth 63 in the debug build without overflowing.

## D3 — E209 fires at exactly the documented limit, inconsistently (minor)

Cases: `adversarial-1/adv-04-bracket-depth-at-limit`,
`adversarial-1/adv-05-group-depth-at-limit`,
`adversarial-1/adv-06-logic-block-depth-at-limit`

SPEC §3.7 requires E209 "when one is exceeded". Three of the six limits are
enforced one level early, and the implementation disagrees with itself about
which convention applies:

| Subject | Documented limit | Largest value accepted | Correct? |
|---|---|---|---|
| Segments in an assignment path | 64 | 64 | yes |
| Nested `(` in a logic condition (release) | 64 | 64 | yes |
| Bracket nesting in a value | 64 | 63 | **no** |
| Nesting depth of schema groups | 64 | 63 | **no** |
| Nesting depth of `if` / `for` blocks | 64 | 63 | **no** |
| Length of a clone chain | 64 | see D4 | **no** |

The bracket case is the one that changes which diagnostic an author sees: a
value with 64 nested `[` is at the limit, so it must be lexed and rejected for
what it is, a nested list, which is E441 (SPEC §4.6, §5.5, §10.4). With 63
brackets the implementation reports E441 and with 65 it reports E209, so only
the boundary is wrong.

Two bracket-depth checks in `lexer.rs` disagree by one: `push_bracket` rejects
the 65th bracket (correct, and the check the logic condition parser reaches),
while `scan_single_value` rejects an element already at depth 64 (one too
early, and the check a list value reaches). `schema.rs` and `logic.rs` both use
`depth >= LIMIT`.

## D4 — The clone-chain limit is enforced against resolver recursion, not chain length (major)

Cases: `adversarial-1/adv-03-clone-chain-length` (fails),
`adversarial-1/adv-03b-clone-chain-order` (passes, kept as the control)

**Expected per SPEC §3.7 and §5.7** — the limit is the "Length of a clone chain
(transitive clone depth)", and §5.7 states it plainly: "Chains deeper than 64
are E209."

**Actual** — whether E209 is reported depends on how the instances are named.
An 80-instance chain in which `a79` clones `a78` clones … clones `a00`
compiles with exit 0 and emits 80 objects. The same 80-instance chain with the
edges reversed, so that `a00` clones `a01` clones … clones `a79`, is E209.

The cause is that the check counts the resolver's recursion depth
(`resolve.rs`: `if self.active.len() >= CLONE_CHAIN`). Instances are resolved
in `(template, id)` order and resolution is memoised (SPEC §5.7 requires the
memoisation), so when the ids sort along the chain each source is already
resolved and the recursion never nests. Renaming the instances therefore
changes the diagnostic while changing nothing about the program, which is the
kind of invisible rule SPEC §1.2 P4 exists to forbid. It also means the
protection §3.7 is asking for is not in place: an 80-, 800- or 8 000-long chain
is accepted whenever the ids happen to sort favourably.

The two cases are kept as a pair so that the defect cannot be closed by
removing the check.

## D5 — A `]` closing a `(` is not a bracket-type mismatch (minor)

Case: `adversarial-1/adv-07-bracket-type-mismatch`

**Input** — `caps(id): (search]`

**Expected per SPEC §3.6** — "The lexer maintains a value-bracket stack for
`(` `)`, for `[` `]`, and for the `{` `}` of a multi-path key list. … Types
MUST match: a `)` closing a `[` is E204". `GRAMMAR.ebnf` repeats it for
`bare_text`: "Brackets inside a bare_text MUST be balanced and correctly
typed." E204's template names both brackets and the position of the opening
one. E205 is defined for "A closing bracket with nothing open", which is not
this case.

**Actual** — two diagnostics, neither E204:

```text
data/items/x.ab:3:22: error[E210]: Unexpected ']' here; expected ',' or ')'.
data/items/x.ab:3:22: error[E205]: Unexpected ']'.
```

The mirror image, `tags: [core)`, is E204 today, so only the `(`-open direction
is unhandled. The related spelling `tags: [core}` reports E203 followed by
E205; that one is defensible, because SPEC §3.6 puts `}` on the value-bracket
stack only when it closes a multi-path key list, and a `}` with no open block
brace is E205.

## D6 — A byte-order mark inside a bare value is accepted and emitted (minor)

Case: `adversarial-1/adv-08-bom-inside-value`

**Input** — `name: a<U+FEFF>b`

**Expected per SPEC §2.2** — "A UTF-8 byte-order mark (`EF BB BF`) at offset 0
MUST be removed before lexing and has no other effect. A BOM anywhere else is
an ordinary character and is E210 outside a quoted string." The BOM here is at
neither offset 0 nor inside a `quoted_string`, so it is E210.

**Actual** — exit 0, and the document carries the field `name` with the BOM
emitted literally as UTF-8, because SPEC §8.8 escapes only C0 and C1 controls.
An invisible character therefore survives into every consumer of the document.
A BOM at the start of a statement line is E210 today, so the rule is applied
outside values but not inside them.

## D7 — A bare boolean literal is accepted as a logic condition (minor)

Case: `adversarial-1/adv-10-bare-literal-condition`

**Input** — `require true else throw "never reached"`

**Expected per SPEC §6.6** — "A condition MUST evaluate to a boolean. A bare
path is a valid condition only when it names a `bool` field; anything else (a
bare `text` field, a bare `length(...)`, a bare literal) is E513. Abstract has
no truthiness." `true` is a boolean literal (SPEC §3.5), so this is a bare
literal used as a condition: E513.

**Actual** — exit 0. `require false else throw "m"` is likewise accepted and
fails with E515 rather than E513, and `require not true` is accepted. Every
other bare operand is rejected correctly: `require 1`, `require "a"`, `require
version`, `require length(.name)` and `require .name` on a `text` field are all
E513, so only the boolean literal escapes the check.

## D8 — Whitespace between a field name and its list head is ignored (minor)

Case: `adversarial-1/adv-11-space-before-list-head`

**Input** — `tags [1..2]: text @optional`

**Expected per SPEC §4.3 and §3.1** — §4.3: "`[]` immediately after the name
marks a list field". §3.1 lists "`[]` and `[<cardinality>]` in a field head"
among the constructs that are single lexemes and says "Whitespace inside any of
these is E210, except before a parameterised type's `(`, which is E304" — the
one exception is spelled out and a field head is not it. With no list head, the
token after the field name is `[` where `:` was expected, which is E210. SPEC
§1.2 P4 forbids a silently ignored token.

**Actual** — exit 0; the space is discarded, the field compiles as the list
field `tags[1..2]`, and `tags: [a, b]` is emitted as a two-element array. The
implementation does apply the adjacency rule to the inside of the same lexeme:
`tags[1 ..2]` is E210 with the message "expected the list head 'tags[1 ..2]' to
be written as one lexeme".

## D9 — A malformed image size token is reported as E206, with a self-refuting message (minor)

Case: `adversarial-1/adv-12-image-size-token`

**Input** — `icon: image(png -1x-1)`

**Expected per SPEC §4.4.7** — "A size token is one lexeme with no internal
whitespace (§3.1): `WIDTHxHEIGHT`, where each side is a decimal integer or `*`
meaning 'any'." `-1x-1` is one lexeme in the size-token position and is not a
`WIDTHxHEIGHT`, so it is E308 (SPEC §10.3), "Invalid image size '{text}';
expected WIDTHxHEIGHT where each side is a number or '*'."

**Actual**

```text
data/templates/thing.abt:3:21: error[E206]: Invalid character '-' in identifier '-1x-1'; identifiers use A-Z a-z 0-9 _ -.
```

E206's condition is "A character that cannot appear in an identifier", and `-`
is an identifier character (SPEC §3.3), so E206 cannot be the condition here;
the sentence names `-` as invalid and then lists `-` among the valid
characters. `image(png 128)`, `image(png 128 x 128)` and `image(png 1x2x3)` are
all E308 today, so only the signed spelling is misrouted.

## D10 — The integer literal `-0` on a `float` field renders as `-0.0` (minor)

Case: `adversarial-1/adv-13-negative-zero-int-literal`

**Input** — `ratio: -0` where `ratio: float`

**Expected per SPEC §3.5, §4.4.3 and §8.7** — §3.5 defines a float literal as
digits followed by `.` and digits, or an exponent, or both; `-0` has neither,
so it is an integer literal, and the integer it denotes is zero (the same
lexeme on an `int` field emits `0`). §4.4.3: "A `float` field accepts an
integer literal as well as a float literal (`price: 19` yields `19.0`)", so the
value is the integer 0 widened to `0.0`, and §8.7 renders `-0.0` only "for
negative zero".

**Actual** — `"ratio": -0.0`. A schema default written `= -0` behaves the same
way. `ratio: -0.0` and `ratio: -0e0` are float literals for negative zero and
correctly render `-0.0`.

The distinction is load-bearing rather than cosmetic. SPEC §7.5 says "Two
numbers are structurally equal when they render identically under §8.7, so
`0.0` and `-0.0` are not equal", so a field written `-0` in one version and `0`
in another produces an overlay entry that the spec says should have been
discarded.

## D11 — E422 for a matched-but-malformed image reads "is bmp, not bmp" (minor, no case)

No case: the corpus expectation format records identifiers, not message text,
so this cannot be pinned as a regression. It is recorded here instead.

E422's template (SPEC §10.4) is "Image content mismatch at {context}.{field}:
'{path}' is {actual}, not {expected}." When the file's signature matched its
declared extension but the header is truncated or malformed, the implementation
substitutes the declared format for `{actual}`, so all four of these read as a
contradiction:

```text
error[E422]: Image content mismatch at x.icon: 'a.bmp' is bmp, not bmp.
  note: malformed BMP width.
error[E422]: Image content mismatch at x.icon: 'a.bmp' is bmp, not bmp.
  note: malformed canvas size.
error[E422]: Image content mismatch at x.icon: 'a.bmp' is bmp, not bmp.
  note: file is truncated.
```

The notes are exactly the ones SPEC §4.4.7.1 prescribes, and the genuine
format-mismatch spelling is correct ("is not an image (5A 5A …), not bmp"), so
only the `{actual}` substitution for a matched signature is at fault. See S6:
the spec does not say what `{actual}` is in this case.

---

## Spec issues

These are gaps or contradictions in `docs/SPEC.md` rather than implementation
defects. They are listed because a conforming implementation cannot be written
against them as they stand.

**S1 — §8.1 understates when `data` can be empty.** §8.1 says "An empty `data`
array is only reachable in single-file mode when the named files declare no
instances; a project with no sources at all is E103." A whole-project compile
of a project that has `.abt` files and no instances also emits `"data": []`,
and §7.2 blesses that project explicitly: "A project with zero instances is
still fully schema-checked". The sentence in §8.1 needs the second case.

**S2 — §4.12 puts no upper bound on a version number, and §7.4 requires every
version to be compiled.** `versions 1..2000000000` is therefore a legal program
under §4.12 that no implementation can materialise, while §3.7's limit table
has no row for it and §3.7 forbids aborting. One of the three has to give: a
new §3.7 row ("Number of versions in the project range", reported as E209), an
explicit cap in §4.12 reported as E602, or an explicit statement that the range
is bounded by the implementation. This is what D1 falls out of. The
implementation's own de-facto cap is `u32`, which it reports as E602 with the
message "Invalid version range '1..4294967296'; both are integers >= 1 and min
<= max." — a sentence that denies its own condition.

**S3 — §3.1 and §3.5 disagree about non-ASCII whitespace in bare text.** §3.1:
"Horizontal whitespace (space `U+0020`, tab `U+0009`) separates tokens and is
otherwise insignificant. … No other whitespace character may appear outside a
quoted string (E210)." §3.5: "Bare text keeps its exact characters (after
trimming leading and trailing whitespace); no escape processing is applied to
it". A bare value containing `U+00A0` satisfies the second and violates the
first. The implementation currently follows §3.5 for `U+00A0`, `U+0085` and
`U+2028` and §3.1 for `U+000B` and `U+000C`, and applies §3.1 to `U+00A0`
outside a value but not inside one. Case
`adversarial-1/adv-09-nonascii-whitespace-in-value` carries the three inputs
and is marked `pending` until the ruling is made; the reading matters because
`U+00A0` in a caption is plausible authored text, and `U+2028` reaches the
output verbatim under §8.8, where it is a line break for YAML 1.1 readers.

**S4 — §6.6's "an unbalanced parenthesis is E511" is unreachable.** §3.6 makes
an unmatched `)` E205 and an unclosed `(` E203 at the lexical level, and
§11.1's precedence rules (same phase, same position, lower identifier) select
those. Observed: `require .flag == true) else throw "m"` is E205 and `require
(.flag == true else throw "m"` is E203. Either §6.6 should defer to §3.6 or
§11.1 should pin the pair.

**S5 — §3.6 rule (b) does not say whether a blank or comment line may sit
between `}` and `else`.** The rule suppresses the `NL` "when the next token
after the line terminator is `else`", and a blank line produces its own `NL`,
so the strict reading makes

```abstract
    }

    else {
```

E517. The implementation accepts it. §6.3's wording ("An `else` that does not
immediately follow a closing `}` of an `if` is E517") does not settle it
either.

**S6 — §10.4 leaves `{actual}` undefined for E422 when the signature matched.**
§4.4.7.1 prescribes the three notes (`file is truncated.`, `malformed BMP
width.`, `malformed canvas size.`) but not what the message says the file *is*.
See D11.

**S7 — nothing bounds the work a legal program can demand.** §3.7's limits are
about stack depth. 63 nested `for` blocks over two-element literal lists is
inside every one of them and requires 2^63 body executions; the run produced no
output in 60 s. §11.1 requirement 5 forbids crashing but not diverging, so this
is conforming as written. If the intent is that a hostile source cannot make
the compiler hang, §3.7 needs either an iteration budget or a rule that a `for`
body's cost is bounded.

**S8 — §9.8's `{value}` does not fit E413 on a `text` range.** §9.8 says
"`{value}` renders a scalar exactly as §8.7 and §8.8 would render it in JSON",
but for a `text` length violation the implementation substitutes the scalar
count (`Range mismatch at x.name: 3 is not in 2.`), which is the readable
choice and not what §9.8 describes. The same message renders `{ranges}` for
`text(2..2)` as `2` rather than `2..2`, which §9.8's "renders its items in
declaration order" does not authorise either.

---

## Behaviour confirmed correct

Recorded so that a later change to any of it is visible as a regression rather
than as new work.

- **Limits.** 64 path segments accepted and 65 E209; 65 nested brackets E209;
  bracket depth reported once rather than per bracket; deep and unbalanced
  bracket runs of 100 000 handled in 1.4 s.
- **Encoding.** Invalid UTF-8 in any position is E102 with the byte offset,
  including truncated sequences, overlong encodings, encoded surrogates,
  Latin-1 and UTF-16; a BOM at offset 0 is stripped; CRLF, LF, a missing final
  newline and a BOM+CRLF combination all compile; a lone CR is E210.
- **Strings.** Unterminated strings, unknown escapes including `\u`, a string
  crossing a line terminator, and trailing text after a closing quote are all
  the prescribed errors; `"// not a comment"` and `"a ] b"` are one value;
  NUL inside a string is emitted as the escape `\u0000`.
- **Numbers.** i64 boundaries exact on both sides, `1e400` E211, `1e-400`
  accepted as `0.0`, 5 000-digit literals E211 rather than a hang, `.5` and
  `5.` not float literals, `1..3` never lexed as a float.
- **Floats.** All 32 probed values render per §8.7, including `1e-6` →
  `0.000001`, `9e-7` → `9.0e-7`, `1e20` → `100000000000000000000.0`, `1e21` →
  `1.0e+21`, `5e-324` → `5.0e-324` and shortest-round-trip forms.
- **Text length** counts Unicode scalar values: a skin-toned emoji is 2, a
  zero-width-joiner sequence is 3.
- **Image probing.** All 44 crafted headers produce a diagnostic and no hang,
  including zero-length and one-length JPEG segments, a 60 000-byte `FF` fill
  run, 30 000 segments, and truncation at every format's byte requirement; the
  65 536-byte budget is respected.
- **Asset paths.** `..`, absolute, UNC, drive-relative and extended-length
  paths are E424; alternate data streams, trailing dots and trailing spaces are
  refused at the extension check before the filesystem is touched; device names
  do not hang; diagnostics print project-relative paths with `/`.
- **CLI.** Every command, flag and positional combination of §9 behaved as
  specified, including `--out` refusals (E808), a missing parent directory
  (E810, exit 3), repeated flags (E811), `--max-errors` bounds and its trailing
  note, format-keyword resolution, and a quiet exit 0 on a broken pipe.
- **`init`** produces a scaffold that compiles, lints and lists templates, and
  refuses a non-empty target with E813.
- **Fuzzing.** 5 400 mutation runs over a valid multi-version project produced
  no panic, abort, non-{0,1,2,3} exit code or 25 s timeout.

---

# Second adversarial pass (same stage, later run)

The compiler was not modified between the two passes: `project/src` carries the
same bytes it did when D1–D11 were written, and this pass changed nothing under
`project/`. It re-ran the earlier repros and then attacked the areas the first
pass had covered only in outline — comment stripping, header-tag continuation,
multi-path key lists, body blocks, tuple arrays, `#tag` arguments, brace
patterns, interpolation, and the diagnostic-precedence rule of §11.1.

**D1–D11 all reproduce**, in the same shape and at the same boundaries:
`versions 1..2000000000` still aborts with an allocation failure and an exit
code outside §9.6 (D1); 59 nested parentheses in a logic condition still
overflow the stack in the debug profile while 64 are fine in release (D2);
E209 still fires at the documented limit rather than past it for bracket
nesting, schema groups and logic blocks, and correctly past it for path
segments (D3); an 80-instance clone chain still compiles or fails depending
only on how its instances are named (D4); `tags []` is still accepted (D8);
`-0` on a `float` still renders `-0.0` (D10).

| Probe | Cases run | Panics / crashes / hangs |
|---|---|---|
| Comment stripping (§3.2): every position of `//` relative to line start, whitespace, quotes, brackets, blocks, headers and logic | 27 | 0 |
| Statement continuation and header tag lists (§3.6, §5.2): leading, trailing and doubled commas, blank and comment lines inside a continuation, windows, tag values | 33 | 0 |
| Paths, multi-paths and body blocks (§5.4): empty segments, index syntax, dotted keys, unbalanced and misplaced braces, one-line blocks, nesting | 62 | 0 |
| Values (§5.5, §5.6, §5.8): tuple arrays, `#tag` arguments, wildcards, brace patterns, quoted/bare boundaries | 48 | 0 |
| Interpolation (§5.11): `$name`, `${name}`, `$$`, unknown, unterminated, chained, in headers, tags, paths and quoted strings | 37 | 0 |
| Numeric literals (§3.5, §8.7): i64 and f64 boundaries, overflow, underflow, 4 000-digit runs, every rendering branch | 35 | 0 |
| Limits (§3.7): assignment, clone and logic path depth; block nesting; schema groups; brackets; nested `#tag`; clone chains in both id orders; document depth | 89 | 1 (D2) |
| Encoding and control characters (§2.2, §3.1): BOM at every position, doubled BOM, lone CR, CRLF, invalid UTF-8 of five kinds, C0/C1/DEL in values and between statements, column counting | 43 | 0 |
| Schema and logic lexemes (§4, §6): adjacency, modifiers, ranges, cardinalities, `versions`, every logic statement and operator form | 86 | 0 |
| Output (§8.4–§8.9): 39 hostile strings through JSON, YAML and RAW, parsed back with a JSON and a YAML reader and compared | 39 × 3 | 0 |
| CLI and project discovery (§2.3, §9): every command, every flag, both `--out` spellings, both project layouts, `init`, broken pipe, root spellings | 85 | 0 |
| Filesystem: junction cycle, junction out of the project, aliasing junction, nested `data`, a directory named `*.ab`, 40-level paths, unicode file names, empty files | 15 | 0 |
| Mutation fuzzing of a valid multi-version project (three formats, two seeds) | 2 700 | 0 |

Determinism was re-checked and held: three repeated runs, six root spellings
(`.`, `./`, `.\`, `data`, `data/`, absolute), a run from inside `data` with
`..`, both orders of two file arguments, two creation orders of 400 source
files, and `--skip-assets` all produce byte-identical documents. JSON and YAML
agreed on every one of the 39 hostile strings, including `yes`, `no`, `null`,
`~`, `---`, leading and trailing spaces, C1 controls, emoji and bidi overrides;
every mapping key is quoted, as §8.5 requires. A source reached through a
junction that leaves the project is reported under its walk path
(`data/ext/bad.ab`), never an absolute or canonical one.

Eight new defects and one message defect follow, with four new specification
issues. Cases are `conformance/adversarial-1/adv-14` … `adv-24`; the corpus
runner now reports 273 cases, 228 passed, 6 pending, 12 without an expectation
and 27 failed — the 19 the two earlier stages pinned plus this pass's 8. No
case outside `adversarial-1/` changed, and `cargo build` is warning-free in
both profiles with 508 unit, binary, CLI and compiler tests passing.

---

## D12 — An unterminated `${` in an unquoted value is E203, not E427 (major)

Case: `adversarial-1/adv-14-unquoted-dollar-brace`

**Input**

```abstract
Thing :: @id.atlas
    name: Atlas
    label: ${id
```

**Expected per SPEC §5.11 and §10.4** — §5.11: "An unterminated `${` is E427."
§10.4 gives E427 the condition "Unterminated `${`", the message
`Unterminated '${'.` and the example `label: ${id` — this file, character for
character. §3.1 settles why the lexer must not intervene: `${` is listed among
the constructs that "are single lexemes", so its `{` is part of that lexeme and
is neither the `{` of a multi-path key list nor a block brace, which are the
only two kinds §3.6 tracks. Nothing is left open at end of file and the value
reaches interpolation.

**Actual** — `data/items/x.ab:3:13: error[E203]: Unclosed '{' opened at 3:13.`

The `{` of the `${` is pushed onto the bracket stack, so E427 is unreachable
from an unquoted value. It stays reachable from a quoted one — `label: "${id"`
is E427, and so is `${` inside a quoted element of a bracketed list — so the
identifier is not dead, but the spelling the catalogue publishes as its example
produces a different diagnostic. The same push also makes `label: ${id` join
nothing and `label: ${id}}` report E205 rather than emitting the text `x}`.

## D13 — A bare `#tag` flag argument on a non-`bool` field is silently coerced to the text `true` (major)

Case: `adversarial-1/adv-18-tag-flag-non-bool`

**Input** — `caps: [#search(label)]`, where `label` is declared `text`.

**Expected per SPEC §5.5** — "A bare `flag` argument sets the field `flag` to
`true` and is valid only when `flag` is a `bool` field (E412)." §1.2 P4 forbids
a silent coercion and §1.3 guarantee 4 says every emitted value has the type its
schema declares.

**Actual** — exit 0, and the element emits `"label": "true"`.

The flag is never checked against the field's declared type. It is turned into
the bare value `true` and then interpreted by whatever type the field has, so
the outcome depends on the type rather than on the rule:

| Declared type of the flag's field | Result |
|---|---|
| `text` | accepted, emits the string `"true"` |
| `text` **and** the group's `@tag` field | accepted, the element is keyed `"true"` |
| `bool` | accepted, emits `true` (the only correct case) |
| `enum` | E414, `'true' is not one of a, b` — not the E412 §5.5 prescribes |
| `int`, `float` | E412, but with the message `expected int, found text` |

The `text` rows are the serious ones: a typo'd argument reaches the output as
data, which is the class of failure §1.3 exists to rule out.

## D14 — A leading `,` in an instance header tag list is silently discarded (minor)

Case: `adversarial-1/adv-15-header-leading-comma`

**Input** — `Thing :: , @id.atlas`

**Expected per GRAMMAR `header_tag_list` and SPEC §1.2 P4** — the rule is
`header_tag , { "," , header_tag }`: the list begins with a tag, never with a
separator. §5.2 gives the `,` one job only ("The header tag list is the one
construct that continues across physical lines on a trailing `,`"). The token
after `::` is a `,` where a header tag, `@since(`, `@removed(` or the end of the
line was expected, which is E210 — the identifier the implementation already
produces for `Thing ::: @id.x` and for `Thing :: @id.x @since (1)`.

**Actual** — exit 0; the comma is discarded and the instance compiles as if it
were not there. Exactly one leading comma is swallowed: `Thing :: ,, @id.x` is
E442 and a header that is only `Thing :: ,` is E442, so the second one is seen.

## D15 — A trailing `,` in a multi-path key list is silently discarded (minor)

Case: `adversarial-1/adv-16-multipath-trailing-comma`

**Input** — `owner.{team, contact,}: Knowledge Systems`

**Expected per GRAMMAR `multi_path_assignment` and SPEC §5.4** — the rule is
`path , "." , "{" , identifier , { "," , identifier } , "}"`, with no trailing
separator, and the one place the grammar does allow one is written out
(`tuple_array_assignment` ends `{ "," , tuple_row } , [ "," ] , "]"`). §5.4:
"Keys are single identifiers"; the token after the last `,` is `}`, which is not
an identifier, so it is E210. §5.5's "a trailing comma is allowed inside
brackets" is about list values, not about a key list.

**Actual** — exit 0; the value is assigned to both keys. The other two
positions are checked: `owner.{, team}` and `owner.{team,, contact}` are both
E210 with `expected a key name`, so only the trailing position escapes.

## D16 — Whitespace before a tuple-array column list is accepted (minor)

Case: `adversarial-1/adv-17-tuple-columns-space`

**Input** — `copy (key, value): (en_us, Welcome)`

**Expected per SPEC §3.1** — the adjacency paragraph lists "a tuple-array
column list" among the constructs that "are single lexemes and MUST contain no
internal whitespace", and states the diagnostic in the same sentence:
"Whitespace inside any of these is E210, except before a parameterised type's
`(`, which is E304." A tuple-array column list is not that exception.
`GRAMMAR.ebnf` L9 repeats the list.

**Actual** — exit 0; the space (or a tab, or several spaces) is discarded and
the tuple array compiles. Three constructs named in the same sentence are
enforced — `caps: [#a (txt: b)]` is E210, `@since (1)` is E210 and `$ (Thing)`
is E210 — so the rule exists in the implementation and this one construct is
outside it. This is D8's failure in a second place: there the field head
`tags []`, here the column list.

## D17 — A trailing comma after an unbracketed tuple row is E210 naming the line terminator, not E442 (minor)

Case: `adversarial-1/adv-19-tuple-trailing-comma`

**Input** — `copy(key, value): (en_us, Welcome),`

**Expected per SPEC §3.6 and §5.5** — §3.6: "In particular a trailing `,` at
value-bracket depth 0 does **not** continue a statement; it is E442." §5.5 makes
tuple rows depth-0 comma separated and allows a trailing comma only in the
bracketed spelling, so this is a trailing comma at depth 0, exactly like
`tags: a,` and `caps: [#a],` — both of which the implementation reports as E442.

**Actual**

```text
data/items/x.ab:3:40: error[E210]: Unexpected control character U+000A here; expected '('.
```

The message is wrong in a second way, independently of the identifier: §2.2
makes `LF` a line terminator, not a character of the program, so naming it
"control character U+000A" describes something the source does not contain.
The bracketed form `copy(key, value): [(en_us, Welcome),]` is accepted, as it
should be.

## D18 — Trailing text after a closed quoted value is reported at the wrong position when it contains a quote (minor)

Case: `adversarial-1/adv-20-text-after-quoted-value`

**Input** — `name: "Atlas" b"`

**Expected per SPEC §5.5, §7.1 and §11.1** — §5.5: "a leading `"` always starts
a quoted string, whose closing quote MUST end the value — trailing text after it
is E210." The value closes at column 17 and the trailing text starts at column
19. §7.1 makes lexing and parsing one phase (P1), so §11.1's tie-break (3)
applies — within a phase the earliest source position wins — and column 19
precedes the column 20 at which the second quote opens. E210 is therefore the
diagnostic that must be reported first.

**Actual** — `data/items/x.ab:2:20: error[E201]: Unterminated string literal;
strings must open and close on the same line.`, and no E210 at all. The same
line without the stray quote (`name: "Atlas" b`) is correctly E210 at column 19,
and `name: "Atlas" "b"` is correctly E210 at column 19; lengthening the trailing
text moves the E201 further right (`name: "Atlas" bb cc"` is E201 at column 24),
which shows the diagnostic is following the stray quote rather than the first
offending token.

## D19 — A body block written on one line reports E203 and E205, neither of whose conditions occurred (minor)

Case: `adversarial-1/adv-22-block-brace-one-line`

**Input** — `owner { team: Knowledge }`

**Expected per SPEC §3.6 and §5.4** — "A block's `{` MUST be the last token of
its logical line and its `}` MUST be the first token of a logical line",
repeated in §5.4 for body blocks. The first token after the `{` that is not on a
new line is `team`, so the violation is E210 there.

**Actual** — two diagnostics:

```text
data/items/x.ab:3:11: error[E203]: Unclosed '{' opened at 3:11.
data/items/x.ab:3:29: error[E205]: Unexpected '}'.
```

E203's condition (§10.2) is "End of file with brackets still open" and E205's is
"A closing bracket with nothing open". Neither happened: the `}` on this line
does have an open block brace — the one at column 11 that E203 simultaneously
reports as never closed. Every neighbouring spelling of the same violation is
E210 with `expected end of line`:

| Input | Reported |
|---|---|
| `owner { team: T }` | **E203 + E205** |
| `owner {team: T}` | **E203 + E205** |
| `owner { team: T contact: C }` | **E203 + E205** |
| `owner { team: T` … newline … `}` | E210 at `team` |
| `owner { x` | E210 at `x` |
| `owner { }` | E210 at `}` |
| `owner{team, contact}: T` | E210 at `team` |
| `}` followed by text | E210 at the text |

The pair appears only when a complete `key: value` statement sits between the
braces on one line.

## D20 — E412 for a bare `@name` header flag on a non-`bool` field reads "expected text, found text" (minor, no case)

No case: the corpus expectation format records identifiers, not message text,
and the identifier here is the one §5.2 prescribes.

§5.2: "A bare `@name` flag MUST target a `bool` field; on any other type it is
E412." On a `text` field the implementation does report E412, but renders it as

```text
data/items/x.ab:1:17: error[E412]: Type mismatch at x.name: expected text, found text.
```

`{found}` is §9.8's `{kind}` substitution, and printing the field's own declared
type on both sides gives the author a sentence that denies itself. On an `int`
field the same input reads `expected int, found text`, which is at least
consistent, so only the case where the field is `text` degenerates. This is
D11's shape ("is bmp, not bmp") in a second diagnostic.

The same substitution is loose elsewhere: `n: 19.5` on an `int` field is E412
with `expected int, found text`, although the value is a float literal and §9.8's
`{kind}` list contains `float`. §10.4's own E412 example (`count: "5"`) is the
case where `text` is the right answer, so the substitution is right for a quoted
value and approximate for every other shape the validator actually recognises.

---

## Specification issues (second pass)

**S9 — §3.6 classifies every `{` as one of two kinds, and three constructs are
neither.** §3.6: "A `{` immediately preceded by `.` opens a multi-path key list;
every other `{` is a **block brace**", and the enumeration that follows says a
block brace is "A `{` that opens a schema body (§4.2), a group body (§4.7), a
logic block (§6.3) or an instance body block (§5.4)". Three constructs match
neither description: the `{` of a `${` (§3.1 makes `${` one lexeme), the `{` of
a brace file pattern (§5.8), and a literal `{` inside bare text (§3.5: "Bare
text keeps its exact characters"). The gap is not academic — it decides D12,
and it decides which identifier an unclosed brace pattern gets: `imgs:
a/{p,q.png` is E203 today, where §5.8's own E435 ("Malformed brace pattern")
reads like the intended answer. §3.6 needs a third category ("a `{` inside a
value that is not preceded by `.` is an ordinary character of the value") or an
explicit statement that brace patterns and `${` are lexed inside the value.

**S10 — §3.7 does not say how "nesting depth of the emitted document" is
counted, and two rows of its own table disagree by four levels.** Case
`adversarial-1/adv-21-document-depth-limit` (pending). The same table gives 64
for "Nesting depth of schema groups (static), and of `$(Schema)` values in one
compiled object (dynamic)" and 64 for "Nesting depth of the emitted document".
An instance that nests 61 `$(Schema)` values satisfies the first row and is
rejected under the second, because the implementation counts the JSON envelope
(document object, `data` array, instance object) and the scalar leaf as well as
the authored objects. The practical ceiling is therefore 60, and no project can
ever reach the 64 the group row promises. Three readings are available —
authored objects only (61, compiles), authored objects plus envelope (64,
compiles), the implementation's (65, fails) — and the spec picks none. §7.2
depends on the answer too: it blesses recursion through an `@optional` field
with "instances of it are bounded by the document depth limit of §3.7".
Separately, this is the only E209 the implementation reports with no file, line
or column, which §3.7 ("where a source position is known, a position") and §9.8
(the bare `abstract:` form is for a diagnostic that "concerns no file") both
argue against.

**S11 — §3.3 states three rules for an identifier and names a diagnostic for
two of them.** Case `adversarial-1/adv-23-identifier-leading-hyphen` (pending).
The character set is E206 and "MUST NOT end with `-`" is E207, but "MUST begin
with `A-Z a-z 0-9 _`" has no identifier. The implementation reaches for E206,
whose condition is "A character that cannot appear in an identifier" and whose
message lists `-` among the characters that can, so `-name: 1` produces
`Invalid character '-' in identifier '-name'; identifiers use A-Z a-z 0-9 _ -.`
D9 is the same contradiction reached through the image size token; this is the
rule underneath it. The fix is a fourth condition on E206, a new identifier, or
E207 widened to both ends.

**S12 — §3.2's "an unquoted value MUST NOT begin with `//`" is stated as a rule
but derived as a consequence.** Case
`adversarial-1/adv-24-bare-value-begins-with-slashes` (pending). §3.2 justifies
the MUST NOT by the comment rule ("Because a `//` at the start of a line or after
a space starts a comment"), and `GRAMMAR.ebnf` turns it into a hard constraint
on `bare_text` ("does not begin with `//` and contains no ` //`"). But the
comment rule does not fire when the `//` follows a `:`, so `label://x` is a value
whose text begins with `//` and which no comment rule touches. The implementation
accepts it and emits `"//x"`; the grammar says E210. The two readings differ on
text an author will really write, because §3.2's own remedy for a
protocol-relative URL is to quote it.

**S13 — §9.3 puts no upper bound on `--max-errors`.** "`--max-errors` requires a
decimal integer greater than or equal to 0; anything else is E812."
`--max-errors 99999999999999999999` is a decimal integer greater than or equal
to 0, and the implementation rejects it with `Flag '--max-errors' requires a
decimal integer greater than or equal to 0; found '99999999999999999999'.` —
a sentence that denies its own condition, in the same way S2 records for E602.
Either §9.3 states the bound (the implementation's is `u32`) or the flag
saturates.

**S14 — the character set of bare text is undefined in the other direction as
well.** S3 already records that §3.1 ("No other whitespace character may appear
outside a quoted string") and §3.5 ("Bare text keeps its exact characters")
disagree about non-ASCII whitespace. The same gap decides non-whitespace
control characters, which neither rule mentions and which `GRAMMAR.ebnf`'s
`bare_text` does not exclude. The implementation rejects every C0 control and
`U+007F` inside a bare value with `E210: Unexpected control character U+0000
here; expected a value character`, and accepts every C1 control (`U+0085`, `U+0090` and `U+009F` were probed), which it then escapes into the output per §8.8. So a NUL written bare is E210, while the same NUL inside a quoted string compiles and is emitted as the escape `\u0000` -- with no rule in the specification drawing that line. A ruling that names the permitted set once, for quoted and bare text alike, would settle S3 and this together.

---

## Behaviour confirmed correct (second pass)

Additional to the first pass's list; recorded so that a change to any of it
shows up as a regression.

- **Comments (§3.2).** `//` starts a comment only at line start or after a
  space or tab; `a//b` and `https://x` keep their slashes; a comment inside an
  open bracket, on a block-brace line, after a header's trailing comma and
  inside a `#tag` argument list all behave; brackets and quotes inside a
  comment are invisible to the lexer; a value emptied by comment stripping is
  E210 and never an empty assignment.
- **Header tag semantics (§5.2).** The value ends at the next depth-0 `,` or
  `@`; `[`, `]`, `{`, `}` and `#` inside it are ordinary text; a quoted value
  may contain `,` and `@`; `@icon../a` yields `./a`; `@since.2` is a tag and
  `@since(2)` a window; E409, E429, E437 and E438 all fire where §5.2 says.
- **Interpolation (§5.11).** Single pass with no rescanning; `$$` and `$$$id`;
  the maximal identifier run including `-`; E425 with a suggestion, E426 for a
  `$` before a space or at end of value; nested and list fields are not
  variables; interpolation reaches header tag values and quoted strings and
  does not reach paths (E210).
- **Tuple arrays and `#tag` (§5.5).** E416 for both arities and for `caps(): ()`
  with the prescribed note, E417, E409 inside a group, E418, E419, E444, E210
  after a closing `)` and between rows, wildcard expansion in a cell and in a
  `#tag` name, and a cell that may not hold a list or a tag object.
- **Brace patterns (§5.8).** Cartesian product with the leftmost group varying
  slowest, E434 on a non-list field, E435 for an empty and for a nested
  alternative, and no expansion on a `text` field.
- **Schema lexemes (§4).** E303 for an unknown, repeated or post-default
  modifier; E304 for whitespace before a parameterised `(`; E308 for a spaced
  size token; E315, E323 for `[3]`, `[..4]` and `[4..2]`; E601 and E602 for
  every malformed `versions` line, including `1.5..3` and `-1..3`.
- **Logic lexemes (§6).** E507, E508, E511 (`===`), E512, E513, E514, E516,
  E517, E519; `!` as a spelling of `not`; `require` / `else throw` split across
  two lines; a condition continued inside parentheses but not outside them; two
  statements on one line E210; `length (…)` with a space is accepted, which
  §3.1's adjacency list does not cover.
- **Diagnostic columns** count Unicode scalar values: astral, CJK and combining
  text before the offending token all report the same column as ASCII, under LF
  and under CRLF.
- **Filesystem.** A junction cycle inside the discovery root terminates; a file
  reachable through two junctions is collected once; a directory named `x.ab`
  is not a source and is E806 when named; `data/data` resolves per §2.3 step 2;
  40-level paths, unicode file names, zero-byte `.ab` and `.abt` files and a
  file containing only a BOM all compile.
- **`--out` refusals** distinguish the two layouts correctly: `--out out.ab` is
  allowed at the project root when the discovery root is `data/`, and is E808
  in a project with no `data/` directory, where the discovery root is the
  project root.
