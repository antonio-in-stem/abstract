# SPEC readings applied while making the corpus pass

Scope: the whole corpus, run end to end against the 1.0 compiler. The entries
below record the readings that decided a case where the specification settles
the question only by combining two passages, or where two areas of the corpus
had adopted opposite answers. Each names the cases that depend on it and the
wording that would make the reading explicit.

Two expectation files were corrected as a consequence; both are recorded in
`CORRECTIONS.md` with their own derivation.

---

## 1. The one `ambiguous` case: `cli-project/cli-08`, `bundle --key … --plain`

**Question recorded by the case.** Appendix D.1 item 1 makes `--key` together
with `--plain` a usage error and forbids writing an unencrypted container while
a key was given, but SPEC 10.8 defines no identifier for two mutually exclusive
flags: E802 is an unknown flag, E803 a missing value, E811 a repeated flag and
E812 an invalid flag value, and none describes this condition.

**Reading applied: the refusal carries no catalogue identifier, and the case is
not a language conformance case.** SPEC 9.9 places `bundle` and `unbundle`
outside the language and outside conformance, and Appendix D binds only the
reference implementation. SPEC 10 is the catalogue of *language* diagnostics,
so the absence of an identifier is not an omission: there is nothing to assign,
because the condition cannot arise in `compile`, `lint`, `templates` or `init`.
What Appendix D.1 item 1 does fix is the behaviour, and that is testable: the
invocation is refused, exit code 2, and no container is written.

The compiler implements it that way — `bundle::validate_bundle_flags` returns
its own `BundleFlagError`, which the CLI prints as `abstract: error: …` with no
`error[Exxx]` prefix, the convention SPEC 9.9 leaves open and `cli.rs`
documents. The behaviour is covered by
`tests/cli_tests.rs::bundle_flag_errors_keep_their_wording_and_exit_code`
rather than by a corpus golden, because the corpus runner compares catalogue
identifiers.

`cli-08`'s `status` was therefore changed from `ambiguous` to `manual`, the
value the corpus already uses for the other seven Appendix D cases
(`dte-05`, `dte-06`, `dte-27`, `mbc-02`, `mbc-03`, `mbc-08`, `mbc-09`,
`mbc-14`, `oc-08`, `oc-09`, `oc-14`). No `ambiguous` case remains.

**Suggested wording.** One sentence in Appendix D.1: "A `bundle` or `unbundle`
usage error carries no chapter 10 identifier; the reference implementation
prints it as `abstract: error: {message}` and exits 2."

---

## 2. A statement left side that is not a path: E316 or E210

**Cases:** `output-canon/oc-10`, `docs-tests-examples/dte-12`,
`docs-tests-examples/dte-32`, `docs-tests-examples/dte-01`,
`schema-types/yaml-key`.

Raised as N2 in `SPEC-ISSUES-logic-engine.md`; two areas of the corpus had
adopted opposite answers for the same input (`note #x: hello`).

**Reading applied.** SPEC 11.1's precedence rules decide it, so neither
identifier has to win in the abstract:

- E316 is reported at the **first token of the left side** and names the whole
  text, because SPEC 5.4 makes the left side one lexical unit that must be a
  path and SPEC 10.3 gives E316 the condition "invalid field name".
- E210 is reported at the **offending token**.
- SPEC 11.1 rule (3) prefers the earlier position, and rule (4) prefers the
  lower identifier at the same position. So a left side whose *first* token can
  never begin a path is E210 (`(a, a): (1, 2)`), and one whose defect appears
  later is E316 (`note #x: hello`).
- A left side built only from path material — names, `.`, `..`, `[`, `]` — is
  always E316, which is what SPEC 5.4 states for `a..b`, `.a`, `a.` and
  `name[0]`.
- Inside a **schema** body every declaration begins with a field name (SPEC
  4.3), so a token that cannot be one is E316 there, which is what SPEC 4.13's
  "Invalid field name" row says (`yaml-key`: `[a]:`, `*b:`, `&c:`).

**Suggested wording.** Add to SPEC 10.3's E316 row: "reported at the first
token of the left side, naming the whole of it; a left side whose first token
cannot begin a path is E210 at that token instead."

---

## 3. A `{` inside a value: bracket of the value, or block brace

**Cases:** `lexing-parsing/lp-006`, `lexing-parsing/lp-012`,
`lexing-parsing/lp-013`, `language-design/l13`, `docs-tests-examples/dte-07`,
`instances-merge/im-22`.

Raised as Q1 in both `SPEC-ISSUES-lexing-parsing.md` and
`SPEC-ISSUES-docs-tests-examples.md`, independently and with the same answer.
Confirmed here and implemented.

SPEC 3.6 and GRAMMAR L5 say every `{` not preceded by `.` is a block brace,
which read alone would make SPEC 5.8's own example
`images: ./textures/{hero,thumbnail}.png` a block brace and split the value at
the comma. SPEC 5.8 requires the opposite: the group's comma is part of the
value, and for a field of any other type `{` and `}` are "ordinary characters
with no expansion".

**Reading applied.** SPEC 3.6's rule governs `{` as a **token**, outside a
value. A value's unquoted run is one `bare_text` lexeme (SPEC 3.5) inside which
`{` and `}` are ordinary characters that raise and lower the run's own bracket
depth, subject to `bare_text`'s balance requirement. That makes the depth-0
comma of a brace group part of the value, and supplies E203/E204/E205 for
braces that do not balance (`lp-006`, `icon: a}.{b`, is E205 at the `}` and
E203 at the `{` it leaves open).

**Suggested wording.** One sentence in SPEC 3.6: "This rule applies to `{` as a
token. Inside a `bare_text` both `{` and `}` are ordinary characters that raise
and lower the run's bracket depth and are subject only to the balance
requirement of §3.5."

---

## 4. The phase of E428 and E410

**Cases:** `instances-merge/im-04`, `instances-merge/im-03`,
`schema-types/template-field`, `output-canon/oc-05`,
`lexing-parsing/lp-010`.

Raised as Q3 in `SPEC-ISSUES-lexing-parsing.md`, which asks the editor to name
E428 and E410 in SPEC 7.2's P2 list or to state that both are P1. The corpus
constrains them further than that question suggests, and the two land in
*different* phases.

**E428 is P2.** SPEC 5.3 makes the id something the project tables compute, and
SPEC 7.2 lists the instance table among P2's work. `im-04` pins `E402` then
`E428` for a project where one file repeats an id and another writes `@id` as a
bare flag; if E428 were a P1 parse diagnostic, P2 would never run and E402
would be unreachable. The parser therefore records the text it could not use
and P2 reports it, ordered against E402 by the source order of SPEC 2.4. An id
that is not an identifier names nothing, so it takes no part in the uniqueness
(E402) and length (E413) checks.

**E410 is P4.** `template-field` pins `E314` alone for a project whose schema
declares a field named `template` *and* whose instance assigns `template`. SPEC
7.1 runs a phase only if every earlier phase succeeded, so E410 must come after
P3: it belongs to step 1 of SPEC 7.3, where the header tags, the clone
statements and the body statements are applied. A statement that names an
envelope key is refused there, writes nothing, and therefore takes no part in
P2's duplicate-assignment check (E429) either — without that, `@id.a` in the
header plus `id: other` in the body would be reported as E429 in P2 and mask
the E410 that `im-03`, `oc-05` and `lp-010` pin.

**Suggested wording.** In SPEC 7.2's P2 bullet list, add "every id is an
identifier (E428)" to the `instances` bullet. In SPEC 7.3 step 1, add "A header
tag, clone path or body statement whose leading segment is `template` or `id`
is E410."

---

## 5. Lexing and parsing are one phase, ordered by position

**Cases:** `language-design/l02`, `logic-engine/logic-03`,
`lexing-parsing/lp-004`, `docs-tests-examples/dte-01`,
`schema-types/name-space`.

SPEC 7.1 puts "lex + parse" in one phase, P1, and SPEC 11.1 orders the
diagnostics of one phase by source position. Four cases pin a diagnostic that
only the parser can raise, in a file where the lexer also fails *later* in the
file: `l02` pins E210 for `schemma W {` on line 1 although the lexer reports
E207 on line 10, and `logic-03` pins E210 for a `require` at template top level
on line 11 although the lexer reports E205 on line 13.

**Reading applied.** A compiler that stops at the first lexical defect cannot
satisfy SPEC 11.1 rule (3). P1 therefore parses the recovered token stream even
when lexing reported diagnostics, and merges the two lists into source-position
order. A line the lexer could not tokenize contributes only its lexical
diagnostic: whatever the parser then makes of the recovered tokens on that line
is a consequence of the same defect, not a second one, which is what keeps
`im-14`, `l20` and `dup-nonascii` at one identifier per defective line.

The same rule decides `lp-004`, whose `spec_note` states it outright: an
unterminated `[` keeps the value-bracket stack non-empty to end of file (SPEC
3.6), so E203 is reported at the opening bracket on line 2 and outranks, on
position, the E210 the missing separator raises on line 3.

**Suggested wording.** In SPEC 7.1, after the phase table: "P1 is one phase: an
implementation that reports several diagnostics reports the lexical and
syntactic ones together, in the order §11.1 fixes." In SPEC 11.2, state whether
parser recovery is part of the golden contract; today it is not, which is why
this corpus pins identifier prefixes rather than byte-exact stderr.

---

## 6. What `expected-error.txt` pins

Every `expected-error.txt` in the corpus holds diagnostic **identifiers**, one
per line, not rendered messages. That is the convention the expectation stages
recorded in `SPEC-ISSUES-docs-tests-examples.md` and
`SPEC-ISSUES-logic-engine.md`: SPEC 7.1 fixes no recovery rule, so only the
first identifier and the further *independent* ones are common to every
conforming compiler.

`tests/conformance.rs` therefore compares the identifiers a run reports against
the file as a **prefix**: the pinned identifiers must be the first ones, in
order, and a compiler may report more. SPEC 11.2 describes a byte-exact
`err.txt` instead; making the corpus byte-exact needs a recovery rule in SPEC
7.1 first, and is noted here rather than silently adopted.

---

## 7. Other readings the compiler now implements

Each of these was decided from the spec while making one case pass. They are
smaller than the items above and are listed for the record.

- **An enum member is an identifier.** SPEC 10.3 gives E306 exactly two message
  forms — an empty member list and a duplicate member — so a token in a member
  position that is not an identifier is E210, not E306
  (`schema-types/enum-modifier-strip`, `schema-types/star-member`). Since `*`
  is not an identifier character, a member can never end with one, which is
  what SPEC 5.6 relies on when it says a wildcard can never shadow a member.
- **A schema name runs to the block brace.** SPEC 10.2 gives `schema My Thing {`
  as E208's own example, so the name a `schema` or `logic` line declares is
  everything before the `{`, and `My Thing` is reported whole
  (`schema-types/name-space`). Inside `ref(` and `$(` the name ends at the
  closing bracket, unchanged.
- **`#prefix*` is a tag name.** SPEC 5.6 position (c) makes the trailing `*`
  part of the `#tag` shorthand's name, bracketed or bare, so the lexer folds it
  into the name rather than emitting a `*` token (`instances-merge/im-19`,
  `instances-merge/im-22`).
- **Operators have no place in an instance file.** Comparison and logic
  operators belong to a logic block (SPEC chapter 6); outside a value an
  instance file has no token they could spell, so one is E210 and the rest of
  the physical line is abandoned rather than lexed as a value
  (`docs-tests-examples/dte-01`).
- **`compile_sources` skips the on-disk asset checks.** The in-memory entry
  point has no project root, so E421, E422 and E423 cannot run and are skipped
  exactly as `--skip-assets` skips them; E420 and E424 are pure path checks and
  still run. SPEC 7.6 makes the compiled bytes identical either way, so no
  observable behaviour depends on the choice.
