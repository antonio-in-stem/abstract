# SPEC issues raised while deriving expectations — areas `logic-engine` and `output-canon`

Scope: the 28 `logic-engine` cases and the 27 `output-canon` cases. Every expectation was
derived from `docs/SPEC.md` alone; 0.2.0 behaviour was never used as evidence. The renderers
that produced every `expected.json`, `expected.yml` and `expected.raw` were validated by
reproducing, byte for byte, the SPEC's own §8.1, §8.4, §8.5, §8.6 samples and the complete
Appendix C.6 (JSON) and C.7 (YAML) worked example.

## A. Blocking questions (case status `ambiguous`)

None. Every case in both areas is decidable from the spec as written.

## B. Non-blocking clarifications requested

These did not stop a case from being decided, but each one is a place where two readings of
the spec are defensible and the wording could be tightened. The reading adopted by the corpus
is stated for each.

### N1 — a `derive` right-hand side that is `$var.path` with `$var` unbound (LOGIC-14)

§6.4 says the right-hand side "is read as one of the first four forms when its first token is
`.`, a loop variable, the keyword `length` or the keyword `version`", and §6.5 says `$x.a`
"starts at the value bound to the loop variable `$x`". §6.9 then splits the unbound case in
two: bullet 4 assigns **E510** to a `$name` used "as an operand or as a path segment" that no
enclosing loop binds, while bullet 5 says a `$name` "inside a derive value or a throw message"
that no enclosing loop binds "resolves against the **current** object", is **E503** at P3 when
it is not a declared root field, and **E425** at run time when it is a declared root field with
no current value.

**Question.** For `derive .x = $owner.team`, where `owner` is a declared root **group** field
and no `for` is in scope, which rule governs? Bullet 4 names operands and path *segments*, and
`$owner` is neither — it is the *root* of a `loop_path`. Bullet 5 covers "a derive value", but
its outcomes (E503, E425) do not cover a declared root field whose current value is an object
rather than a scalar.

**Adopted.** E510. §6.4's form-selection rule is purely lexical, so `$owner.team` is a
`loop_path`, and E510's own text ("Unbound variable `'${name}'`") is the closest fit.
Suggested wording: in §6.9 bullet 4, replace "as a path segment" with "as a path segment or at
the head of a path".

### N2 — a statement left side that is not a path (OC-10)

§5.4 defines the left side of a body statement textually ("ends at the first `:` at bracket
depth 0 that is outside a quoted string") and requires each path segment to be an identifier.
§10.3 gives E316 the message `Invalid field name '{text}'.` but illustrates it only with the
empty-segment case `a..b: 1`.

**Question.** For `note #x: hello` — a valid identifier followed by a token that can never
continue a path — is the diagnostic **E316**, naming the whole left side, or the generic
**E210** (`Unexpected {found} here; expected {expected}`)?

**Adopted.** E316, because §5.4 makes the left side one lexical unit that must be a path, and
because "invalid field name" is what the author actually wrote. If the intent is that the path
is parsed token by token, E210 is the right answer and §10.3's E316 row should say so.

### N3 — a non-final path segment naming a scalar field, written before the scalar (OC-03/OC-04, `case-c`)

§5.4 phrases E443 value-first — "A path that traverses a value **already holding** a
non-object … is E443" — and separately gives one schema-directed rule, for list fields:
"When a non-final segment names a field the schema declares as a list, the statement is E443."

**Question.** Is there an equivalent schema-directed rule for a non-final segment naming a
field declared as a **scalar**? Read strictly value-first, `label.sub: boom` followed by
`label: hello` raises nothing: the second statement is a plain assignment at `label` (§7.3
step 1) and silently discards the object the first one built — which §1.2 P4 forbids ("no
silently dropped statement").

**Adopted.** E443 in both statement orders, decided from the schema. Suggested wording:
generalise the list sentence to "When a non-final segment names a field the schema declares as
anything other than a group or a `$(Schema)`, the statement is E443", and keep the value-based
sentence for paths built entirely from group fields.

### N4 — how much a golden may pin after a P1 failure (LOGIC-03, LOGIC-07, LOGIC-10, LOGIC-11, LOGIC-21, LOGIC-26)

§7.1 allows an implementation to report several diagnostics from one phase and §11.1 fixes
their order, but nothing specifies how the parser **recovers** from a malformed construct, so
the diagnostics after the first are implementation-defined. LOGIC-03 is the clearest example:
the stray `}` on line 10 makes line 11 E210, and a recovering parser then also reports E210 on
line 12 and E205 on line 13.

**Adopted convention for this corpus.** `expected-error.txt` pins **only the first
diagnostic** when the failure is in P1, and pins **every** diagnostic, in source order, when
the failure is in P2 or P3, where the spec states that every schema, every logic block and
every instance is checked (§7.2). Six cases in `logic-engine` rely on the first half of this
convention and four (LOGIC-02, LOGIC-04, LOGIC-09, LOGIC-28) on the second. If the conformance
runner is to compare stderr byte for byte (§11.2), parser recovery needs to be specified.

### N5 — the phase of E511 / E512 / E513

§7.2 lists E511, E512 and E513 among the P3 checks, but a condition such as `.status = "active"`
cannot be parsed in P1 either, so E511 could equally be a P1 diagnostic.

**Question.** Is a `condition` parsed in P1 and only type-checked in P3, or is the whole logic
body kept as tokens until P3?

**Adopted.** The answer changes no case in these two areas: no case mixes a malformed condition
with an error from a different phase. It becomes observable as soon as one project contains
both a malformed condition and a broken schema, so it is worth pinning before such a case is
written.

## C. Corpus repairs and notes

- **The runner cannot express an argv.** `project/tests/conformance.rs` compiles the case
  directory with `CompileOptions::default()`; there is no `cmd` file and no way to pass a flag
  or a format keyword. Three `output-canon` cases are about the command line and are therefore
  `status = "manual"`, with their mandated outcome recorded in `note_1_0`: **OC-08** (broken
  pipe, §9.7), **OC-09** (`--out` onto a source file, E808, §9.4) and **OC-14** (`--out` with no
  value, E803, §9.3, plus E811, E810 and the accepted `--out YML`). Giving the runner a `cmd`
  line and an `exit` expectation, as SPEC §11.2 describes, would make all three runnable; SPEC
  §11.3 requires such cases anyway.
- **RAW expectations are named `expected.abraw`.** That is what the runner looks for, and SPEC
  §11.2 names the artifact `out.abraw`. Two files elsewhere in the corpus use `expected.raw`
  and are therefore silently ignored by the runner:
  `docs-tests-examples/dte-11/expected.raw` and `docs-tests-examples/dte-31/expected.raw`.
- **`expected-error.txt` holds error identifiers, one per line.** The runner currently compares
  it byte for byte against the rendered diagnostics, so identifier-only files will not match a
  full `path:line:col: error[Exxx]: message` rendering. Every area of the corpus writes
  identifiers, so the comparison needs to extract them (or the files need to grow full
  messages) before the suite can go green.
- **OC-01, OC-17 and OC-27 carry both `expected.json` and `expected.yml`.** The runner takes
  the first match and so checks only the JSON; the YAML file is the other half of the
  cross-format equivalence SPEC §11.3 requires and was validated against SPEC §8.5 and
  Appendix C.7.
- **OC-09** — `data/items/a.ab` had been captured from the 0.2.0 repro *after* the destructive
  `--out` write had already replaced the source with a JSON document, so the project could not
  parse and the case tested the wreckage rather than the refusal. It has been restored to the
  two-line source the case's own `command` field records
  (`T :: @id.a` / `label: precious source`).
- **OC-23** — the NTFS junction `data/loop -> data` has been recreated with
  `mklink /J` (it did not require elevation), so the cycle-detection case is runnable. The
  case's own `make-loop.cmd` recreates it if the corpus is copied without it. A tool that walks
  the corpus without following-link protection will loop here.
- **OC-08** — besides depending on argv, the case carries no project of its own: a default
  compile of its directory would collect zero source files and report E103.
- **OC-01, OC-12** — the `oc19-corpus/` directories are empty placeholders for the private
  real-world corpus. They are deliberately not populated: that corpus is read-only and must
  never be copied into this repository.
- **OC-18** — kept as a rejection case although the underlying defect (the `lang` →
  `lang_values` rename) no longer exists in 1.0: the stored source names a tuple column
  `lang_values` that the group does not declare, which is E409 for an unrelated reason.
