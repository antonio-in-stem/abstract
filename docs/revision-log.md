# Abstract 1.0 — revision log

Scope: the revision of `SPEC.md` and `GRAMMAR.ebnf` that applies `review-audit-coverage.md`
(insertions `R01`–`R39`) and `review-implementability.md` (findings `B1`–`B18`, `M19`–`M40`,
`m41`–`m49`), followed by one end-to-end consistency pass over the whole specification.

Counts: **88 review items** — **87 applied** (70 as written, 17 in a modified form, where the two
reviews prescribed different wording for the same site or where a binding decision constrained the
fix) and **1 rejected**. Beyond the reviews, **13 further changes** were made: 10 restore or complete
constructs the binding decisions require and the reviewed draft omitted, and 3 repair defects found
in the consistency pass.

---

## 1. Audit-coverage review — `R01`–`R39`

| Item | Verdict | Where it landed / why |
|---|---|---|
| R01 | applied (modified) | §3.6 and GRAMMAR L5. Merged with **B1**, **M22** and **M34**: the value-bracket stack covers `(`, `[` and the `{` of a multi-path key list; every other `{` is a block brace, tracked for balance only; brackets inside a `quoted_string` are neither pushed nor popped; two continuation rules, (a) header tag list, (b) a line terminator before `else` inside a logic block. |
| R02 | applied | §4.4.1's example is now `caption: "Hello, world"`; §5.5 states that a depth-0 `,` always separates list items, with E412's note. |
| R03 | applied | §4.3's modifier bullet, §4.12's example, §7.5's example and Appendix C.2 all put modifiers before `=`. |
| R04 | applied | §4.12's closing paragraph, §5.13's example and explanation, §7.5's example, C.4 and C.5. An unannotated statement is clipped to the field's existence set. |
| R05 | applied | §9.2's FORMAT paragraph with the five-row invocation table; E805 and E806 message cells updated. Supersedes **B18**. |
| R06 | applied | §6.4's expression bullets, GRAMMAR `derive_value` / `derive_expr`, §6.13's second table. |
| R07 | applied (modified) | §6.5, §6.8 and E519. Merged with **B14**: an operand that can project several values is E519 (a static P3 check, since a path crossing a list is visible in the schema); an operand resolving to *no* value makes the comparison false. Appendix C.2's loop rewritten to `contains`. |
| R08 | applied | §6.9, first bullet. |
| R09 | applied | §6.4, E520 in §10.5, §6.13. |
| R10 | applied | §5.8's first bullet and §7.3 steps 3 and 7. |
| R11 | applied | New §4.4.7.1 "Header probing", verbatim, with the `jpg` canonical-spelling clause folded into **M36**'s canonicalisation rule in §4.4.6. |
| R12 | applied | §5.5 (E444), §5.12's table, §10.4. Preferred over **M28**'s proposal to reuse E429. |
| R13 | applied | §5.4. |
| R14 | applied | §3.2. |
| R15 | applied | §9.3. |
| R16 | applied | §9.8, "Suggestions". |
| R17 | applied | §11.2's `out.*` / `err.txt` bullets. |
| R18 | applied (modified) | §5.6's closing bullet, merged with **B12**'s (a)/(b)/(c) phrasing. The value left by a stray `*` on a non-list enum field is E414 (R18's reading), not E412 (B12's): the value simply falls outside the vocabulary. |
| R19 | applied | §4.10's last paragraph. |
| R20 | applied (modified) | §5.2, merged with **M23** and with **m44**'s "a header tag naming no root field is E409". The nested-field case additionally gets its own identifier, E438 (see §4 below). |
| R21 | applied | §5.7's "Merging" paragraph, merged with **B8**/**B9**. |
| R22 | applied (modified) | §8.9's "Group presence". The wording about `{}` being reachable "only as the `set` object of an overlay entry" is dropped, because overlays no longer carry `set` objects (see §4 below); `{}` is now unreachable anywhere in a conforming document. |
| R23 | applied | §6.11 step 4, and §7.3's recursion paragraph (**M30**). |
| R24 | applied | §6.4. |
| R25 | applied | New Appendix D, and §9.9 now points at it. |
| R26 | applied | §5.9. |
| R27 | applied | §4.4's empty-argument paragraph; E304's example cell. |
| R28 | applied (modified) | §5.5 gains the "at least one column, at least one cell" rule (E416) verbatim. Its second bullet — "a tuple-array statement cannot continue onto another line" — is replaced by **M35**'s bracketed row list, which makes long tables writable; the restriction R28 documented would otherwise be true but unusable. |
| R29 | **rejected** | Superseded by **M39** and by binding decision M. R29 says `--out` must name a path outside the project when the project has no `data/` directory; that makes the same command shape (`--out <project root>/out.json`) legal in one layout and illegal in the other, which is exactly the defect M39 reports. §9.4 now refuses a destination that is a source file, that lies inside the data directory, or that is an `.ab`/`.abt` file inside the discovery root — which keeps decision M's "never write inside `data/`" and makes `<project root>/out.json` always legal. |
| R30 | applied | §11.3's four new bullets, with `--max-errors` added to the CLI bullet and the negative-logic range extended to E520. |
| R31 | applied | A66–A70 in §A.1 and A71 in §A.3. |
| R32 | applied (modified) | Every `[DECISION-PENDING]` tag and every sentence addressing the decisions document is removed from §1.4, §4.11, §7.4, §9.2 and A.35 — **except** §6.8's, which is a genuinely open language question and which the task instruction directs to keep. §1.4's convention is rewritten so the tag names the alternative instead of referring to an external document, and states that exactly one rule carries it. See "Pending decisions". |
| R33 | applied | §5.11's last three bullets. |
| R34 | applied | GRAMMAR §2, comment under `path`. |
| R35 | applied | §9.2's `init` paragraph. |
| R36 | applied | §4.8's last bullet. |
| R37 | applied | §5.12, after the table. |
| R38 | applied | E309's message cell. |
| R39 | applied | §4.12's last paragraph. |

## 2. Implementability review — `B1`–`B18`

| Item | Verdict | Where it landed / why |
|---|---|---|
| B1 | applied (modified) | Merged into **R01**. B1's "`{` and `}` are tracked for balance only" is adopted; R01's multi-path exception is kept, disambiguated lexically (a `{` immediately preceded by `.`). Continuation rule (b) is adopted as B1 wrote it. |
| B2 | applied | GRAMMAR `bare_text` may not begin with `[`; §5.5 states the rule. |
| B3 | applied | GRAMMAR `bare_text` excludes a trailing `annotation_list`; §5.13's first bullet states it with the `note: deprecated @removed(3)` example. |
| B4 | applied | Same edit as **R03**. |
| B5 | applied | Same edit as **R02**. |
| B6 | applied | §6.6's table levels 3 and 4 swapped, plus the `not` bullet. |
| B7 | applied | §5.5 ("an element that begins with `[` is E441"); the misleading GRAMMAR comment is replaced. |
| B8 | applied | §7.3 step 1 (clones, then header tags, then body statements) and §5.2's closing bullet. |
| B9 | applied | §5.7's "Merging": the instance's own statements are plain assignment, never a merge. |
| B10 | applied | Same edit as **R04**. |
| B11 | applied (modified) | Same site as **R06**; R06's disambiguation rule (first token `.`, loop variable, or `length`) and E519 are used instead of B11's E513. |
| B12 | applied (modified) | Merged into **R18**. |
| B13 | applied | §5.10's "Interpretation" paragraph; §7.3 steps 3 and 7 renamed and rewritten. |
| B14 | applied (modified) | §6.8's "Absent operands" bullet. B14's blanket "every operator is existential" is not adopted, because decision J forbids existential `==`/`!=`; absence yields false, plurality is E519. |
| B15 | applied | §6.10: absence yields `0`; E514 is a static P3 check on the declared type. §7.2's P3 list matches. |
| B16 | applied (modified) | §6.5's loop-variable bullet, in a shortened form; §7.2's P3 list refers to it. |
| B17 | applied | §2.3 step 2 (ancestors, then immediate children) and the paragraph under the layout diagram. A tie among children is broken by name order, which B17 left open. |
| B18 | applied (modified) | Superseded by **R05**, which keeps E805 reachable (a non-keyword, non-path last positional alongside another positional) instead of deleting it or inventing `--format=`. |

## 3. Implementability review — `M19`–`M40`, `m41`–`m49`

| Item | Verdict | Where it landed / why |
|---|---|---|
| M19 | applied | GRAMMAR `size_token` and `ws`; §4.4.7's first paragraph. |
| M20 | applied | §3.1 "Adjacency"; GRAMMAR L9; `?` added to §3.1's punctuation list and to B.3. |
| M21 | applied | §3.1 "Longest match"; GRAMMAR L8. |
| M22 | applied | §3.5's "one token" paragraph and §3.6's first paragraph; GRAMMAR L5/L6. |
| M23 | applied | Merged into **R20**. |
| M24 | applied | §4.10's interpolation paragraph; §7.3 steps 2 and 4. |
| M25 | applied | §5.11's first bullet. The worked chain example is not reproduced; the rule states the outcome in one sentence, which is what a normative document needs. |
| M26 | applied | §5.11's "per version" bullet. |
| M27 | applied | §5.7's partial-clone bullet; §5.13's last bullet; E408's message and note. |
| M28 | applied (modified) | §5.7 gains "the merge is schema-directed" and "wildcards are expanded before folding". The duplicate-key case uses **R12**'s dedicated E444 rather than M28's E429, so that the two conditions keep separate identifiers. |
| M29 | applied | §4.7's "Presence" paragraph and §7.3 step 4. |
| M30 | applied | §7.3's recursion paragraph. |
| M31 | applied | §4.12's "`$(Schema)` fields" paragraph. |
| M32 | applied (modified) | §11.1's "Precedence" paragraph. E445-before-E411 is added; M32's `n: int(1..10) = 999 @optional` example is no longer expressible after **R03**, so E320-before-E313 is kept for the legal spelling. §3.5 states E211's precedence. |
| M33 | applied | §9.8's "Substitutions". |
| M34 | applied | Continuation rule (b) in §3.6; §6.3's second bullet. |
| M35 | applied | GRAMMAR `tuple_array_assignment`; §5.5's bracketed-rows bullet and example. |
| M36 | applied | §4.4.6's canonicalisation paragraph; §4.4.7 and B.4 refer to it. |
| M37 | applied | §7.2's E321 bullet and §3.7's third limit row. |
| M38 | applied | §4.12's "never removed and re-added" paragraph; §4.3's first bullet; E302's note. |
| M39 | applied (modified) | §9.4's destination bullets. Decision M requires refusing a destination inside `data/`, which M39's rule alone would allow; the combined rule keeps both and removes the layout-dependent verdict M39 objected to. |
| M40 | applied | §4.12's "Every version must compile" paragraph, with the `exists` guard example; E411's note. |
| m41 | applied (modified) | See **R32**: all tags removed except §6.8's. |
| m42 | applied | §4.13 gains E317, E321 and (new) E323. |
| m43 | applied | §5.7's "Identity is never cloned": `&other.id` is E410. |
| m44 | applied | §5.5 (tuple column and `#tag` argument key are E409) and §5.2 (header tag name is E409). |
| m45 | applied | §5.5's paragraph on values beginning with `#` or `"`, with E412's note. |
| m46 | applied | §6.2's literal-list paragraph. |
| m47 | applied | §6.9's last bullet; §6.9's E510 bullet narrowed to operands and path segments so the two do not contradict. |
| m48 | applied | §8.5 (`- {}` / `- []`) and §4.9 (RAW numbered keys). |
| m49 | applied | §7.5 step 1 (`0.0` and `-0.0` differ) and §5.3 rule 2 (final extension, checked only when some instance omits `@id`). |

## 4. Changes beyond the two reviews

Eight of these restore constructs the binding decisions require and the reviewed draft did not contain;
five repair defects found while re-reading the whole document.

| # | Change | Why |
|---|---|---|
| X1 | §8.1 rewritten: the envelope is `{ "abstract": { "format", "compiler", "versions" }, "data", "overlays" }`. Every JSON, YAML and RAW example follows, and A46 records the migration. | Decision L specifies this envelope; the draft emitted `"abstract": "1.0"` and a top-level `versions`. |
| X2 | §7.5 rewritten: an overlay entry is a **complete instance object**, not a `{id, set, remove}` delta. §2.7, §8.9, Appendix B.2, C.6, C.7 and §11.3 follow. | Decision L: "full materialised instance objects … No field-level merge". The draft also contradicted its own §2.7, which orders `overlays[].data` by `(template, id)` — impossible for entries that carry no `template`. |
| X3 | List cardinality: `tags[2..]`, `tags[2..4]` (§4.3, §4.6, GRAMMAR `cardinality`), with E323 (malformed) and E445 (violated). | Decision I and "Other new features" item 2; absent from the draft. |
| X4 | Body blocks in instances (§5.4, GRAMMAR `body_block`, `body_item`), including their interaction with §3.6, E404, E429 and E437. | Decision J and "Other new features" item 3; absent from the draft. |
| X5 | `#tag` arguments accept dotted keys and bracketed lists (§5.5, GRAMMAR `tag_arg_key`, `tag_arg_value`). | Decision J ("`#tag(...)` arguments accept dotted paths, nested `#tag`, and lists"). |
| X6 | `--max-errors <n>`, default 20 (§9.2, §9.3, §11.1, §11.3) with E812 for an invalid flag value. | Decision B and "Other new features" item 4; absent from the draft. |
| X7 | E438: a header tag whose target is a group, list group or `$(Schema)` field. | Decision J requires that a header tag addressing a nested field be an error naming the construct; the draft left it as a type mismatch. |
| X8 | E439: a `::` token outside an instance header. | Decision A requires a dedicated error naming the text on the left of a top-level `::`. |
| X9 | An empty right-hand side is E210 (§5.4). | Decision H ("empty right-hand side = error"); the draft had no rule. |
| X10 | Every logic diagnostic carries the instance and the version; inside a `for` body it also carries the index and the item (§6.12, E515). | Decision K. |
| X11 | Versions are compiled in ascending order (§7.4), and §11.2 normalises `abstract.compiler` to `0.0.0` before comparing goldens; §7.6 says the compiler string is the only byte not fixed by this document. | Consistency: without an order, "the first failing `require`" is undefined across versions; without the normalisation, X1's `compiler` key makes every golden case implementation-specific. |
| X12 | Consistency repairs: §5.10's "Interpretation" paragraph moved below the syntax-value table; §6.4's opening sentence says *expression*; §6.9's E510 bullet narrowed so it no longer contradicts the `$name`-in-`derive` rule; §6.13 gains a lead-in for its second table; §5.1 and §5.4 cite the new grammar rules. | End-to-end re-read. |
| X13 | Example and message repairs: E320's example (`n: int @optional = 1`, since a modifier after a default is now E303), E804's example (`templates a b`, since `compile a b JSON` is E807 under the new precedence), E808's three-way message, A49 (`- {}`), B.4's extension row, §11.3's `E501–E520` dash. | End-to-end re-read. |

Verification performed after the rewrite: every `Exxx` used in the prose appears exactly once as a
row in §10 (119 rows, no duplicates, none orphaned); every `§n.m` cross-reference resolves to a
heading that exists; every grammar rule cited in `SPEC.md` is defined in `GRAMMAR.ebnf`; no
`set`/`remove` overlay vocabulary, no `"abstract": "1.0"` and no `dimension_spec` survives.

---

## Pending decisions

Two questions remain genuinely open. The first is marked `[DECISION-PENDING]` in the specification,
as §1.4 describes; the second is a consequence of a binding decision and needs confirmation rather
than a rule change.

**P1 — String comparison in logic (§6.8).** The binding decisions do not settle it.

- *As specified:* `==`, `!=` and `contains` compare strings **exactly**, as sequences of Unicode
  scalar values. Case and `-`/`_` differences matter. Enum values are stored normalised, so
  `.status == "active"` is right and `"Active"` never matches.
- *Alternative:* normalise both operands before comparing, as 0.2.0 did (audit row A35). `"Active"`
  would then match an enum value `active`, and a `text` field would stop comparing as its own bytes.
- *At stake:* every existing project that compares text with `==`. The exact rule is the stricter of
  the two and is the one that keeps `text` honest; the normalising rule is the one that keeps 0.2.0
  projects compiling unchanged.

**P2 — `abstract.compiler` in the envelope (§8.1) versus byte-exact goldens (§11.2).** Decision L
puts `"compiler": "1.0.0"` in the document. It is the only byte of a compiled document that is not a
function of the source bytes, so a golden suite that compares stdout byte for byte is otherwise
bound to one release.

- *As specified:* the key is emitted, and §11.2 normalises its value to `0.0.0` in both the expected
  and the actual stream before comparing. This is the only normalisation besides `CRLF` → `LF`.
- *Alternative A:* drop `compiler` from the envelope; consumers that need it read it from the
  producer, not from the document.
- *Alternative B:* make it a constant naming the **language** version (`"1.0"`), which is a function
  of the specification rather than of the build, and drop the normalisation.

---

## Contradictions in the binding decisions (reported, not resolved)

Carried forward from `review-audit-coverage.md` §5, plus one found in this pass. None of them blocked
the revision; each is recorded with the reading the specification follows.

| # | Decision | The contradiction | Reading followed |
|---|---|---|---|
| D1 | L (output) | "format keyword required (default JSON if absent is fine but a wrong keyword is an error)" — required and optional in one sentence. | Optional, JSON default, wrong keyword is E805 (§9.2, via R05). |
| D2 | K (ordering) | Instance order is answered three times: source-path order, then `(template, id)`, then "Decision: sort by template then id." | The last answer (§2.7). |
| D3 | A (continuation) | A blanket prohibition on trailing-comma continuation and its one exception are stated in the same clause. | The refined form: header tag lists continue, body statements do not (§3.6, §5.2). |
| D4 | M vs §9.9 | Decision M lists fourteen concrete fixes for `bundle`, the Java runtime and the editor extension; the specification declares all of them outside the language. | Both: they are requirements on the reference implementation, recorded in Appendix D, not language rules. |
| D5 | Versions / overlays (**new**) | "For each instance **and each version** `v < max` whose materialised object differs from the base, group consecutive versions with identical objects into one range" reads as per-instance grouping, which produces overlapping ranges; the next two clauses require that "ranges are disjoint" and that a consumer use "the single overlay whose range contains `v`". | Document-level grouping: consecutive versions whose **whole set** of changed objects is identical form one range (§7.5 step 2). It is the only reading under which the ranges are disjoint. |

---

# Revision 3–4

Scope: the "Revision 3 addendum" (`R3-1`–`R3-13`) and the "Revision 4 addendum" (`R4-1`–`R4-8`) of the
orchestrator's decisions file, applied to `SPEC.md` and `GRAMMAR.ebnf` after the revision recorded
above. Every item is binding and overrides earlier text where they differ.

Counts: **21 items** — **13 applied** (new or changed normative text), **7 verified as already
satisfied** (R3-2, R3-5, R3-6, R3-9, R3-10, R3-11, R3-12 — the last with one clarifying sentence), and
**1 no-change** (R4-2, which confirms the existing rule). R3-13 (re-derive every example) is recorded
separately in §3 below.

## 1. Revision 3 addendum

| Item | Verdict | Where it landed |
|---|---|---|
| R3-1 | applied | §4.11 rewritten: `template` is never declarable; `id` is implicitly `id: text(1..64)` and MAY be declared at root level as `id: text` or `id: text(a..b)` with no list head, no modifier and no default — anything else is E314. An id outside the applicable range is E413 (§5.3, §7.2 P2). §8.2 and §8.3 say the id is still emitted once, in envelope position, and that a declared `id` field is skipped in the field pass. §4.13's row, E314 and E413 in §10.3/§10.4, A7, A.5 item 1, B.1 and §11.3 follow. §4.11 also settles a contradiction the earlier draft carried: inside a group, `id` and `template` are ordinary field names (which is what §4.8's `@tag` example has always used), and §5.12's "declared fields" note is reworded accordingly. GRAMMAR: constraint comment above `scalar_field`. |
| R3-2 | satisfied | §6.8 already required exact comparison; R4-1 removed the tag that marked it provisional. |
| R3-3 | applied | New **§5.14 Instance version windows**: a header MAY end, after all tags, with `@since(n)` / `@removed(n)`; the window is validated like a field's existence set (E603, E604); an instance is compiled only for the versions in its window. §5.1's bullet, §5.2 (tag-versus-annotation disambiguation, E437 reworded), §5.13 (the applicability set now intersects the window; E430 versus E440), §5.7 (a clone source's window MUST contain the cloning instance's, E440; E408 rewritten), §4.4.8 (a `ref` target MUST exist in the version being compiled, E431), §7.2 P2, §7.3's preamble and step 3, §7.4, §7.5, §8.1 (overlay key table), §10.4 (**new E440**, plus E408, E431, E437), §11.3, Appendix C, and GRAMMAR (`instance_header` gains `[ annotation_list ]`, plus the L9 adjacency note and the `annotation_list` comment). Overlays gained `removed` (see R4-3). |
| R3-4 | applied (discovery); the rest already satisfied | §2.3 steps 2–5 rewritten: the marker walk goes **upwards only**; B17's "immediate children" search and its sort-order tie-break are gone. A command-line directory MUST be the project root or the data directory (step 5); a directory inside a data directory is E806, as is a directory with more than one `data` child. The paragraph under the layout diagram, §9.2's `compile` paragraph and E806's row follow. The other three clauses of R3-4 were already in the document: `derive?` on a defaulted field is E518 (§6.4), `--allow-unknown` does not exist (§5.12, §9.3, A52), and the three-argument direct form is removed (§9.1, A53). |
| R3-5 | satisfied | §3.6's continuation rule (b) and §6.3: a line terminator before `else` inside a logic block does not end the logical line, and there is no other continuation except inside open value brackets. |
| R3-6 | satisfied | §5.7 "Merging": the keyed-list merge table governs clone-to-clone folding only; the instance's own header tags and body statements are plain assignment and replace a cloned list entirely. |
| R3-7 | applied | GRAMMAR: `derive_value` is folded into `derive_expr`, which now carries all five alternatives — `logic_path`, `length_call`, `version_builtin`, `loop_variable`, `value` (a literal or interpolated text) — with the disambiguation rule in its comment; `derive_statement` uses it. §6.4 gains the `Grammar:` line, the five-form list, and "the type follows the source" for a `.path`. |
| R3-8 | applied | §4.12, "`$(Schema)` fields": a new sentence states that both windows apply and that a nested value exists only in the intersection of the two existence sets. |
| R3-9 | satisfied | §7.2 P3: E321 only when **every** edge of the cycle is a required, non-list `$(Schema)` field; a cycle through an `@optional` or list field is legal and is bounded by §3.7. |
| R3-10 | satisfied | §6.5 and E519: `==`, `!=`, `<`, `<=`, `>`, `>=` require operands that resolve to at most one value; a path crossing a list is E519, naming the path, as a static P3 check. |
| R3-11 | satisfied | §3.6: the value-bracket stack holds `(`, `[` and the multi-path `{` only; every block brace is matched for balance but joins no lines, so schema, group and logic blocks stay line-oriented. GRAMMAR L5 says the same. |
| R3-12 | satisfied (+1 sentence) | §11.2 already expressed "exit 0, empty stdout" as `exit` = `0` with no `out.*` file; a bullet now says so explicitly and requires the runner to check both. |
| R3-13 | applied | See §3 below. |

## 2. Revision 4 addendum

| Item | Verdict | Where it landed |
|---|---|---|
| R4-1 | applied | The `[DECISION-PENDING]` tag and its alternative are removed from §6.8 (the exact-comparison rule is unchanged, and now also states that a mismatched case is false rather than an error), and §1.4's convention bullet describing the tag is deleted. No tag remains in the document. In its place §1.4 gains the convention that a partial example assumes the rest of its project is declared elsewhere — which is what makes the fragment examples of §4.4.8, §5.5, §5.8, §5.11 and §6.2 legal as written. |
| R4-2 | no change | `abstract.compiler` stays in the envelope (§8.1) and §11.2 normalises it to `0.0.0` before comparing goldens. |
| R4-3 | applied | §7.5 rewritten around **per-instance** reduction: structural equality with `⊥` for "absent in this version"; step 1 splits `lo..hi-1` into maximal runs per instance and emits **replace** or **remove** entries; step 2 groups entries by range, one overlay per distinct range, `data` ordered by `(template, id)` and `removed` ascending by id, overlays sorted by `(min, max, first id)`; overlay ranges MAY overlap, but the ranges mentioning one id are disjoint; step 3 makes a consumer apply **every** overlay whose range contains `v`, replacing or adding by id and deleting the ids in `removed`. §7.5's worked example is re-derived, §8.1 documents the three overlay keys, A.5 item 10 and Appendix D item 11 (the Java-consumer note) are rewritten, and §11.3 asks for overlapping ranges and a non-empty `removed`. This resolves contradiction **D5** of the previous revision: document-level grouping is gone. |
| R4-4 | applied | §7.4's "Every instance exists in every version" is replaced by the window rule, and the two facts that depended on it ("`template` and `id` are identical in every `D(v)`", "the number of objects … identical") are restated correctly. §7.1's P4 line follows. |
| R4-5 | applied | The built-in **`version`** is added to the logic language: §6.6 gains it in the precedence table (level 1) and a labelled paragraph giving its spelling, type, scope and determinism; §6.4 lists it among the five `derive_expr` forms; §6.13's second table gains a row; B.3 lists it as a logic-expression keyword; GRAMMAR gains `version_builtin` and uses it in `operand` and `derive_expr`. The "silently skipped" rule for a `derive` to a non-existent field is **deleted** (§6.4, §6.13) and replaced by **new E521**, raised only when the statement executes; §4.12's "Every version must compile" paragraph gains the write-guard example, and §11.3 asks for a case. |
| R4-6 | applied | Part of R3-1. A7 now reads "keep it or delete it; ranges are honoured", and A.5 item 1 matches. |
| R4-7 | applied | Part of R3-4: the walk is upward-only for every root, including a single `.ab` file, and `abstract compile <dir>` accepts the project root (resolved by the one-child test of step 3) or the data directory only. |
| R4-8 | applied | **E440** (reserved, unused) now carries R3-3's condition — a statement or a clone that falls outside its instance's window. R4-5's error needed the logic family, so it took the next free identifier there, **E521**; **E809 stays reserved and unused**. |

## 3. R3-13 — examples re-derived by hand

Every fenced example in the document was re-derived against the revised rules. Findings and fixes:

| # | Example | Finding | Fix |
|---|---|---|---|
| E1 | §5.13's `Item :: @id.torch` block | The block opened with `versions 1..3`, a `.abt` construct, inside what the rest of the block makes an instance file — E210 as written | The declaration line is dropped; a lead-in sentence imports the schema and the range from §4.12 |
| E2 | §5.14's new example | The same hazard, avoided | Written as one instance file, with the schema and range imported by prose |
| E3 | §4.8's `capabilities[] { id: … @tag }`, and §5.5, §5.7, §8.6 | Contradicted §4.11's old "no field named `id`, at root level **or inside any group**" | §4.11 now restricts the rule to root level, which is where the envelope exists; the examples stand unchanged |
| E4 | §7.5's worked example | The narration referred to document-level `changed(v)` lists | Re-derived per instance: `O(torch, 1)` ≠ `O(torch, 2)`, two runs, two overlays; both overlay objects gain `"removed": []` |
| E5 | Appendix C | Needed the `removed` key, an instance window to exercise it, and an explicit `id` declaration | `schema Pack` gains `id: text(3..24)`; a second pack `spring_2026` (`Pack :: @since(2)`, id from the file stem) is added to C.1 and C.3; C.6's JSON gains its object in `data` (first, because `spring_2026` sorts before `winter_2026`) and `"removed": ["spring_2026"]` in the single overlay; C.7's YAML matches; C.6's reading paragraph is re-derived (three entries, one shared range `1..1`, one overlay) |
| E6 | Appendix C.8 | The diagnostic position moved when `Pack.abt` gained a line | `31:9` → `32:9`, checked by counting the block |
| E7 | §4.12's write-guard example | An `if version >= 2 { derive .glow = true }` next to the `require .glow == false` read-guard fragment read as one contradictory block | The write-guard example targets `legacy_tint` (`if version < 3 { derive? .legacy_tint = 0 }`), which also demonstrates a `@removed` window |

Everything else was re-derived and found correct: §4.4.1 (full envelope, `versions 1..1`, `overlays: []`),
§4.4.5, §4.4.8, §4.4.9 (key order follows `schema Owner`), §4.8, §5.5, §5.6, §5.8, §5.11, §6.2, §8.4,
§8.5, §8.6, and the whole of Appendix C (ordering by `(template, id)`; frost's `core_*` expanding to
`core_ui, core_game`; `es_*` expanding one tuple row into two; ember's clone carrying `tint` in version
1 only; `derive? .owner.contact` firing for both stickers; both `require`s passing).

Mechanical checks run over the final files:

- every `Exxx` in the prose is a row of §10 and every row of §10 is used elsewhere — **121 rows, no duplicates, no orphans**; the families are contiguous except the still-reserved E809;
- every `§n.m` reference resolves to a heading that exists (0 unresolved);
- every rule name in a `Grammar:` line of `SPEC.md` is defined in `GRAMMAR.ebnf`;
- every JSON example parses (whole documents directly, fragments when wrapped in braces) and every YAML example parses;
- cross-format equivalence: §8.4 equals §8.5, and Appendix C.6 equals C.7, as data;
- no `abstract` example block mixes `.abt` and `.ab` constructs;
- no `[DECISION-PENDING]` tag, no `derive_value` rule, and no "silently skipped" version rule survives.

## 4. New and changed error identifiers

| ID | Status | Condition |
|---|---|---|
| E440 | **new** (was reserved) | A body statement whose applicability set falls outside its instance's version window, or a clone whose source does not exist wherever the cloning instance does |
| E521 | **new** | A `derive` / `derive?` that executes against a field that does not exist in the version being compiled |
| E314 | changed | Now: a field named `template`, or an `id` declaration that is not `id: text` / `id: text(a..b)` with no list head, modifier or default. A second message template is added |
| E413 | changed | Extended to an instance id outside the declared or implicit `id` range |
| E431 | changed | A second message template: a `ref` target that exists but not in the version being compiled |
| E437 | changed | The message now reads "on a body statement or at the end of an instance header" |
| E408 | changed | "in every version" is now "in every version in which the cloning instance exists" |
| E806 | changed | Two message templates added: a named directory inside the data directory, and a directory with more than one `data` child |
| E809 | unchanged | Still reserved and unused |

## 5. Superseded entries of the previous revision

- **Pending decision P1** (string comparison) is closed by R4-1: exact comparison, tag removed.
- **Pending decision P2** (`abstract.compiler`) is closed by R4-2: no change.
- **Contradiction D5** (overlay grouping) is closed by R4-3: grouping is per instance; ranges may
  overlap between instances and are disjoint per id.
- **B17** (the "immediate children" search in §2.3) is withdrawn by R3-4 / R4-7; the upward-only walk
  plus the single project-root child test replaces it.

## Open

None. Every item of both addenda is applied or verified as already satisfied, and no contradiction
between the addenda and the rest of the specification was left unresolved.

---

# Revision 6

Scope: the "Revision 6" rulings (`R6-1`–`R6-16`) of the decisions file, applied to `SPEC.md` and
`GRAMMAR.ebnf` after the two adversarial passes (`DEFECTS-1.md`, `DEFECTS-2.md`), the defect-fix
stage (`CORRECTIONS.md`) and the whole-corpus run (`SPEC-ISSUES-conformance.md`). Every item is
binding. This is the freeze revision: it closes the open specification questions those stages left,
so that no diagnostic in the language is decided by an implementation choice a second implementation
could not derive from the text.

Counts: **16 rulings** — **14 changed normative text**, **1 is a build requirement with no spec text**
(R6-14) and **1 is corpus hygiene** (R6-15). Ten of the fourteen also changed the compiler.

## 1. The rulings and where they landed

| Item | Ruling | Where it landed |
|---|---|---|
| R6-1 | A project range covers at most 4096 versions; E602 names the bound | §3.7 gains the row and the sentence that it is the one limit reported as E602 rather than E209; §4.12 gains the bullet; §10.6's E602 row gains a second message template; `GRAMMAR.ebnf` notes it on `versions_decl` |
| R6-2 | Braces in a bare value MUST balance, whatever the field type; E435 is for pattern-level faults only, and a pattern holds at most 8 groups | §3.6 gains the "`{` as a token" paragraph, the balance rule and its example; §5.8 gains the eight-group bullet and the table separating E203/E205 from E435 and E434; §10.4's E435 row is rewritten; `GRAMMAR.ebnf` L5 and `bare_text` carry both halves |
| R6-3 | A header tag is an assignment: E603/E604 on a statement's own annotations, and an empty applicability set is E440 rather than silence | §5.2 gains the applicability bullet; §5.13 gains the E603/E604 bullet and the header-tag sentence; §7.2's P2 list now says "every assignment"; §5.12 and §10.4's E440 row name the header tag; §11.1 orders E603 **and E604** before E430 and E440 |
| R6-4 | E422's `{actual}` is `unreadable` when the signature matched but the header is unusable | §10.4 gains a paragraph after the E4xx table, with the rendered example |
| R6-5 | Instance depth is counted from the instance object as level 1, object or list = one level, checked once in validation with a position | §3.7 renames the row and gains the counting paragraph and the single-site sentence; §7.2's recursion sentence follows the rename; §7.3 gains the enforcement sentence after step 8 |
| R6-6 | `--max-errors` takes 1..10000; anything else is E812 | §9.3's flag table and its bullet; §10.8's E812 row |
| R6-7 | Bare text admits every scalar value except C0, `U+007F` and C1; non-ASCII whitespace is literal | §3.1's whitespace sentence is scoped to "between tokens"; §3.5 gains the character-set block; §10.2's E210 row cites it; `GRAMMAR.ebnf` L4 and `bare_text` carry it |
| R6-8 | Output escapes C0, `U+007F` and C1, in all three formats | §8.8's table row, and the paragraph naming the set as §3.5's exclusion set |
| R6-9 | An identifier begins with `A-Z a-z 0-9 _`; a leading `-` is E210 | §3.3 gains the three-rule table; §10.2's E210 row gains the example; `GRAMMAR.ebnf` notes it on `identifier` |
| R6-10 | A bare value cannot begin with `//`, wherever it begins | §3.2 states it as a rule of its own, with the `cdn://…` example; §10.2's E210 row gains it; `GRAMMAR.ebnf`'s `bare_text` says where it holds |
| R6-11 | The CLI accepts Windows extended-length input paths and strips the prefix | §9.1 gains the "Extended-length arguments" paragraph; §9.8 says the prefix never reaches a diagnostic |
| R6-12 | E412's comma note is for the bare comma spelling only | Implementation-level: the note is not catalogued, so §10.4 is unchanged; the AST now records the spelling |
| R6-13 | Interpolating a non-scalar is E522 | §6.9 gains the bullet and its three-line example; §6.13's right-hand-side table gains a row; §10.5 gains the E522 row; §11.3's negative-logic coverage reads E501–E522 |
| R6-14 | `cargo clippy --all-targets` clean under `-D warnings` | No spec text; the eight lints are fixed in the crate |
| R6-15 | Corpus hygiene | No spec text; recorded in `conformance/INDEX.md` |
| R6-16 | The six `SPEC-ISSUES-conformance.md` wording suggestions | §7.1 ("P1 is one phase"), §7.2 (E428 in the `instances` bullet), §7.3 step 1 (E410), §10.3 (the E316 row), §3.6 (the "`{` as a token" sentence), §11.2 (parser recovery is not part of the golden contract), Appendix D.1 (a `bundle` usage error carries no chapter 10 identifier) |

## 2. Examples re-derived by hand

Every example the amendments touch was re-derived against the amended rules.

| # | Example | Derivation |
|---|---|---|
| E1 | §3.2's `cdn://cdn.example.com/x.png` | New. The `//` follows the `:` with no space, so no comment rule fires and the old reading made it the value `//cdn.example.com/x.png`; under R6-10 it is E210 at the `//`. The two quoted lines above it are unchanged and remain the remedy |
| E2 | §3.6's `icon: a}.{b` and `label: "a}.{b"` | New. In the bare run the `}` closes nothing (E205 at the `}`) and the `{` is left open when the run ends (E203 at the `{`); quoting makes both ordinary characters. Matches `lexing-parsing/lp-006` |
| E3 | §3.3's three-rule table | New. `-name: 1` and `@-name.x` are E210 at the `-`; `name-: 1` is E207, naming the whole run. `TAMAÑO: 3` stays E206, which is §10.2's own example for that row |
| E4 | §6.9's `derive .label = cap-$c` block | New, against `schema Card { name, label, caps[] { id: enum(...) @tag } }`. `for $c in .caps` binds an object, so the interpolation is E522; `throw "bad $c"` is E522 for the same reason; `derive .label = $c.id` is a `loop_path` (§6.5), the first `derive_expr` form of §6.4, and resolves to the element's scalar. The fourth form — `$c` as the whole right-hand side — keeps its own type and is E412 against a `text` field, which the paragraph states so that the two are not confused. Compiled as `adversarial-2/adv2-10` |
| E5 | §10.4's E422 rendering | New. `frost.icon` is the `{context}.{field}` of §9.8; `{actual}` is `unreadable`, `{expected}` is `bmp`, and the note is the one §4.4.7.1 prescribes for a malformed BMP width |
| E6 | §5.2's `Item :: @id.x, @glow @removed(2)` | New. `glow` is `@since(2)` in a `1..3` project, so its existence set is 2..3; the instance window is 1..1; the intersection is empty, and because a tag carries no annotation of its own the empty set can only come from the window, which makes it E440 and not E430. Compiled as `adversarial-2/adv2-09` |
| E7 | §5.13's `glow: true @since(3) @removed(2)` | New. Both numbers lie inside `1..3`, so E603 does not fire; `2 <= 3` makes it E604, checked before the applicability set is formed. Compiled as `adversarial-2/adv2-11` |
| E8 | §3.7's depth sentence ("61 `child` objects is 62 levels") | Re-derived: the instance object is level 1 and each `child` object adds one, so 61 objects reach 62 and compile; 63 objects plus a `tags` list reach 65 and are E209, while the same path without the list reaches 64 and compiles. All three halves compiled, as `adversarial-1/adv-21-document-depth-limit` and `adv-21b-instance-depth-over-limit` |

Nothing else moved. No existing example uses a value beginning with `//`, an unbalanced brace, a
control character, a pattern with more than eight groups, `--max-errors 0`, or a tree deeper than 62
levels, so §4.4.1, §4.8, §5.5, §5.6, §5.8, §5.11, §6.2, §7.5, §8.4, §8.5, §8.6 and the whole of
Appendix C are unaffected, and each was re-read to confirm it.

Mechanical checks run over the final files:

- every `Exxx` in the prose is a row of §10 and every row of §10 is used elsewhere — **122 rows, no
  duplicates, no orphans**; the families are contiguous except the still-reserved E809;
- every `§n.m` reference resolves to a heading that exists (87 distinct references, 0 unresolved);
- every rule name in a `Grammar:` line of `SPEC.md` is defined in `GRAMMAR.ebnf` (63 names, 0 undefined);
- every JSON example parses (13 blocks; the one shape sketch with an explicit elision is exempt);
- no `abstract` example block mixes `.abt` and `.ab` constructs.

## 3. New and changed error identifiers

| ID | Status | Condition |
|---|---|---|
| E522 | **new** | An interpolated `$name` in a `derive` value or a `throw` message is bound to a value that is not a scalar |
| E210 | changed | Now also: an identifier that begins with `-` (R6-9), a bare value that begins with `//` (R6-10), and a C0, `U+007F` or C1 control character inside a value (R6-7). The last widens the old rule, which refused C0 and `U+007F` but accepted C1 |
| E206 | changed | Narrowed: no longer reported for a leading `-`, which is an identifier character and could never be the character its own message called invalid |
| E209 | changed | The document-depth row is now the instance-depth row, counted and positioned as §3.7 states, and checked in validation only |
| E602 | changed | A second message template: a range wider than 4096 versions |
| E604 | changed | Now stated for a body statement as well as for an instance header and a field |
| E440 | changed | Now stated for a header tag as well as for a body statement and a clone |
| E435 | changed | Reserved for pattern-level faults, with a third one: more than eight groups |
| E812 | changed | Now also a `--max-errors` outside 1..10000 |
| E809 | unchanged | Still reserved and unused |

## Open

Five specification issues recorded in `DEFECTS-1.md` fall outside `R6-1`–`R6-16` and are **not**
resolved here. None of them changes a diagnostic identifier the corpus pins, and each needs a ruling
rather than an editorial fix:

- **S1** — §8.1's "an empty `data` array is only reachable in single-file mode" omits the second
  case, a whole-project compile of a project with `.abt` files and no instances, which §7.2 blesses
  explicitly.
- **S4** — §6.6's "an unbalanced parenthesis is E511" is unreachable: §3.6 makes an unmatched `)`
  E205 and an unclosed `(` E203, and §11.1's precedence rules select those.
- **S5** — §3.6 rule (b) does not say whether a blank or comment line may sit between `}` and `else`.
  The implementation accepts it; the strict reading of the rule makes it E517.
- **S7** — nothing bounds the work a legal program can demand. §3.7's limits are about stack depth,
  and 63 nested `for` blocks over two-element lists sits inside every one of them.
- **S8** — §9.8's `{value}` does not describe what E413 substitutes for a `text` range violation
  (the scalar count) or what `{ranges}` renders for `text(2..2)`.


---

# Revision 7

Scope: the five specification issues the Revision 6 section left **Open** — `S1`, `S4`, `S5`, `S7`
and `S8` — each settled by a binding ruling and applied to `SPEC.md`, `GRAMMAR.ebnf`, the compiler,
the tests and the conformance corpus. Shipped as **1.0.1**.

Counts: **5 rulings** — **5 changed normative text**, **1 changed the compiler's behaviour** (S7),
**1 changed a documentation page's wording only** in addition to the specification (S4, the
reference site's E511 note). Four of the five make the specification say what the compiler already
did; S7 adds a limit, an identifier and the code that enforces it.

## 1. The rulings and where they landed

| Item | Ruling | Where it landed |
|---|---|---|
| S1 | A project whose sources declare schemas but no instances compiles to an **empty `data` array** and lints ok; a project with **no source files at all** is the discovery error E103. Both are stated. | §8.1's closing paragraph is replaced by the two-way list and the E103 sentence; §7.2 and §2.4 already said their halves and are unchanged |
| S4 | E511's unbalanced-parenthesis condition is **unreachable** and is removed. The reachable reasons are the two the implementation reports: a token that cannot begin an operand, and a token that is not an operator where one is due. E511 stays in the catalogue. | §6.6's single bullet becomes two — the two reachable reasons, and the sentence that a parenthesis fault is E203/E204/E205 from the lexer; §10.5's E511 row names the two reasons and gains a second example; `site/reference/diagnostics.html`'s E511 note is corrected |
| S5 | Blank lines and comment lines **MAY** sit between a closing brace and `else` / `else if`, and between a `require` condition and its `else throw`. | §3.6 rule (b) gains the "next **token**, not next physical line" sentence and the note that the rule does not ask what precedes; §6.2 gains the continuation sentence; §6.3 gains the bullet, the corrected E517 bullet and a worked example; `GRAMMAR.ebnf` L5 (b) and the `block` note carry both halves |
| S7 | The total number of loop iterations executed by the logic of one instance for one version **MUST NOT exceed 1 000 000**; exceeding it is the new **E523**. | §3.7 gains the row, the reworked "not E209" sentence and the "How logic work is counted" paragraph; §6.2 gains the bound; §7.3 step 5 names the charge; §10.5 gains the E523 row; §11.3 reads E501–E523 and gains the two-sided bullet; `GRAMMAR.ebnf` notes it on `for_statement`; the counter is in `logic.rs` and the budget in `validate.rs` |
| S8 | §9.8 describes the `{value}` substitution of E413 on a `text` range precisely: it is the **scalar count**, not the text. The message template and the implementation agree, so no message changed. | §9.8's substitutions paragraph is split: `{ranges}` leaves the list-substitution sentence and gets its own paragraph, and `{value}` gets one that states the `text`-range and id exception with two rendered examples; §10.4's E413 row repeats it |

## 2. S7 in detail — what one unit of work is

The bound is on **iteration**, because iteration is the only construct in the language that
multiplies: `derive`, `derive?`, `require` and `if` each run at most once per enclosing iteration, so
charging one unit per execution of a `for` body bounds the whole evaluation with a single counter.

- The budget is **per instance and per version**. `ValidationContext` is built once in
  `compile_instance` for exactly that pair, so the counter lives there, and the evaluator — which is
  built afresh for the instance's own block and for every nested `$(Schema)` block (§6.11) — charges
  against it. A block that runs for a nested object therefore spends the same budget as the object's
  own block, which is what §3.7 says and what a per-evaluator counter would have got wrong.
- The check is made **before** the iteration runs, and crossing the bound aborts that instance. A
  compiler that reported the diagnostic and then finished the loop would not have bounded anything.
- The diagnostic is positioned at the `for` whose iteration crossed the bound, not at the outermost
  one, so the position names the statement that was executing.

## 3. Examples re-derived by hand

| # | Example | Derivation |
|---|---|---|
| E1 | §6.3's `if` / `else if` / `else` / `require` block | New. Every `else` is reached across a blank line and a comment line; rule (b) skips both because neither carries a token, so the block is one `if` statement with two branches and an `otherwise`, followed by one `require`. Compiled as `revision-7/r7-04`, whose instance takes the `else if` branch |
| E2 | §3.7's "seven `for` blocks demand 11 111 110 iterations" | New, and computed rather than measured. A `for` at level *k* over ten elements costs `C(k) = 10 * (1 + C(k+1))` with `C(7) = 10`, so `C(1) = 11 111 110` for seven levels and `111 110` for five. The 1 000 001st charge falls on the tenth iteration of the **second** loop: one of its iterations costs `1 + C(3) = 111 111`, and `1 + 9 * 111 111 = 1 000 000`. The compiler reports it at that statement, which the unit test pins as `data/schema.abt:6:9`. Compiled as `revision-7/r7-01` and `revision-7/r7-02` |
| E3 | §9.8's `Range mismatch at atlas.tier: 5 is not in 1..3.` and `abcdefgh.id: 8 is not in 1..4.` | New. `{value}` is the scalar count in both, and `{ranges}` renders `1..3` as an interval; the id form is the same message with `{field}` fixed to `id` |
| E4 | §9.8's `text(2..2)` renders as `2` | New. An `IntRange` whose bounds are equal renders as the bare number, so `text(2..2)` and `text(2)` — the same constraint under §4.5 — render alike. A float part has no such collapse and `float(1.0..1.0)` renders both bounds. All four are pinned by `a_range_violation_names_the_measured_quantity` |
| E5 | §8.1's empty document | Unchanged bytes, newly reachable in a second documented way. `revision-7/r7-03` compiles a project of one `.abt` file with a logic block and no instance, and gets exactly the JSON block already printed in §8.1 |

Nothing else moved. No existing example nests a `for` more than three deep, sits an `else` past a
comment, or violates a `text` range, so §4.4.1, §5.x, §6.2's own example and the whole of Appendix C
are unaffected, and each was re-read to confirm it.

Mechanical checks run over the final files:

- every `Exxx` in the prose is a row of §10 and every row of §10 is used elsewhere — **123 rows, no
  duplicates, no orphans**; the families are contiguous except the still-reserved E809;
- `ErrorId::ALL` and §10 agree on 123 identifiers, which `every_catalogue_id_has_a_unique_code_and_a_template`
  asserts;
- `cargo build` and `cargo clippy --all-targets` are clean under `-D warnings`, `cargo fmt --check`
  is clean, and `cargo test` is green: 439 library unit tests, 26 binary unit tests, 43 CLI
  tests, 28 compiler tests (21 more stay `#[ignore]`d as the record of the 0.2.0 migration), 1 doc test,
  the documentation-example runner, and the conformance corpus at **282 cases** (273 passed, 9
  without an expectation, 0 failed).

## 4. New and changed error identifiers

| ID | Status | Condition |
|---|---|---|
| E523 | **new** | The logic of one instance executed more than 1 000 000 loop iterations for one version (§3.7, §6.2) |
| E511 | changed | Narrowed to the two reachable reasons. The unbalanced-parenthesis reason is withdrawn: it was unreachable, because §3.6 makes an unclosed `(` E203, a stray `)` E205 and a `]` closing a `(` E204, all in P1 |
| E413 | changed | No condition and no message changed. `{value}` is now defined for a `text` range and for an instance id: the scalar count, not the text |
| E517 | clarified | Unchanged in condition. §6.3 now says which fault an `else` after another statement is: rule (b) joins it to that statement, so the malformed logical line is reported where it is malformed (E210), and E517 is for an `else` that begins a logical line |
| E103 | clarified | Unchanged. §8.1 now names it as the case an empty `data` array is **not** |

## 5. Superseded entries of the previous revision

- The **Open** list of the Revision 6 section is closed by this revision: `S1`, `S4`, `S5`, `S7` and
  `S8` each have a ruling above. The list is left as it was written, because it records what the
  freeze knew.
- §6.6's sentence "An unknown operator, a missing operand, or an unbalanced parenthesis is E511" is
  withdrawn by S4 and replaced by the two bullets.
- §8.1's sentence "An empty `data` array is only reachable in single-file mode …" is withdrawn by S1
  and replaced by the two-way list.

## Open

None. Every issue the Revision 6 section recorded as open is settled by a ruling above, and no new
question was left behind: the two additions that could have opened one — the work limit's counting
rule and E413's `{value}` — are each stated in the specification with a worked example and pinned by
a test.
