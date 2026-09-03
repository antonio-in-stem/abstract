# Spec questions raised while deriving the docs-tests-examples, language-design and media-bundle-crypto expectations

Scope: the 32 `docs-tests-examples/dte-*`, 24 `language-design/l*` and 14
`media-bundle-crypto/mbc-*` cases. **No case was left undecided**: 62 are `status = "ready"`
and 8 are `status = "manual"` (Appendix D optional tooling — bundle/unbundle, the Java runtime,
the editor extension, the distribution's install commands — which §9.9 explicitly leaves
outside the language, so their outcome is not fixed by the spec and cannot be a compiler
golden). Entries Q1–Q4 below record readings the spec settles only by combining two passages,
or, in Q3, a place where the spec contradicts the design decisions record. Each entry
names the cases that depend on it.

---

## Conventions used by the expectation files in these three areas

Recorded here because there is nowhere else in the corpus to state them.

- **Where the file lives.** The expectation for a case's *primary* invocation is at the case
  root (`expected.json` / `expected.yml` / `expected.abraw` / `expected-error.txt`; the RAW
  suffix is `.abraw`, as §11.2 spells the RAW golden and as `output-canon` already uses it,
  and it is the only one `tests/conformance.rs` recognises). A secondary
  scenario that has its own subdirectory carries its expectation inside that subdirectory
  (e.g. `dte-18/only24/expected.json`). A second invocation over the same tree that differs
  only by flags or arguments uses a suffix (`dte-03/expected-skip-assets.json`,
  `dte-13/expected-error-abt-arg.txt`). Every case's `expectation_1_0` key names the command
  that produces each file.
- **`expected-error.txt`** holds error identifiers only, one per line, in the order §11.1's
  precedence rules give. The first line is the diagnostic a conforming compiler MUST report
  first. Further lines are the further diagnostics a compiler collecting up to the default
  `--max-errors 20` must report, in the same order; they appear wherever the diagnostics are
  independent — different files, different instances, or two clearly independent declarations or
  statements inside one schema, one logic block or one instance (`l06` lists two E409 for two
  fields of one instance, `l07/case18` two E314 for two declarations of one schema, and
  `dte-02` / `dte-16` / `dte-21` / `dte-23` follow the same rule). Where a
  later phase would not run because an earlier one failed (§7.1), only the earlier phase's
  diagnostics are listed. Cascaded recovery diagnostics inside one statement are never listed.
- **`abstract.compiler`** is written as `"1.0.0"` in every accepted-document expectation, as the
  spec's own examples do. §11.2 requires the runner to normalise that string on both sides
  before comparing, so the value is not part of the contract.
- Every accepted-document expectation carries exactly one terminating line break and otherwise
  uses the byte-exact forms of §8.4 (JSON), §8.5 (YAML) and §8.6 (RAW): two-space indentation,
  `": "` between key and value, no trailing whitespace on any line, and the number spelling of
  §8.7. The 30 JSON expectations in these three areas were checked mechanically against that
  rendering, against the envelope shape of §8.1 and against the `(template, id)` ordering of
  §2.7.
- **Line endings.** Every expectation file in the whole corpus — all nine areas — is stored with
  `CRLF`, which §11.2 normalises to `LF` on both sides before comparing, so the stored form is
  conformant. A runner that skips that normalisation will fail every case; a corpus-wide sweep
  to `LF` would remove the dependency, and must be done for all areas at once or not at all.
- **Nothing under the private corpus was read, copied or referenced.** Three staged scenarios
  point at it or at paths outside the corpus (`dte-20/check.sh`, `dte-27/check.ps1`,
  `dte-30/check.js`); none is runnable as a conformance case and each is recorded as such.

---

## Q1. Is a `{` inside a value a bracket of the value, or a block brace?

Same question as Q1 of `SPEC-ISSUES-lexing-parsing.md`, reached independently and answered the
same way. §3.6 and GRAMMAR L5 say the value-bracket stack holds `(`, `[` and the `{` of a
multi-path key list, and that "every other `{` is a **block brace**" whose `{` "MUST be the last
token of its logical line". Read alone that makes §5.8's own example
`images: ./textures/{hero,thumbnail}.png` a syntax error, and it would also make the depth-0
comma of §5.5 split `{hero,thumb}` into two list items.

**Reading applied:** the `{`/`}` classification of §3.6 governs tokens *outside* a value; a
value's unquoted run is one `bare_text` token, inside which `{` and `}` are ordinary characters
that raise and lower the value's bracket depth. This is required by GRAMMAR `bare_text`
("contains no `,` at bracket depth 0 of the value", "Brackets inside a bare_text MUST be
balanced and correctly typed") and by §5.8's statement that one brace group holds alternatives
separated by depth-0 commas.

Cases that depend on it: `dte-07` (`files: ./x/{hero,thumb}/{small,large}.png` on a `text[]`
field is **one** element, not three, and the braces reach the output verbatim), `l13` (the same
on `images[]`, plus `label: greet {name}.now`), `l03/case26` (the RAW re-parse is one
`data: [ … ]` statement and therefore one E403).

**Question for the editor:** add one sentence to §3.6 saying that the rule applies to `{` tokens
outside a value, and that inside a `bare_text` both braces are ordinary characters subject only
to the balance requirement.

---

## Q2. Within P1, may an implementation finish lexing a source before parsing it?

§7.1 makes "lex + parse" one phase, and §11.1 orders diagnostics of one phase by source
position. §10.2 lists E201 (unterminated string) and E210 (token not valid here) in the same
`E2xx` group, so no phase or step distinction separates them and position alone decides.

`dte-01` line 7 is `!!! total garbage ((( unterminated "`. Under the position rule the parse
error at 7:1 (E210, `!` cannot start a statement) precedes the unterminated string at 7:31
(E201), and `expected-error.txt` therefore says `E210`. An implementation that lexes each file
to completion before parsing it would report E201 instead, and would still satisfy every
sentence of §3 and §7.1 read on its own.

**Question for the editor:** state explicitly either (a) that lexing and parsing are interleaved,
so §11.1's position rule decides between a lexical and a syntactic diagnostic on one line, or
(b) that lexing is a sub-phase of P1 that runs to completion first, in which case §11.1 needs a
rule (1a) naming it. The same question decides whether the trailing `(((` of that line also
yields E203 at end of file; the expectation deliberately lists only the first diagnostic.

---

## Q3. CONTRADICTION with the design decisions record: is the interpolation variable table built before or after defaults?

Not a question about the spec's internal consistency — the spec is coherent — but a place where
it contradicts the design decisions record, reported rather than resolved.

- **SPEC §5.11:** "After all clones are merged and all of the instance's own statements are
  applied — and before defaults, before validation and before logic — the authored object's
  **root** fields whose syntax value is `Bare` or `Quoted` form the variable table."
- **SPEC §4.10** agrees and depends on it: "Because that table is built at §7.3 step 2 from
  authored values, a default may reference any root scalar field the instance itself authored."
- **SPEC §7.3** orders step 2 (interpolation) before step 4 (defaults).
- **Design decisions record, item F:** "variables = root scalar fields of the instance AFTER clones AND
  defaults (L11)". Revision 5 does not retract it.

The two readings differ observably. `l11`'s `Var.abt` declares `tier: text = gold` and
`v.ab` writes `path: ./textures/$tier/$id.png`. Under the spec, `tier` is not a variable and
`$tier` is **E425**; under decisions item F it resolves to `gold` and the tree compiles. The
expectation follows the spec (`l11/expected-error.txt` = `E425`, `E425`; the second is
`$nested`, which names a group and is a variable under neither reading).

**Decision needed** before the compiler stage implements §7.3: keep the spec as written and mark
decisions item F superseded, or change §5.11, §4.10 and §7.3 to fill defaults first. Note that
the spec's order is what makes a default's own interpolation well defined (§4.10) and what keeps
audit finding L11 ("`$defaulted_field` silently stays literal") only half-fixed: 1.0 makes it an
error rather than making it resolve.

---

## Q4. What is a `$name` whose maximal identifier-character run ends with `-`?

§5.11 says `$name` takes "the maximal following run of identifier characters", and §3.3 makes
`-` an identifier character while requiring that an identifier "MUST NOT end with `-` (E207)".
§5.11's own note ("`$id_large` refers to `id_large`, not to `id` followed by `_large`") confirms
that the run is greedy.

`l11/case5` and `l12` contain `label: $id-$missing-${id}`. The greedy run after the first `$` is
`id-`, which normalises to `id_` and is not a valid identifier. Three readings are available:

1. the run is the variable name; it names no root field, so **E425**;
2. the run is a malformed identifier, so **E207**;
3. the run is trimmed to the longest *valid* identifier (`id`), the `-` is literal text, and the
   first diagnostic is the later **E425** for `$missing`.

The expectation assumes reading 1 (`case5/expected-error.txt` = `E425`). `$missing` is E425 under
all three, so only the position and the identifier of the *first* diagnostic are at stake.

**Question for the editor:** add one sentence to §5.11 saying which of the three applies. Reading
3 is the friendliest and matches how `${name}` is motivated two bullets later; reading 2 is the
strictest and matches §3.3's wording most literally.

---

## Observations that are decided, but bite hard

Recorded because each one changed a staged case's outcome and each will surprise authors
migrating from 0.2.0.

**O1. A group with no `@optional` is a required field, even when every child is optional.**
§4.10 (no `@optional` and no default means required), §4.7 (a group may not declare a default;
an absent required group is E411) and §4.7's presence rule (present only when something wrote a
value at or under its path) combine to reject an instance that simply never mentions the group.
`l08` and `l24` both fail this way — `schema Pack { … slots { one: text @optional  two: text
@optional } }` and an instance that writes only `owner` is E411 — which is what turned two cases
about body-block syntax into rejection cases. Worth an explicit sentence in §4.7, because the
0.2.0 corpora are full of all-optional groups.

**O2. `abstract init`'s 0.2.0 scaffold is not a 1.0 project layout.** It puts `pack.ab` beside
`data/`, and §2.3/§2.4 make the data directory the discovery root, so that instance is not
collected at all — silently, because not collecting a file is not an error. `dte-20` records
this. §9.2's `init` contract already requires a different scaffold; the layout change is the
part most likely to be missed.

**O3. A default that is an empty string violates a `text(1..n)` range at schema time.**
`text(1..20) = ""` is E313 in P3 (§4.10, §4.4.1: the range counts Unicode scalar values, and 0
is outside `1..20`). Both trees of `dte-23` are rejected for this reason alone, before the
construct the case is about — a dotted multi-path prefix, which §5.4 makes legal — is ever
reached. A positive case for `a.b.{c, d}` needs a schema whose defaults are valid.

**O4. `--allow-unknown`, the three-argument direct form and the trailing `true` write flag are
E802 / E804 / E806 in 1.0.** Several staged `command` fields still carry them (`dte-12`,
`dte-13`, `l06`, `l16`, `l17`). Each affected case's `expectation_1_0` names the 1.0 spelling of
the invocation.

**O5. Three staged cases carry assets or files their instances never reference**, so the
behaviour the case is named for is not actually exercised: `mbc-04` and `mbc-13`
(`assets/small.webp`, the 25-byte VP8L that motivates the per-format byte requirement of
§4.4.7.1) and `mbc-07` (`assets/deep.jpg`, whose SOF0 sits at offset 0x692 and which the
byte-bounded walk now accepts). Adding one instance statement to each would turn them into
positive cases for the rules they document.

**O6. The runner cannot yet express most of these invocations.** `project/tests/conformance.rs`
calls `compile_project(<case dir>, CompileOptions::default())`: it always compiles the case
directory itself, with no flags and no format argument, and `case.toml` has no machine-readable
command field (`command` holds the audit's prose and `expectation_1_0` holds ours). Three
consequences, all for the runner to fix rather than the corpus:

1. A case whose primary invocation names something other than the case root compiles the wrong
   tree. `dte-13` (`split/` and `flat/`, one instance id `x` in each) and `dte-20`
   (`init-scaffold/my-pack` and `my-pack2`, identical ids) both become E402 duplicate-id
   failures when the case root is compiled, because §2.3 step 3 finds no `data/` child and the
   whole case directory becomes the discovery root. Every case that *does* carry a `data/` child
   resolves correctly by accident of the same rule.
2. Flags and formats are unreachable: `dte-03` needs a second run with `--skip-assets`, `dte-11`
   and `dte-31` need `RAW`, and every secondary scenario listed in an `expectation_1_0`
   (`dte-01/data-field-name`, `dte-13/flat`, `dte-14/final-round4`, `dte-17/edge`, `edge2`,
   `textnum`, `dte-18/only24`, `dte-19/only24`, `dte-23/ctl`, `dte-28/id-normalization`,
   `l03/case25`, `case26`, `l05/case16`, `case19`, `case24`, `l06/case18b`, `l07/case18`,
   `l08/case10`, `l11/case5`, `l12/case9`, `l14/case14`, `l17/case13`, `l18/case22`,
   `l19/case9b`, `l20/case19`, `l21/case18b`, `l24/case24`) is invisible, because a
   sub-directory without its own `case.toml` is not collected as a case.
3. The runner special-cases only `status = "pending"`. A `status = "manual"` case has no
   expectation file, so it is reported as "no expectation" rather than as deliberately
   out of scope. Teaching the runner `manual` would make that distinction visible.

A `cmd` key per scenario — the arguments that follow `abstract`, resolved relative to the
directory that holds it, exactly as §11.2 defines `cmd` — is the smallest change that closes
all three.
