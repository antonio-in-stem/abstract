# Schema references and rename: first semantic-symbol increment

Snapshot: 2026-09-08 UTC, based on
`5d5f4a24ec43d19e2dc488bef4408dc64c8e1500`. Extension **1.2.3**;
compiler still identifies as **1.0.1**, with separately negotiated
`schemaBindings: 1`. This is a bounded schema-symbol increment, not completion
of all language references/rename or the editor roadmap. No commit, publication,
global installation or benchmark was performed in this increment.

## Contract and implementation

Input is a discovered project plus its current file-backed editor overlays.
`analyze --stdio --symbols` runs the existing compiler pipeline, then builds
schema identities from parsed declarations, project tables and the compiler
lexer's exact `SchemaName` tokens. Declaration, logic binding, instance header,
nested type and reference type occurrences share one identity. Case-distinct
schemas remain distinct. The source language has no import syntax; invalid
nested loop shadowing remains E516 and disables this valid-snapshot feature.

Output is a complete, bounded graph with source hashes and UTF-16 token spans,
or an explicit unavailable result. The client checks those hashes, versions,
membership, ranges and identity invariants. Rename rejects collisions and
invalid names, applies only bound occurrence edits in memory, and recompiles
the proposed overlays before returning a `WorkspaceEdit`. The original
compiler's compilation behavior and diagnostic-only protocol remain unchanged.

See [the protocol](../ANALYSIS-PROTOCOL.md#optional-schema-bindings-feature-version-1)
for admission limits and [the extension README](../../editors/vscode/README.md)
for product scope. The tolerant completion/navigation index is never a Rename
fallback. Older compilers without this feature cannot enable schema operations.

## Verification and retained attempts

The raw logs and source/artifact hashes are retained locally under
`out/schema-symbols/`; `manifest.json` records the pre-resource-review snapshot. Its
SHA-256 is `e7c7a7179c3b831b2e41d0132a4808105a588cb782cb7a68f8eec3d22ae032b4`.
These are local build artifacts, not a hermetic or published evidence archive.
The table below describes that earlier snapshot; the resource correction and
its separate evidence are recorded below without replacing its logs/manifest.

| Check | Observed result |
|---|---|
| `cargo test --all-targets`, final compiler source | 547 passed, 0 failed, 21 inherited ignored tests (`cargo-test-02.log`) |
| All extension Node tests, final release compiler | 43 passed, 0 failed/skipped/cancelled (`node-release-02.log`); 17 tests added by this increment |
| Real VS Code 1.92.0, trusted isolated profile, final provider/release compiler and real preserved legacy compiler | Six scenario groups passed, no legacy skip (`host-04.log`) |
| Real VS Code 1.92.0 Restricted Mode | Completion remains available; compiler commands/diagnostics and semantic Rename are refused (`host-restricted-01.log`) |
| VSIX readback | 21 entries; new provider/client bytes equal source; previous icon bytes unchanged; no compiler, tests or node_modules bundled |

The 21 ignored Rust cases are inherited legacy compiler assertions; they are
not newly passing cases. The conformance harness is one Rust test function
that runs its own corpus, not hundreds of separately counted Rust tests.
The Restricted Mode host observation predates the last open-document
invalidation refinement; the final Node suite separately checks that unchanged
trust gate starts no compiler. Host tests use public provider commands and
`workspace.applyEdit`; they do not simulate a user holding F2 preview open.

The new tests cover cross-file declaration/logic/header/`$()`/`ref()` identities,
case distinctions, comments and value text, dirty declaration/use edits, eligible
new sources, BOM/CRLF/UTF-16, ambiguous and unresolved names, E516, collisions,
malformed graphs, source/version changes, cancellation, trust and capability
negotiation. Controlled provider tests use a small VS Code API adapter with the
real compiler; the separate Electron host exercises actual registration and
multi-file edits. These are distinct evidence layers.

Two canonical-output comparisons use owner-created disposable sources:
renaming a private schema used by nested/reference types preserves exact JSON
output bytes; renaming an emitted schema changes only the expected `template`
value in the parsed canonical document. Homonymous text, comments and an
independent case-distinct schema remain unchanged. This is not a theorem about
all downstream consumers or all possible renames.

Retained non-passing attempts are not overwritten:

- `host-01.log` and `schema-integration-01.js`: VS Code 1.92.0's command adapter
  passed trailing arguments as an array while creating a transient closed-file
  model, causing `Unexpected type` before the private-schema Rename provider.
  The test now opens that document as the editor flow does. This is visible in
  [the exact upstream adapter](https://github.com/microsoft/vscode/blob/1.92.0/src/vs/editor/browser/editorExtensions.ts#L463).
- `node-test-01.log`, `symbols-before-canonical-fix.rs` and
  `schema-providers-test-01.js`: a saved source's compiler origin retained its
  junction spelling. The first graph therefore disagreed with the client's
  canonical identity and was rejected. Canonicalizing that origin fixed the
  defect; the final tests cover contextual references and rejection of external
  Rename targets.
- `abstract-language-1.2.3-package-01.vsix` preserves the first package before
  the final open-document invalidation refinement. It is superseded by the
  current VSIX below; `package-02.log` records that packaging operation.

Independent source review identified the F2 version limitation and two alias
cases. The final provider rechecks every captured open URI's canonical target,
rejects edits to multiple open aliases even if their text agrees, and cancels
when another source document opens during analysis. Controlled tests retarget
only their own temporary links and verify that source bytes remain unchanged;
they execute no untrusted source or modification of a real user project.

## Artifacts and practical limits

| Local artifact | Bytes | SHA-256 |
|---|---:|---|
| `editors/vscode/abstract-language-1.2.3.vsix`, after both resource corrections | 42,252 | `64aa8bf2f7cf044733b0c2890c14417cb0d9d77e2a0d3c387af39abc7551622d` |
| `target/release/abstract.exe` | 1,409,536 | `f4f1a45e6d37e4e345160de49ebe1da0f4a22e1dba151e2186357136ddd2d8c5` |
| Preserved `editors/vscode/abstract-language-1.2.2.vsix` | 33,227 | `f59c7c6a42d3d123025d6f28849c4f984de988615591d17173d2d78c1c5daf86` |

The new compiler must be configured separately; the VSIX does not silently
replace an installed compiler. All host runs use temporary profiles/projects.
The observed VS Code startup logs include environment mutex/chat registry
messages; passing results above refer to the explicit test assertions and exit
codes, not absence of all background application messages.

References remain contextual to one project. Rename refuses edits outside its
canonical discovery root, including externally linked declarations, and cannot
establish all other consumers of sources inside that root. Public schema names
may be an output API. Invalid projects disable these operations rather than
providing approximate edits. Resource limits are implemented admission limits;
this increment made no throughput or memory-stress measurements.

Snapshot checks end at provider return. The public VS Code 1.92 Rename adapter
does not carry our captured source versions into later F2 preview/application.
No private API or inferred version guarantee is used. The protocol records
primary source anchors for that limitation. Concurrent external asset changes
and filesystem changes after the final check also remain outside the guarantee.

Fields, normalized/versioned instance IDs, clone/reference targets and local
loop variables remain planned work. Loop bindings must distinguish sibling
scopes and resolve interpolations; field identities must follow nested schemas,
groups and dynamic paths. Those capabilities require further compiler-owned
resolution and their own differential tests, not broader text replacement.

## Resource-admission correction (2026-09-08)

Independent review found that provider capture/revalidation used unbounded
`fs.readFile`, and discovery accumulated all paths before the graph source cap
was checked. Trust and compiler protocol limits did not bound those earlier
extension-host allocations. `resource-boundary-01/provider-red.log` records the
new regression against the prior provider: a sparse **4 MiB + 1 byte** source
reached compiler analysis and its 30-second timeout instead of being rejected
before launching a process. No multi-GiB file was read or allocated.

The provider now admits 4 MiB per source, 16 MiB aggregate bytes and 1,024
canonical sources, counting dirty/new buffers as well as saved files. It checks
actual bytes through opened handles, catches growth beyond the allowance with
one sentinel byte, and closes handles on errors. Initial saved-file validation
occurs before capability negotiation; later validation uses the same limits.
Rename preflights projected expansion before constructing candidate strings,
then checks their actual text before candidate compilation. Schema-only
discovery options use incremental `opendir` iteration and explicit entry,
directory and depth caps. The default discovery path for other callers remains
unchanged. No limit silently truncates a project.

`out/schema-symbols/resource-boundary-01/manifest.json` binds the corrected
sources, artifact and logs. `prior/` preserves the previous source snapshot,
39,914-byte VSIX and exact earlier manifest; the original logs remain intact
in the parent evidence directory. The compiler binary and Rust sources did not
change during this correction, so the prior Rust results remain applicable.

- `targeted-02.log`: **27/27** relevant Node cases passed (8 schema-binding,
  13 provider, 4 bounded-resource and 2 project-index cases). Eight cases were
  added by this correction. Controls include aggregate/UTF-8/dirty source
  admission, the 1,025th saved/new source, exact-limit reads, growth after stat
  and during analysis, candidate expansion, and handle closure on failure.
  Small real-file growth controls read exactly 9 bytes for an 8-byte allowance;
  they do not measure arbitrary concurrent-writer interleavings or heap peaks.
- `host-01.log`: all **six** real VS Code 1.92.0 scenario groups passed with
  the corrected provider and the same release/legacy compilers, using an
  isolated temporary profile. This is a functional host regression, not a
  claim that the host itself stress-tested every resource limit.
- `package-01.log` and `vsix-readback.json`: the corrected package contains
  **22 entries**, including `schema-resources.js`; all packaged source bytes
  match the working sources, and icon bytes match the previous package.

This bounds provider source admission and read sizes. It is not a universal
heap/CPU bound for VS Code, the tolerant index, filesystem discovery by other
callers, or the compiler. The existing overlay-frame limits and the public F2
handoff/concurrent-filesystem limitations still apply. No new Rust build,
benchmark, installed-user-profile change or publication was performed.

### Open-document preflight refinement

A second read-only review identified that `getText()` itself could join an
oversized open document before its UTF-8 admission. The next retained attempt,
`out/schema-symbols/resource-boundary-02/`, closes that gap with a line-count
lower bound followed by public `offsetAt(Position(lineCount, 0))` before
`getText()`. VS Code adjusts the position to the document end. The line-count
check precedes offset lookup because its implementation may build a line index;
the API is not assumed to be allocation-free. See the primary
[TextDocument API](https://code.visualstudio.com/api/references/vscode-api#TextDocument)
and [VS Code 1.92.0 implementation](https://github.com/microsoft/vscode/blob/1.92.0/src/vs/workbench/api/common/extHostDocumentData.ts#L134).

Its `provider-red.log` and exact `provider-test-red.js` preserve the failure
before the fix; the fixture refuses to materialize full text and uses only
small owned files. `targeted-01.log` records **28/28** relevant Node tests, and
`host-01.log` records the same **six** real host groups passing with the final
provider. The new case checks both an oversized single line and excessive line
count, with no compiler launched. The prior 42,042-byte package and all attempt
01 records remain frozen in that attempt's `final/` and manifest. Attempt 02's
manifest and `vsix-readback.json` bind the current 22-entry package and exact
sources. No other blocking finding was reported in the independent source
review. Filesystem cancellation remains cooperative between awaited operations;
it does not abort already pending OS I/O or provide an I/O deadline.
