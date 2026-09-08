# Public inventory compiler validation

2026-09-08. Compiler 1.2.0 / extension 1.4.0 implementation checkpoint. Compiler
and protocol gates and final client checks below are complete for this increment. This is
agent implementation/review and finite testing, not a human audit or
runtime-override validation.

## Change and source boundary

The compiler adds `publicInventory: 1` and `analyze <root> --stdio --public` over
the existing overlay frame. `src/analysis/public_inventory.rs` invokes the existing
private compiler projection and public exporter once, retains a declaration
catalogue/provenance, and returns a complete fragment only on export success.
Ordinary and export diagnostics remain separate. The editor wire contains no
private compiled document or build envelope. `lib.rs`, `public_contract.rs`, the
language grammar and ordinary/schema-analysis APIs were not changed by this work.

Production changes are `src/analysis.rs`, the new inventory module, `src/cli.rs`,
and the separately reproduced correctness fix in `src/versions.rs`. Tests are
inventory-module units and `tests/public_inventory_cli.rs`. Root owns release
version changes; the extension was implemented by another contributor.

The current protocol is in [ANALYSIS-PROTOCOL.md](../ANALYSIS-PROTOCOL.md#optional-public-inventory-1).
Two initial assumptions were corrected before client integration: the lexer
already rejects `@ public`, despite the parser's modifier branch not checking
adjacency itself; and P3 rejects every empty declaration window, so `versions`
is required/nonempty in a successful catalogue. Neither changed language syntax.

## Preserved execution records

All paths below are under `out/public-inventory/`. Each Rust attempt records
commands, captured Rust sources/Cargo inputs, tool versions and exact stdout/
stderr bytes with hashes. These are local evidence, not a hermetic toolchain.
Commands used Cargo 1.95.0 and rustc 1.95.0 (`59807616e1fa2540724bfbac14d7976d7e4a3860`),
host `x86_64-pc-windows-msvc`, LLVM 22.1.2.

| Record | Actual outcome |
| --- | --- |
| `rust-targeted-01` | Compiled; 8/9 inventory units passed. The authored `@removed(1)` positive fixture was correctly rejected with E604. CLI stage not run. |
| `version-max-01` | Four actual invocations of the preserved release 1.1.0 compiler reproduced the maximum-version defect described below. |
| `version-max-red-01` | Two new maximum-version tests failed before the production fix: ordinary E605/E430 prevented valid endpoint behavior. |
| `rust-targeted-02` | 11/12 units passed, including both maximum-version controls after the fix. A remaining positive fixture omitted a required group at v5 and correctly failed E411. CLI stage not run. |
| `rust-targeted-03` | 12/12 inventory units and 4/4 real CLI tests passed, zero skips. The fixture now bounds its group consistently; no production validation rule was weakened. |
| `protocol-fixture-01` | Three real CLI responses: admitted, export-rejected, unavailable; exact frames, overlays, disk sources and capability response retained. |

The protocol fixtures were produced before the version bump and final response
fallback by a 2,656,768-byte debug executable, SHA-256
`dedaf6e0b8606cf0585923c7bc35dee16ae9571faf39ec3c4fc57ccd27105488`.
They establish observed protocol shapes for that snapshot, not final release
binary provenance. Their response hashes are recorded in their own manifest.

After targeted-03, a complete-envelope size fallback and one additional unit test
were added: nesting indentation can make the outer JSON exceed 4 MiB even when
an earlier piece estimate fits. The fallback returns unavailable without partial
data. The coordinated `rust-01` stdout records that thirteenth inventory unit,
`complete_envelope_budget_failure_returns_unavailable_without_partial_data`, as
passed. It is not counted retroactively among targeted-03's 12 executed units.

## Coordinated compiler and protocol gates

The following records are in the separate Covenant checkout, under
`C:/Users/PC/Repositories/covenant/out/research/abstract-public-inventory-20260908-01/`.
This note was checked against their result files and raw Rust/Node output; the six
Rust/release/Node stdout/stderr files also matched their recorded sizes and hashes.
No archived scripts or compiler commands were rerun for this documentation review.

| Record | Actual outcome |
| --- | --- |
| `rust-01` | `cargo test --locked --offline` exited 0. Summing the ten raw test summaries gives 601 passed, 21 inherited ignored, zero failed. Includes the final complete-envelope fallback regression. |
| `release-01` | `cargo build --release --locked --offline` exited 0; stderr identifies `abstract-lang v1.2.0`. Produced the executable identified below. |
| `node-01` | Node test runner: 72 passed, zero failed, cancelled, skipped or todo. These results precede the subsequent client lifetime fix. |
| `wire-01` | Preserved harness failure, no completed fixture: public export exited 0 but the harness incorrectly rejected its normal stderr success message. |
| `wire-02` | Four differential fixtures passed: numbers (13 declarations / 13 targets), Unicode (7 / 7), nested (2 / 8), versions (5 / 5). The harness now checks the exact expected export success message. |
| `independent-cli-01` | Preserved failure after four passing cases: the authored export-rejection fixture omitted the required `choices` group, so ordinary compilation correctly returned E411 and inventory was unavailable. |
| `independent-cli-02` | Five cases passed: single MAX, partition at MAX, dirty negative zero, dirty invalid source, export rejection. The corrected last fixture supplies `choices.flag: true` and first proves ordinary compilation succeeds. |

The Rust, release and Node records report no changes between their 1,757-entry
captured input maps. Their source revision is the pre-existing commit
`89761508382440fdfb0d40206720b8a62c4e473a` plus the captured worktree changes;
the revision alone does not identify this increment. These are local gates, not
hermetic builds or complete toolchain archives.

Final compiler executable: 1,611,264 bytes, SHA-256
`67805776a54abafaffa80ab9736161a8cf4fa986a672da822caf3263db906b82`.
The release result, both final probes and a readback of the current executable
agree on that size/hash. The independent probe records `abstract 1.2.0` and
unchanged executable hashes before/after its five cases.

`wire-02` compares each inventory fragment with actual current and preserved
1.1.0 public exports and the previously frozen corpus fragment. It also compares
their private compiled document bytes after replacing only the producer-version
string, checks source hashes, pins the analysis compiler to capabilities, and
checks that the editor response omits the document and its hash. This is a finite
four-project differential; it does not establish whole-language equivalence.

After the lifetime and discovery-selection corrections, `node-02` passes all
78 cases. `public-host-min-02` and `public-host-current-02` each pass ten actual
QuickPick/diagnostic/navigation scenarios on VS Code 1.92.0 and 1.136.1.
`public-host-restricted-02` passes its Restricted Mode control. `schema-host-01`
passes five existing authoring/schema groups, with its separate pre-protocol
legacy case explicitly skipped. The public host's real 1.1.0 capability refusal
did execute. These final runs use the exact release 1.2.0 hash above and report
unchanged captured sources. Host logs retain nonfatal upstream messages.

Five lifetime regressions fail against the exact earlier captured provider and
pass after correction. A real-directory discovery regression fails against the
earlier capture and passes after recomputing the compiler's selected source
directory. Only bounded diagnostic identities/text survive a closed picker.
The final 1.4.0 VSIX builds and its 23 source payloads match current/captured
editor files. SHA-256:
`ae0893ea9c1e45c0c5f866764fc3a7dc24c86d39864fdbc36cbeaa2f12b0e73f`,
54,149 bytes. Actual hosts tested the development extension in isolated profiles;
this does not claim a VSIX installation or normal-profile update. Covenant's
`docs/research/ABSTRACT-PUBLIC-INVENTORY.md` records the coordinated evidence,
including a preserved artifact-checker failure on empty historical JAR directories
and the complete successful byte readback in `artifacts-02`.

## Maximum-version correctness repair

Preserved baseline executable: Abstract 1.1.0, 1,525,760 bytes, SHA-256
`7fbececbabccc8203956bd2a6c46d0d25d443329d6616570dd076da7485b8521`.
It was hashed before the probe and was not overwritten by these debug builds.

Minimal source is `schema Settings { n: int @public = 1 }` in normal multiline
syntax, an instance `Settings :: @id.main`, and either project window:

- `4294967295..4294967295`: both ordinary and public compilation returned E605.
- `4294967294..4294967295`: both succeeded but the final base was empty; only the
  previous-version overlay and target variant retained the instance/value.

The inherited `Window::resolve` computed an omitted removal bound with saturating
`max + 1`, then subtracted one. The fix directly uses `project.max` as the inclusive
end when removal is absent. Explicit removal still uses `checked_sub(1)`, preserving
its exclusive meaning and rejection of zero. No version-domain narrowing was added.

The new tests assert source-derived final data, full target/declaration windows,
and exact fragment equality across ordinary compilation, export and inventory.
They also check explicit `@removed(MAX)`, changed signed zero, optional presence,
overlay reduction and version-set rendering near MAX. Source review of the reached
reduction loops found their `+1` operations guarded below the maximum; the focused
tests exercise those paths in the debug build with overflow checks. This is not a
proof about every arithmetic operation in the language implementation.

## Coverage and limits

Executed controls include unused/non-scalar nominations, repeated nested targets,
root navigation, own versus effective windows, i64 extremes and values above 2^53,
signed zero, optional absence, unexecuted logic dependencies, dirty-source repair,
BOM/CRLF/UTF-16 provenance, new unsaved sources, real relative assets, two-project
overlay refusal, malformed framing and ordinary/schema-mode compatibility.

A source review found needless prefix copying for private scalar descendants.
The catalogue now skips those copies and charges cumulative group/public path
copies before allocating them; exact-boundary and no-private-copy controls passed.
Wire/catalogue limits do not bound inherited P1–P5 allocation or the exporter's
internal private envelope. Snapshot hashes are not filesystem transactions.
The release executable and client package were built and checked as recorded
above. No runtime eligibility, performance result,
copy restriction or human validation is established by these compiler attempts.
