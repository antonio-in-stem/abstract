# SPEC issues raised while deriving expectations — areas `schema-types` and `cli-project`

Every question below is one a conformance case could not answer from SPEC.md alone. Each item names
the case, quotes the relevant spec text, gives the reading that was recorded in `case.toml`, and states
exactly what the spec would have to say to close it.

Cases whose expectation was recorded despite a question are marked **decided (needs confirmation)**;
the one case that could not be decided at all is marked **undecided** and carries
`status = 'ambiguous'` in its `case.toml`.

---

## 1. Which identifier does an empty `$()` produce? — decided (needs confirmation)

**Case:** `schema-types/schema-case/empty-ref/` (`child: $() @optional`)

§4.4 enumerates the parameterised types whose empty argument list is E317: *"`enum()`, `file()`,
`image()` and `ref()` are E317; `text()`, `int()` and `float()` are valid and unconstrained"*. `$()`
appears in neither list. `GRAMMAR.ebnf` has `nested_schema_type = "$(" , schema_name , ")"`, so the
empty form is a grammar violation, and §3.4 says *"A malformed schema name is E208"*.

**Recorded reading:** E208, at the type, in the template file (§4.4.9 requires `$(…)` targets to be
resolved at schema-validation time, not lazily at first use). The competing readings are E210 (a token
that is not valid here — the parser expects a `schema_name` and finds `)`) and E317 (a parameterised
type with no arguments), and the message templates decide against both: E317's template reads
`{type}(...) requires at least one argument.`, which renders badly for `$`, and E208's template
`Invalid schema name '{text}'` renders naturally for the empty text.

**Question:** does §4.4's E317 list intentionally exclude `$()`, and is the empty spelling E208? If so,
add `$()` to the E208 sentence in §3.4 or to the §4.4 paragraph.

---

## 2. Which identifier does `bundle --key … --plain` report? — undecided

**Case:** `cli-project/cli-08/` (`status = 'ambiguous'`)

Appendix D.1 item 1: *"`--key` together with `--plain` is a usage error. A supplied key MUST NOT be
silently discarded, and no command may write an unencrypted container while a key was given."*
§9.9 places `bundle` outside the language and outside conformance, and §10.8 defines no identifier for
two mutually exclusive flags: E802 is an unknown flag, E803 a flag missing its value, E811 a repeated
flag, E812 an invalid flag value. None describes this condition.

The behaviour is fixed by Appendix D (refuse, write nothing, exit 2 under §9.6); only the identifier is
open.

**Question:** should §10.8 gain an identifier for "mutually exclusive flags" (a reserved id such as
E809 is available per revision-4 note R4-8), or is `bundle` deliberately outside the identifier
catalogue, in which case the case should assert only the exit code and the refusal?

---

## 3. A non-identifier token inside a type argument list: E210 or E306? — decided (needs confirmation)

**Cases:** `schema-types/enum-modifier-strip/` (`c: enum(a, @optional , b)`),
`schema-types/star-member/` (`tags[]: enum("a*", ab)`)

`GRAMMAR.ebnf` gives `enum_type = "enum" , "(" , identifier , { "," , identifier } , ")"`, and §4.4.5
says each member *"is an `identifier` and is normalised"*. §4.4 makes an **empty** entry E306, and
§10.3 gives E306 exactly two message forms — `enum(...) must declare at least one member.` and
`Duplicate enum member '{name}'.` — neither of which covers a member that is present but is not an
identifier.

**Recorded reading:** E210 (`Unexpected {found} here; expected {expected}.`) for both a modifier token
and a quoted string in a member position. The same reasoning would apply to `file(…)` and `image(…)`
argument lists.

**Question:** is E210 the intended identifier for a malformed (as opposed to empty or duplicate)
argument-list entry? A sentence in §4.4 saying so would remove the guess.

---

## 4. When is the `--out` destination refused, relative to parsing? — decided (needs confirmation)

**Case:** `cli-project/cli-12/` (`--out data/Item.abt`, and `data/Item.abt` in that tree is not a
parseable template: it still holds the JSON that the 0.2.0 run wrote over it)

§9.4 lists three refusal conditions for E808. Two of them — *"when it lies inside the data directory"*
and *"when its extension is `.ab` or `.abt` and it lies inside the discovery root"* — need only the
project resolution of §2.3. The first — *"when it is a discovered source file"* — needs P0. §11.1's
precedence rule orders diagnostics by *"the earliest phase of §7.1"*, and E8xx diagnostics are not
phases of §7.1, so the rule does not say whether E808 precedes a P1 parse error in the same run.

**Recorded reading:** E808 is decided from §2.3 project resolution alone, before P1, so the
unparseable `data/Item.abt` is never reported and the run produces exactly one diagnostic.

**Question:** should §9.4 or §11.1 state where the `--out` destination check sits in the phase order?
Anything that can be decided before P0 ought to say so, otherwise a case like this one has two defensible
expected outputs.

---

## 5. How many diagnostics does one P1 error budget produce? — affects several cases

**Cases:** `schema-types/dup-nonascii/` (four non-ASCII identifiers across two files),
`schema-types/junk-abt/` (three invalid top-level lines in one file),
`schema-types/yaml-key/` (three invalid field names in one file)

§7.1 says *"an implementation MAY report several diagnostics from the same phase before stopping"*, and
§9.3 fixes `--max-errors` at 20 by default, but nothing specifies **parse recovery**: after E206 at
`data/items/a.ab:2`, whether the lexer resynchronises and also reports line 3 is left open. §11.2's
golden format is byte-exact stderr, so two conforming implementations could not both pass the same
golden case.

**Recorded reading:** `expected-error.txt` lists one id per source file (the first error in each file),
because files are parsed independently, and the `expected_1_0` note in each `case.toml` records how many
further occurrences the input contains. `bad-default` is the one multi-id file in these two areas where
the count is certain, because P3 checks defaults *"within a schema in field declaration order"* (§7.2),
which fixes both the number and the order.

**Question:** should §7.1 or §11.2 fix a recovery rule (for example: at most one diagnostic per logical
line, and parsing resumes at the next `NL` at bracket depth 0)? Without one, every golden case whose
input contains more than one lexical defect is implementation-defined.

---

## 6. Do directory junctions that leave the project root stay in the source set? — decided

**Case:** `cli-project/cli-16/case-junc-escape/` (the junction was recreated with `mklink /J`; it points
at a sibling `outside/` directory holding `x.ab`)

§2.4 says *"Symbolic links and directory junctions MUST be resolved. The walker MUST keep a set of
canonical directory paths already visited and MUST skip a directory whose canonical path is already in
the set."* It says nothing about a link whose target lies outside the project root.

**Recorded reading:** the link is followed and `outside/x.ab` is compiled, so the case's expected
document contains the instance `outsider`. This is a deliberate departure from the 0.2.0-era
`expected_note` on that case, which asked for such links to be skipped, and it is what the spec text as
written requires: only cycles are cut.

**Question:** none — but this is worth confirming, because it means a compiled document can contain
instances from outside the project root, and §7.6's determinism guarantee then depends on a link the
source tree cannot itself carry.

---

## 7. Coverage gap, not a spec question: `image-cases` never exercises E422

**Case:** `schema-types/image-cases/`

`assets/tex/trunc.png` (20 bytes: a correct PNG signature, short of the 24 bytes §4.4.7.1 requires to
read IHDR) and `assets/tex/fake.png` (GIF89a bytes under a `.png` name) are present in the case tree,
but `data/items/ok.ab` references only `tex/a.png` and `tex/wide.png`, both of which are valid. The case
therefore compiles cleanly and asserts nothing about E422 — neither the `note: file is truncated.` form
nor the content-mismatch form, which is precisely the defect the finding is about.

Closing it needs one more instance file referencing each of the two bad assets (or two variant project
trees, following the convention the other cases in this area use). Inputs were left unchanged here,
because authoring new cases was outside this stage.
