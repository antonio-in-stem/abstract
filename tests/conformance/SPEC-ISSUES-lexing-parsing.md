# Spec questions raised while deriving the lexing-parsing and instances-merge expectations

Scope: the 31 `lexing-parsing/lp-*` and 25 `instances-merge/im-*` cases. **No case was left
undecided**: every case in both areas is `status = "ready"`. The entries below record readings
that the spec settles only by combining two passages, so that the spec editor can either confirm
the reading or tighten the wording. Each entry names the cases that depend on it.

---

## Q1. Is a `{` inside a value a bracket of the value, or a block brace?

SPEC 3.6 and GRAMMAR L5 both say: "A `{` immediately preceded by `.` opens a multi-path key
list; every other `{` is a **block brace**", and a block brace "MUST be the last token of its
logical line". Read alone, that makes the `{` of SPEC 5.8's own example

```abstract
images: ./textures/{hero,thumbnail}.png
```

a block brace, which contradicts 5.8.

Reading applied: the rule in 3.6 / L5 governs `{` **tokens**, and a value's unquoted run is one
`bare_text` token (3.5, GRAMMAR `bare_text`), inside which `{` and `}` are ordinary characters
that raise and lower the value's bracket depth. This is what makes the depth-0 comma rule of
`bare_text` work for `{hero,thumbnail}` and is required by 5.8's last bullets ("For every other
field type, `{` and `}` are ordinary characters with no expansion"). The `bare_text` prose
"Brackets inside a bare_text MUST be balanced and correctly typed" then supplies the error for a
value whose braces are not balanced, via 5.5 ("An unbalanced or wrongly typed bracket in a value
is E203/E204/E205").

Consequences recorded in the corpus:

- `lp-012` (`tags: ./art/{red,blue}/icon/{small,large}.png` on a `text[]` field) and `lp-013`
  (`note: {x}.{y}`) are **accepted**, the braces reaching the output verbatim.
- `lp-006` (`icon: a}.{b`) is **E205**: the `}` closes nothing, and it precedes on position the
  E203 that the trailing `{` would raise at end of file (11.1).

Question for the editor: add one sentence to 3.6 saying that the `{`/`}` rule applies to tokens
outside a value, and that inside a `bare_text` both are ordinary characters subject only to the
balance requirement.

## Q2. Which identifier does a stray `)` in a value carry?

`lp-005` writes `note: #tag)x(`. 5.5 says a leading `#` always starts a tag object and that "Any
text after the closing `)` is E210", while 3.6 says unconditionally that "a closing bracket with
an empty stack is E205".

Reading applied: **E205**. The tag object here has no argument list at all (the `(` of a
`tag_object` must be adjacent, 3.1 / L9), so there is no "closing `)`" for 5.5's sentence to
refer to; the `)` is simply a closing bracket with an empty value-bracket stack. Both candidates
sit at the same position and in the same phase, and 11.1's last tie-break ("the lower numeric
identifier") would also select E205.

Question for the editor: confirm that 5.5's "text after the closing `)` is E210" is meant only
for text that follows a *well-formed* `#name(...)`, and that a bracket that closes nothing is
always E205.

## Q3. In which phase is E428 raised for `@id` written as a bare flag?

`im-04` contains both a duplicate id (`beta.ab` repeats `@id.42`, E402) and `Thing :: @id` with
no value (`delta.ab`, E428 by 5.2). 7.2 lists E402 explicitly as a P2 diagnostic but does not
list E428; 5.3 makes the id something P2 computes, which is where the "value MUST be an
identifier" check has to live.

Reading applied: **both are P2**, so 11.1 orders them by source position in the sorted order of
2.4 — `data/things/beta.ab` before `data/things/delta.ab` — giving `E402` then `E428`. If E428
were instead a P1 parse diagnostic, P2 would never run and E402 would be unreachable for this
input, so the two readings differ in the recorded expectation and not only in the order.

Question for the editor: name E428 (and E410) in 7.2's P2 bullet list, or state that both are
P1.

## Q4. How many diagnostics does one input mandate?

11.1 mandates only the first diagnostic ("when it reports only one, that one MUST be the
first"), while 9.3 makes `--max-errors` default to 20 and 11.2 compares `err.txt` byte for byte,
which fixes the whole reported set for a golden case.

Convention applied in these two areas, so that the files are usable either way:

- one identifier per statement, instance or file that independently satisfies an error
  condition, in the precedence order of 11.1;
- a repeated identifier only when the diagnostics come from different statements
  (`im-04`, `im-14`, `im-18`, `im-21`), never for two faults inside one statement
  (`lp-015`, whose second `$` reference is noted in `case.toml` as permitted but not mandated);
- for an input that repeats one fault hundreds of times (`im-06`, `im-07`), a single identifier,
  with the repetition described in `spec_note`.

Question for the editor: state in 11.2 whether a golden case's `err.txt` is expected to contain
every diagnostic a default `--max-errors` run produces.

## Q5. Errors masked by the phase order (not a defect, recorded for coverage)

7.1 runs a phase only if every earlier phase succeeded, so several cases can never reach the
diagnostic the original audit finding was about:

- `lp-019`: E428 for the non-identifier `@id` value (P2) masks the E424 that
  `data/things/y.ab` would raise for `../../SECRET_OUTSIDE/secret.png` (P4).
- `im-13`: E403 for the clone written before its header (P1) masks the E424 that
  `data/things/abs.ab` would raise for an absolute `C:/...` value (P4).

Both cases are kept as rejection cases with the identifier the spec actually mandates. E424
therefore needs at least one dedicated case in the asset area, or these two inputs need to be
split; flagged here because 11.3 requires a case for every reachable identifier in chapter 10.
