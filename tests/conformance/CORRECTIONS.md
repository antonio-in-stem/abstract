# Corpus corrections

Expectation files changed by the conformance stage, and the SPEC reading that
proves the recorded expectation wrong. Nothing else in the corpus was edited:
every other case passes against the expectation it was given.

Each entry names the file, what it said, what it says now, and why.

---

## 1. `logic-engine/logic-22/expected.json` — the instance id

**Was:** `"id": "one"`  **Now:** `"id": "long"`

`data/items/long.ab` reads:

```abstract
T :: @id.long
name: a-very-long-product-name
```

SPEC 5.3 fixes the id as the value of the `@id` tag, and the file stem when
there is none. Both agree here: the tag says `long` and the file is `long.ab`.
There is no reading under which the id is `one`; `one` appears in the case only
inside the schema's own `require .id == "one"` message, which the case's
`note_1_0` explains is never reached because `length(.name)` is 24, not 1.

Every other byte of the expectation was already correct and is unchanged.

---

## 2. `docs-tests-examples/dte-12/expected-error.txt` — `note #comment: hello`

**Was:** `E210`  **Now:** `E316`

Two cases in the corpus compile the same construct — a valid identifier
followed by a token that can never continue a path — and pinned opposite
identifiers:

| Case | Input | Pinned |
|---|---|---|
| `output-canon/oc-10` | `note #x: hello` | `E316` |
| `docs-tests-examples/dte-12` | `note #comment: hello` | `E210` |

They cannot both hold. `SPEC-ISSUES-logic-engine.md` N2 raises the question and
adopts E316; the `docs-tests-examples` derivation adopted E210 without knowing
of the other case.

**Reading applied: E316.** SPEC 11.1's precedence rules settle it without
having to choose between the two identifiers in the abstract:

- SPEC 5.4 defines the left side of a body statement *textually* — it "ends at
  the first `:` at bracket depth 0 that is outside a quoted string" — and
  requires it to be a path whose every segment is an identifier. E316's own
  condition in SPEC 10.3 is "Invalid field name or empty path segment" with the
  message `Invalid field name '{text}'.`, so the whole left side is what E316
  names, and it is reported at the left side's first token.
- E210 is the generic "a token or construct that is not valid here", reported
  at the offending token.
- For `note #x`, the two candidates sit at different positions, and SPEC 11.1
  rule (3) — "within a step, the earliest source position" — selects the one at
  the start of the left side: **E316**.
- When the offending token *is* the first one (`(a, a): (1, 2)` in `dte-32`,
  `!!! total garbage` in `dte-01`), there is no field name to name and the two
  candidates share a position, so SPEC 11.1 rule (4) — "at the same position,
  the lower numeric identifier" — selects **E210**.

The compiler implements exactly that rule, so `oc-10`, `dte-12`, `dte-32` and
`dte-01` all pass, and the corpus becomes self-consistent. A path that *is*
built only from path material keeps E316 whatever its shape, which is what SPEC
5.4 states for `a..b`, `.a`, `a.` and `name[0]`.

The rule is recorded in `SPEC-ISSUES-conformance.md` item 2, with the wording
change SPEC 10.3 would need to make it explicit.

---

# Fix stage — decisions taken while clearing DEFECTS-1 and DEFECTS-2

Every case marked `status = ready` under `conformance/adversarial-1` and
`conformance/adversarial-2` was re-derived from SPEC before the compiler was
touched. **No verifier expectation was found wrong**, so no expectation file
was edited and no case was re-marked; all twenty-seven now pass against the
expectation they were filed with. The six `status = pending` cases are
untouched and still behave exactly as their entries describe: they turn on
specification rulings (S3/S14, S10, S11, S12, and DEFECTS-2's S1 and S2), not
on implementation defects.

Four fixes needed a decision the specification does not make, and one message
was changed where the specification leaves the substitution undefined. They are
recorded here because a second implementation cannot derive them from SPEC as
it stands.

---

## 3. A project version range is bounded by count (DEFECTS-1 D1, S2)

Case: `adversarial-1/adv-01-version-range-abort`, which pins **E602**.

SPEC 3.7 is normative and unconditional: "An implementation MUST NOT abort,
panic or crash on any input." SPEC 4.12 bounds neither number of a `versions`
declaration and SPEC 7.4 compiles every version in the range, so
`versions 1..2000000000` is a legal four-line program that no implementation
can materialise. The three sentences cannot all hold, which is what S2 records.

The crash is the only one of the three that SPEC states as a MUST NOT, so it
was removed: the implementation now bounds the **count** of versions a project
range may cover, at `versions::MAX_PROJECT_VERSIONS` (4096), and reports a
wider range as E602 with a message that names the bound:

```text
error[E602]: Invalid version range '1..2000000000'; a project range covers at most 4096 versions.
```

The bound is on the count, never on the numbers: `versions 100..163` declares
64 versions and is as legal as `versions 1..64`. A version number larger than
`u32::MAX` is separately E602, with its own message. The old message —
`Invalid version range '1..4294967296'; both are integers >= 1 and min <= max.`
— denied its own condition and is no longer produced for either case; the
catalogue message of SPEC 10.6 is now reported only for a range that really is
malformed (`0..3`, `3..1`, `-1..3`).

4096 is an implementation choice, not a reading of the specification. S2's
resolution — a new SPEC 3.7 row, an explicit cap in SPEC 4.12, or a sentence
saying the range is bounded by the implementation — still has to be taken, and
whichever is chosen fixes the number a conforming implementation must use.

## 4. E604 on a body statement (DEFECTS-2 D1)

Cases: `adversarial-2/adv2-01`, `adv2-02`, `adv2-03` (all E603).

E603 on a body statement is required, not chosen: SPEC 4.12 states the rule of
every version annotation ("`n` MUST be within the project range … otherwise
E603"), and SPEC 11.1's "E603 before E430 and E440" can be about nothing but a
body statement, because SPEC 10.4 defines E430 for no other construct. The
three cases are fixed by checking both numbers before the applicability set is
formed.

The reversed window `@since(3) @removed(2)` on a statement is the part SPEC
does not settle, and DEFECTS-2 asked for it to be decided deliberately. **It is
now E604**, the diagnostic an instance header already gets for the same
window. The reasons: SPEC 4.12 states the rule of the annotations themselves
("`@removed(n)` MUST have `n` strictly greater than the … effective `@since`,
otherwise E604") rather than of the construct that carries them; SPEC 5.14
restates it verbatim for a header, and SPEC 5.13 restates neither of 4.12's two
rules, so reading its silence as "E603 applies, E604 does not" splits a pair the
specification states together. The alternative — leaving it E430 — also produced
the degenerate message DEFECTS-2 recorded, `the statement is annotated none`,
because the annotation was substituted after intersection with the project
range. With E603 and E604 both checked first, that substitution is now
unreachable and E430 always names the range the author wrote.

A SPEC amendment adding "and E604" to SPEC 5.13's annotation bullet would make
this derivable rather than inferred.

## 5. E422's `{actual}` for a file whose signature matched (DEFECTS-1 D11, S6)

No case: the corpus expectation format records identifiers, not message text.

SPEC 10.4 gives E422 the template `… '{path}' is {actual}, not {expected}.` and
SPEC 4.4.7.1 prescribes the notes for a truncated or malformed header, but
neither says what `{actual}` is when the file's signature matched its declared
extension. The implementation substituted the declared format, which rendered
as `'a.bmp' is bmp, not bmp.`

`{actual}` is now `unreadable`, for all four of the matched-but-malformed
outcomes (truncated, malformed BMP width, malformed canvas size, no
start-of-frame marker):

```text
error[E422]: Image content mismatch at x.icon: 'a.bmp' is unreadable, not bmp.
  note: malformed BMP width.
```

Nothing is lost: the note names the check that failed and `{expected}` already
names the declared format. The genuine format mismatch
(`is not an image (5A 5A …), not bmp`) is unchanged, and the notes are the ones
SPEC 4.4.7.1 prescribes. S6 still needs a ruling; this wording only removes the
self-contradiction.

## 6. The `{` of a `${` in an unquoted value (DEFECTS-1 D12, S9)

Case: `adversarial-1/adv-14-unquoted-dollar-brace`, which pins **E427**.

SPEC 3.6 classifies every `{` as a multi-path `{` or a block brace, and S9
records that three constructs are neither. The fix touches exactly one of the
three, and does so on rules the specification does state: SPEC 3.1 lists `${`
among the constructs that "are single lexemes", so its `{` is part of that
lexeme; SPEC 5.11 says "An unterminated `${` is E427"; and SPEC 10.4 gives E427
the example `label: ${id`, which is this case character for character. The
lexer now tracks a `${`'s brace apart from a block brace, and an unterminated
one leaves the text intact so that interpolation reports E427 where SPEC 5.11
says it does.

The other two `{` kinds S9 names are untouched: a brace file pattern
(`imgs: a/{p,q.png`) is still E203, and a literal `{` in bare text is still an
ordinary character. S9's ruling still decides those two, and decides whether
the brace-pattern case should be E435.

---

## Message changes with no case

The corpus records identifiers, so these are listed rather than pinned.

- **E412 for a bare `@name` header flag** (DEFECTS-1 D20). SPEC 5.2's bare flag
  form writes a boolean, so `{found}` is now `bool`. On a `text` field the
  message read `expected text, found text`; it now reads
  `expected text, found bool`.
- **E210 for a byte-order mark.** `describe_char` now spells `U+FEFF` as
  `byte-order mark U+FEFF` instead of printing the invisible character between
  quotes.
- **E204 inside a bare text** now spells the open bracket it names, so a `${`
  is reported as `${` rather than as a bare `{`.
- **E210 for trailing text after a value** now names the whole trailing run as
  one bare text (`Unexpected text 'b"' here; expected end of line.`), which is
  what SPEC 5.5 calls it. It was previously reported per token, which is why a
  stray `"` in that run opened a string and produced E201 further right
  (DEFECTS-1 D18).
