# Public scalar compilation

Abstract 1.1 adds the author modifier `@public` and an opt-in build export.
The marker nominates a field; ordinary compilation still emits the same data,
including private fields, and applies the same validation and logic. A marker
does not grant runtime access or establish that an override is safe.

```abstract
schema DisplaySettings {
    caption: text(1..80) @public = "Welcome"
    scale: float(0.5..2.0) @public = 1.0
    quota: int(1..1000) @public = 64
    show_preview: bool @public = true
    tone: enum(quiet, bright) @public = quiet
}
```

`@public` has no arguments and occupies the existing modifier position before
the default. Repetition is an error. A marker on a container does not mark its
children. The language accepts nominations on its field types; export applies
the stricter profile below. Reserved envelope fields remain reserved. Duplicate
field declarations remain errors, including declarations with disjoint version
windows.

## Invocation

```text
abstract public-contract --capabilities
abstract public-contract <project-or-data-directory> [--out <file>]
abstract public-contract <file.ab>... [--out <file>] [--skip-assets] [--max-errors <n>]
```

Negotiate this command independently from the editor analysis protocol. Its
capability response has protocol `abstract-public-compilation`, version `1`,
supported profiles, producer version, and `bindingStatus: "unbound"`. It does
not advertise public field references or dirty-buffer inventory through
`analyze --symbols`; that protocol still describes schema names only.

The command discovers sources once, runs the same P1–P5 compiler pipeline, and
projects the resolved declarations and every selected version materialization
into one private interchange envelope. File selection retains ordinary compiler
semantics: the whole project is validated; only selected instances are emitted.
The library offers `compile_public_paths`, `compile_public_layout`, and
`compile_public_sources`; the in-memory form retains `compile_sources` asset
behavior. Ordinary `compile` and `lint` do not require public export admission.

Language or export failures produce diagnostics, nonzero exit status and no
successful envelope. E702 means an unsupported profile/dependency; E703 means
an export budget or structural consistency failure. Input/output protection and
IO errors use the existing CLI rules. Validation failure leaves an existing
destination untouched. The existing file writer is not an atomic publication
transaction: consumers must reject truncated or failed output.

## Profile: independent-scalars-v1

Supported targets are text, signed i64 integers, finite binary64 floats, booleans
and enums, at the root or in fixed groups and nested schema objects. Lists,
descendants reached through lists, tag fields, container nominations, file,
image and reference fields are rejected by this profile. Recursive occurrence
graphs containing reachable nominations are also rejected. This is an initial
export boundary, not a permanent restriction on future runtime contracts.

A target identity is the tuple `(rootSchema, rootInstanceId, pathSegments)`.
It retains the declaring schema/path and applicable version interval. Schema
names remain exact; other identifiers use normal Abstract normalization. These
are local identities: a downstream build system must qualify them with its
verified product/revision/member identity. Offsets and editor enumeration IDs
are not public identities. Renames can require downstream migration.

Every applicable version is considered, including versions that overlay
reduction later coalesces. A missing optional field or ancestor is explicitly
absent and non-writable; an override cannot create it. Adjacent variants merge
only when presence and exact typed defaults agree. The declared scalar domain
is retained. Public/private field redeclarations are not a language feature.

Admission walks the logic AST, including every branch, conditional derive,
require, loop, path read/write and presence/length observation. Static paths
can prove an unrelated field disjoint; a dynamic path covers its known prefix,
and unresolved loop-variable effects cover their enclosing logic scope.
Clone source and destination subtrees are dependencies. Any dollar-containing
authored/default interpolation, including an escaped dollar, conservatively
covers the containing instance; cloned interpolation is included. Dependency
windows are not used to discard facts. These choices can reject independent
cases, but the exporter never silently omits a rejected nominated target and
reports a complete result. Successful baked execution is not the analysis.

## Envelope and lossless values

The top-level fields, in serialization order, are `format`, `formatVersion`,
`profile`, `compiler`, `languageSemantics`, `documentFormat`, `documentJson`,
`documentSha256`, and `publicFragment`.

- `format` is `abstract.public-compilation`, format version is `1`, scalar
  semantics are `abstract-scalars-v1`, and compiled document format is `1`.
- `documentJson` is the exact JSON rendering from this compilation, as a string.
  `documentSha256` is the lowercase SHA-256 of its UTF-8 bytes. The digest binds
  bytes within the envelope; it does not authenticate the producer.
- `publicFragment` has format version, dependency profile, `complete: true`,
  `bindingStatus: "unbound"`, and sorted `entries`. Empty exposure is explicit.
- Entries carry their full structured identity, declaration interval, domain
  and version variants. Each variant carries its interval, presence, writable
  flag and, only when present, its effective baked default.
- The domain's type discriminates the default encoding. Integer values and
  integer range endpoints are canonical decimal strings, without a floating
  intermediary. Float values/endpoints are exactly 16 lowercase hexadecimal
  digits of IEEE-754 bits, preserving negative zero. Text and enum values are
  strings, booleans are JSON booleans. Enum order and range unions are retained.
  Text range units are Unicode scalars. Absence is never encoded as null.

The envelope contains private compiled data. Do not publish the entire envelope
as a public schema. The fragment intentionally discloses public domains,
defaults and local schema identities. It does not contain author source paths
or private logic. Compact export JSON preserves construction key order,
Unicode tuple target order and array order, uses UTF-8 with escaped JSON controls,
and ends with one LF. It is not JCS applied to raw floating JSON numbers.

## Budgets and remaining bindings

Export applies caps of 16,384 targets, 262,144 traversal/value/version work visits,
65,536 dependency facts, 4 MiB of copied text and serialized fragment bytes,
and 64 MiB serialized private envelope bytes. Conservative rendering preflight
and accounting can reject before the final serialized size reaches a cap.
Existing depth/version limits also apply. These budgets govern added export
work; they do not bound existing compiler discovery, P1–P4 allocations, total
process memory, or wall-clock time.

An unbound fragment is not a runtime contract. Downstream integration must still
verify the exact compiled payload and target values, qualify identities, bind
consumers/effects/update policy, define resource limits and public-domain
narrowing, and authenticate the completed contract. A future dependent profile
needs its own bounded evaluation plan and equivalence evidence. This increment
does not run YAML overrides, provide Java projections, sandbox plugin code,
or prevent observations or copying of the compiled product.
