# Abstract editor analysis protocol 1

This optional compiler protocol is independent of the language and document
versions. Existing CLI commands keep their behavior. It is not LSP: it exposes
the compiler's validation pipeline without adding a second validator or moving
assets into a temporary project. The crate remains dependency-free.

## Process and request contract

```text
abstract analyze --capabilities
abstract analyze <project-or-data-directory> --stdio
abstract analyze <project-or-data-directory> --stdio --symbols
abstract analyze <project-or-data-directory> --stdio --public
abstract analyze <project-or-data-directory> --stdio --values
```

The capability command returns JSON with `protocol: "abstract-analysis"`,
`version: 1`, `positionEncoding: "utf-16"`, the compiler version and the limits
below. Clients negotiate support; the compiler release version alone is not a
capability test.

The analysis command reads exactly one frame from stdin through EOF and returns
one JSON response on stdout. It does not write files, open sockets or persist
state. Successful protocol exchanges exit 0 even when source errors exist.
Invalid invocation, framing or overlay membership exits 2 with a plain tooling
error on stderr and no JSON. Language diagnostics retain their existing codes;
transport failures do not invent language error identifiers. Broken stdout
follows the existing CLI convention.

All numbers are unsigned 32-bit big endian; lengths count UTF-8 **bytes**.

| Field | Encoding |
| --- | --- |
| Magic/version | Eight ASCII bytes `ABANLZ01` |
| Request ID | u32, echoed unchanged |
| Overlay count | u32, 0–128 |
| Each overlay's path length | u32, 1–32,768 |
| Each overlay's text length | u32, 0–4,194,304 |
| Each overlay's path | Exactly the declared UTF-8 bytes |
| Each overlay's text | Exactly the declared UTF-8 bytes |

Both lengths precede that overlay's payload. There are no terminators, escapes,
compression or deletion operations. The complete frame is limited to **16 MiB**.
The decoder reads at most that limit plus one byte, checks lengths before
slicing, and rejects invalid UTF-8, truncation and trailing bytes. The editor
rejects unpaired UTF-16 surrogates before encoding instead of silently replacing
them with different source characters.

Paths are absolute filesystem paths with no NUL or `.`/`..` components. Existing
targets are canonicalized and must belong to the compiler's discovered sources.
Duplicate canonical targets are rejected. A discovered symlink/junction can
point outside the project: its canonical target is accepted because discovery
already included it. Naming an unrelated foreign file does not include it.

A new `.ab`/`.abt` source is accepted only when its parent exists and its
canonical path is inside the discovery root, outside ignored directories.
Existing ignored files are never reintroduced by overlays. No directory is
created. Empty text means an empty source; it does not delete a file. Untitled
buffers without a filesystem identity are outside the protocol.

## Compiler behavior

Project resolution and discovery use SPEC 2.3–2.4. Overlay bytes substitute for
disk reads **before** decoding, so valid editor text can replace an invalid
UTF-8 disk source. Source display paths, origins and deterministic ordering are
preserved. New eligible sources join the same ordering.

The existing compiler phases validate schemas, references, logic, versions and
instances with normal asset checks enabled. `assets_dir` remains the original
resolved `<project root>/assets`; no source or asset tree is copied or rewritten.
Disk-only sources and assets are read normally. This is a coherent overlay set,
not an atomic filesystem snapshot: external changes to other files during the
run remain a filesystem concurrency limitation.

## Response and locations

```json
{
  "protocol": "abstract-analysis",
  "version": 1,
  "requestId": 7,
  "compiler": "1.0.1",
  "positionEncoding": "utf-16",
  "analyzed": true,
  "truncated": false,
  "diagnostics": []
}
```

`analyzed: false` means discovery failed before source analysis and before the
final overlay-membership check; it does not confirm acceptance of the overlay
set. `true` means the pipeline ran; it does not mean every compiler phase
succeeded. No foreign overlay becomes a source merely because discovery failed.

Each diagnostic has `code`, `severity: "error"`, `message` and `notes`. Optional
location fields are `displayPath` (the usual compiler spelling), `path`
(canonical absolute path, slash-separated, without a Windows extended-length
prefix), and `range`. Notes have `message` and the same optional locations.
Unavailable information is omitted rather than attributed to another source.

Ranges use zero-based `{line, character}` positions and exclusive ends, with
characters counted in UTF-16 code units. Rust converts its existing one-based
Unicode-scalar positions using the **analyzed source**, including overlays.
For disk sources, a leading BOM is encoding metadata and is excluded, as in
VS Code's decoded text. Overlay strings are exact editor text: a leading BOM
inserted into a buffer occupies one code unit even though language positions
omit it. Clients must send decoded editor text rather than raw file bytes.
CRLF is one newline. Existing compiler diagnostics identify a point: each
range covers that scalar, or is empty at end of line. These are not claimed to
be full offending-token spans.

Responses contain at most 100 diagnostics, eight notes per diagnostic, 16 KiB
per message and 4 KiB per note. Truncation preserves UTF-8 character boundaries.
The response is bounded to **4 MiB**. `truncated: true` signals omitted entries
or shortened text; clients must not present that list as exhaustive.

## Optional semantic bindings, feature version 1

The capability response advertises `schemaBindings: 1` for the opt-in
`--stdio --symbols` mode. Absence or another value does not enable semantic
references or rename, even if ordinary diagnostic protocol 1 is supported.
The request frame and normal `--stdio` response are unchanged. No compiler
version string substitutes for this feature negotiation.

The symbols response adds `bindings` with `version: 1`, `complete`, `sources`
and `symbols`. A complete graph is emitted only after the normal compilation
pipeline succeeds with no diagnostics. It resolves compiler-validated schema,
field, instance and lexical loop identities. Schema occurrences cover
declarations, logic bindings, instance headers and `$(Name)` / `ref(Name)` type
arguments. Field occurrences cover declarations, header/body/clone/logic paths,
multi-path keys, tuple columns, tagged-object argument paths and interpolation.
Instance occurrences cover explicit or file-stem declarations, clone sources,
`ref` values and direct instance interpolation. Loop occurrences are
scope-aware across paths, operands, derive values, functions and interpolation.
Comments and unrelated strings never become references. Abstract 1.x has no
imports: references span the project's discovered source set.

Each source has canonical absolute `path` and `sha256` of the UTF-8 encoding of
its editor text: disk BOM metadata is omitted, an overlay's typed BOM remains.
Each symbol has a snapshot-local `id`, normalized semantic `name`, `kind`
(`schema`, `field`, `instance` or `loop`), `qualifiedName`, a `declaration`
location, `renamable`, `implicit` and `occurrences`. Fields and loops also carry
`type` / `shape` metadata where the compiler can state it. `ownerId` names the
declaring schema/field or lexical scope. Every location and occurrence carries
`path`, exact UTF-16 `range` and exact source `spelling`; occurrences also carry
their `symbolId` and `role: "declaration" | "reference"`. Schema names are exact
and case-sensitive. Field names, instance IDs and loop variables use language
normalisation, so two occurrences of one symbol may have different spellings.
Exactly one occurrence is the declaration.

A file-stem instance ID has no source token. It is still present so references
and definitions remain exhaustive, with `implicit: true`, `renamable: false`
and a zero-width declaration anchor at its instance header. Renaming it requires
a file operation outside this text-edit protocol. A field reached through a
dynamic path segment is also non-renamable when no single declaration owns the
indirect occurrence.

The complete graph is limited to 1024 sources, 16384 symbols, 65536 occurrences and the existing
4 MiB response budget. Failure, ambiguity or exhaustion returns
`complete: false`, a `reason`, and empty source/symbol arrays; it never publishes
a partial graph as exhaustive. Discovery failures and source diagnostics also
disable bindings. The client rejects incomplete/truncated graphs, malformed
identities/ranges, overlapping occurrences and hashes that disagree with its
captured source text. Existing request limits still apply to dirty or proposed
overlays, including the 128-file and 16 MiB aggregate limits.

Before launching any compiler process, the editor's schema feature admits
at most **1,024 distinct canonical sources**, **4 MiB per source** and **16 MiB
of aggregate source bytes**, including open/dirty and newly created buffers.
Open documents first pass a line-count lower bound and a public `offsetAt`
UTF-16 length check before `getText()` can join their full text. The line-count
check also bounds the host's offset-index work; this does not claim that
`offsetAt` itself allocates nothing. Actual UTF-8 byte admission follows.
Saved files are read through an opened file handle: a regular-file size check
precedes allocation, and fixed-size reads enforce the actual byte allowance
plus one sentinel byte if a file grows. Every handle closes on failure. UTF-8
bytes determine the limits; a saved BOM also counts towards disk admission.
Revalidation uses the same bounded reads. Rename checks the projected byte
growth before constructing candidates, then admits their actual UTF-8 text
before candidate compilation.

## Optional evaluated values, feature version 1

The capability response advertises `values: 1` for the opt-in
`--stdio --values` mode. The ordinary response gains `values` with
`version: 1`, `complete` and `document`. On success, `document` is the exact
canonical value tree that the compiler's existing P1–P5 pipeline would render
as JSON: the maximum-version `data` plus the reduced earlier-version
`overlays`. Defaults and logic-derived values have therefore already been
applied. The editor reconstructs a requested earlier version from those
overlays using the same documented representation; it does not evaluate
Abstract source. Clients must preserve JSON number lexemes when displaying
values: Abstract integers span signed 64-bit values, and exact float rendering
distinguishes spellings such as `-0.0` that JavaScript numbers do not preserve.

This mode runs one compilation and does not use the narrower public inventory
projection. A diagnostic snapshot returns `complete: false`, a reason and no
`document`. If the complete outer response would exceed 4 MiB, the same
unavailable shape replaces the document. No truncated compiled document is
ever published as complete. The request frame, source-overlay rules and
diagnostic envelope remain protocol version 1.

Schema discovery uses incremental directory iteration with rejection limits of
**32,768 entries per traversal**, **4,096 distinct directories**, and **128
directory levels** below the selected root. Root selection has the same entry
budget for its immediate-child scan. Exceeding any limit rejects the entire
operation; it never silently removes sources from the project. The optional
discovery limits do not change other callers' default discovery or bound the
tolerant completion index. These controls bound provider source capture and
read sizes, not universal extension/OS/compiler heap, CPU, or concurrent writes
after validation. The existing 128-overlay/request-frame limits still apply.
Filesystem cancellation is cooperative between awaited operations: it does not
abort an `open`, `read`, `realpath` or directory read already pending in the OS,
and these checks are not an I/O deadline.

The editor's schema operations capture project membership, source text and open
document versions. They negotiate on each operation, check source hashes, and
discard cancelled or superseded work. Rename rejects malformed names and
schema-exact or identifier-normalized owner-scope collisions, constructs edits only at bound occurrences, and asks the real
compiler to validate the complete proposed overlay set before returning a
`WorkspaceEdit`. It checks saved source bytes and discovered membership again
before returning. Preparation records also bind the document version and project
source hashes. Applying a rename can intentionally change the emitted `template`
identity; it is not a promise of unchanged compiler output.

Open URI-to-canonical-file bindings are re-resolved before return. A retargeted
alias invalidates the operation. Rename also rejects multiple open URI aliases
of an edited source, even when their buffers agree, because one URI edit does
not establish that every other buffer will be changed. Opening another source
document while analysis is pending also invalidates the operation. The provider
does not open source documents as a side effect of computing the edit.

References are contextual to one project. Rename refuses any change to a
canonical source outside that project's discovery root, even when discovery
reaches it through a link. It cannot discover unknown consumers in other project
roots or external data readers. Source checks are not filesystem transactions;
changes after the final check or to external assets remain outside that snapshot
guarantee. The provider returns an edit for VS Code to apply; it never writes
source files itself. Symbols marked `renamable: false` retain navigation and
references but never produce a partial edit.

In VS Code 1.92.0, the F2 Rename adapter converts the provider's edit without
`versionInfo`; the public `WorkspaceEdit` API cannot attach our source-snapshot
versions. The checks above therefore end at provider return. They do not promise
that changes made during a later F2 preview will be rejected by the host.
Programmatic tests that obtain an edit and call `workspace.applyEdit` exercise a
different host path, which attaches versions at application time, not at our
original calculation. See the primary
[Rename adapter](https://github.com/microsoft/vscode/blob/1.92.0/src/vs/workbench/api/common/extHostLanguageFeatures.ts#L736),
[edit conversion](https://github.com/microsoft/vscode/blob/1.92.0/src/vs/workbench/api/common/extHostTypeConverters.ts#L592)
and [bulk edit application](https://github.com/microsoft/vscode/blob/1.92.0/src/vs/workbench/api/common/extHostBulkEdits.ts#L26).

## Client cancellation, trust and compatibility

The VS Code client waits 250 ms after an edit, then sends all dirty file-backed
Abstract buffers in that project. One current generation owns each project's
diagnostics. Superseding edits, saves, closes, configuration changes and relevant
watched disk changes clear old entries and terminate active requests. Other
projects retain their entries. Protocol version, request ID, owning generation
and captured document versions must match before publication. Obsolete
callbacks cannot restore cleared diagnostics.

The client requests a five-second process timeout for capability probes and
thirty seconds for analysis. Output capture is bounded and execution uses
argument vectors without a shell. Superseding requests send `SIGKILL` to the
direct child. Terminating the normal Rust compiler also stops its worker thread.
These are requested timeouts/cancellation, not absolute completion deadlines:
a configured wrapper can ignore the timeout signal, or descendants can keep
inherited pipes open. There is no process-tree supervisor or OS memory sandbox,
and no universal bound on project complexity. Use the compiler executable
directly; stale-result checks remain in force even if process exit is delayed.

Probing, analysis and compiler commands require Workspace Trust. Compiler
configuration is resource-scoped and trust-restricted; entry points also check
trust when called programmatically. Schema references and rename also require
trust and their negotiated compiler feature. Static authoring features remain available
in Restricted Mode. Compile commands still require saved source files.

A legacy compiler's real E801 rejection of `analyze`, or an unsupported
advertised analysis version, enables explicit **Saved-file diagnostics** in the
status bar and output channel. No validation runs while that project has dirty
buffers. Saving every buffer enables legacy `lint` again. Missing executables,
malformed responses and timeouts are tooling failures, not successful fallback.
Configuration changes reprobe capabilities; absolute executable mtime/size also
participate in capability-cache identity.

## Optional public inventory 1

Negotiate `publicInventory: 1` in `analyze --capabilities` before invoking
`analyze <root> --stdio --public`. This mode uses the unchanged `ABANLZ01` frame;
it cannot be combined with `--symbols`. Compiler versions alone are not a
capability test. Ordinary analysis and schema bindings keep their existing
contracts. A client must bind the response compiler to its capability probe.

Discovery applies overlays once, retaining the actual assets root. One P1–P5
compilation supplies both validated declaration metadata and the inputs to the
existing public exporter. The projection does not run a second compilation or
duplicate the export profile's dependency decisions. It catalogues nominated
fields in every schema and syntactic group, including unused schemas and types
that the export profile would refuse if instantiated. Nested schemas are
catalogued at their own declarations, not recursively expanded for the catalogue.

The ordinary response gains `publicInventory` with these exact fields:

| Field | Contract |
| --- | --- |
| `version` | Integer `1` |
| `status` | `unavailable`, `export-rejected`, or `export-admitted` |
| `reason` | Tooling explanation on `unavailable` only |
| `catalogComplete` | False only when unavailable |
| `projectVersions` | Required `{min,max}` on admitted/rejected; omitted on unavailable |
| `sources` | Canonical absolute `{path,sha256}` entries, sorted by path |
| `declarations` | Sorted by `(declaringSchema, declaringPath)`; see below |
| `roots` | Successful fragment's unique `(rootSchema, rootInstanceId)` pairs, sorted |
| `exportDiagnostics` | Existing diagnostic shape, including UTF-16 locations and notes |
| `fragment` | Exact public-fragment value, present only on complete export success |

Each declaration has `declaringSchema`, `declaringPath` (normalized path segments
inside that schema), `spelling` (authored field name), `type`, booleans `list`,
`optional`, `tag`, and `versions`. Types are `text`, `int`, `float`, `bool`, `enum`,
`file`, `image`, `ref`, `nested`, or `group`; listness is separate. `versions` is
always a nonempty `{min,max}`: the field's own annotations resolved against the
project, before ancestor or instance intersection. P3 already rejects empty
declaration windows, even in unused schemas. An unannotated project is `1..1`.
Version integers remain u32 values, safely representable as JSON numbers.

`declaration: {path,range}` selects the exact authored field-name token.
`nomination: {path,range}` selects the exact `@public` tokens;
`nominationSpelling` is that captured source slice. The lexer enforces adjacency:
`@ public` remains E210, not new syntax. Locations are derived from AST positions
and matching lexer spans. Every successful fragment entry's
`(declaringSchema,declaringPath)` resolves to exactly one catalogue entry.
Each root has `rootSchema`, `rootInstanceId`, and `declaration: {path,range}`;
its range selects the schema-name token at the actual instance header. This is
navigation provenance, not an instance-ID or assignment-reference graph.

Hashes cover the editor-visible UTF-8 source snapshot. A disk BOM is omitted;
a BOM in an exact overlay is retained in both its hash and UTF-16 ranges.
All locations refer to inventoried sources. Declaration tuples are schema-local;
source ranges and enumeration order never become stable runtime identities.

Ordinary compilation failure returns its diagnostics at the response top level
and an unavailable inventory. Discovery failure also sets `analyzed: false`.
Unavailable means `catalogComplete: false`, empty sources/declarations/roots/
exportDiagnostics, and no fragment or projectVersions. Catalogue admission or
complete-response budget failures use this same state with a tooling reason,
without inventing a language error.

An export failure leaves a complete declaration catalogue and separate export
diagnostics, but empty roots and no fragment. The exporter is fail-fast; fields
not named by its first error are not individually approved. Success includes
the entire fragment, even for zero targets. An unused nomination is not thereby
proved admissible for future instances. Effective target windows, optional
absence and typed defaults remain the exporter's exact values: i64 decimal
strings and binary64 bit strings are not converted to floating JSON numbers.
The ordinary `truncated` flag also records shortened export diagnostics.

The added catalogue admits 1,024 sources, 4 MiB per source and 16 MiB aggregate
editor text; paths are limited to 32,768 UTF-8 bytes. It allows 16,384 declarations,
262,144 field visits, existing group depth, and 4 MiB cumulative copied path
bytes, charged before qualified path copies. Private scalar leaves do not copy
their group prefixes. Catalogue pieces and the complete response are bounded;
the final analysis envelope remains at most 4 MiB. A valid build export can
therefore have an unavailable editor preview. No partial fragment is published.

These are added-work and wire limits, not bounds on discovery/P1–P5 memory or
wall time. Provenance tokenization works on captured sources; the existing
exporter still constructs its bounded private document/envelope internally.
Only the fragment and editor metadata reach this response: no `documentJson`,
private body or build envelope is sent. Source directories/assets/compiler must
remain stable during analysis; hashes and client revalidation are not filesystem
transactions. Missing capability does not trigger saved-file public export.

The inventory remains unbound. It neither authorizes runtime overrides nor
implements field Rename, complete field references, consumer effects, YAML
activation, or downstream product/migration policy.

## Primary references

Position/version conventions are informed by the [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/),
without implementing JSON-RPC or the LSP lifecycle. Process handling follows
the [Node child-process API](https://nodejs.org/api/child_process.html); trust
gating follows the [VS Code Workspace Trust API](https://code.visualstudio.com/api/extension-guides/workspace-trust).
