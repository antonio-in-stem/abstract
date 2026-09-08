# Abstract editor analysis protocol 1

This optional compiler protocol is independent of the language and document
versions. Existing CLI commands keep their behavior. It is not LSP: it exposes
the compiler's validation pipeline without adding a second validator or moving
assets into a temporary project. The crate remains dependency-free.

## Process and request contract

```text
abstract analyze --capabilities
abstract analyze <project-or-data-directory> --stdio
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
trust when called programmatically. Static authoring features remain available
in Restricted Mode. Compile commands still require saved source files.

A legacy compiler's real E801 rejection of `analyze`, or an unsupported
advertised analysis version, enables explicit **Saved-file diagnostics** in the
status bar and output channel. No validation runs while that project has dirty
buffers. Saving every buffer enables legacy `lint` again. Missing executables,
malformed responses and timeouts are tooling failures, not successful fallback.
Configuration changes reprobe capabilities; absolute executable mtime/size also
participate in capability-cache identity.

## Primary references

Position/version conventions are informed by the [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/),
without implementing JSON-RPC or the LSP lifecycle. Process handling follows
the [Node child-process API](https://nodejs.org/api/child_process.html); trust
gating follows the [VS Code Workspace Trust API](https://code.visualstudio.com/api/extension-guides/workspace-trust).
