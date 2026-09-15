# Compatibility

Abstract versions its language, compiler and consumers separately. The
[release manifest](../release-manifest.json) records the components shipped
together. CI checks it against their source declarations.

| Contract | Current version | What it identifies |
| --- | --- | --- |
| Compiler | 1.5.0 | CLI and Rust library implementation |
| Language | 1.2 | `.ab` and `.abt` syntax and evaluation rules |
| Compiled document | 1 | JSON, YAML and RAW envelope and overlays |
| Editor analysis | 1 | `abstract-analysis` protocol; request magic `ABANLZ01` |
| VS Code extension | 1.7.2 | Authoring support; VS Code 1.92.0 or newer |
| Java reader | 1.1.0 | Document format 1 and ABX1; Java 8 or newer |
| Optional bundle | ABX1 | Container framing and authenticated encryption |

## Authors and consumers

Compiler 1.5.0 preserves language 1.2 and document format 1. Arithmetic still
requires compiler 1.4.0 or newer. A consumer reads the document's `abstract.format`
to select its decoder; `abstract.compiler` identifies the producer, not a new
data format. Model `versions` ranges belong to the authored data and have no
relationship to compiler releases.

The extension negotiates the compiler's capabilities before using analysis,
navigation, renaming or evaluated values. It does not infer them from version
numbers. See the [analysis protocol](ANALYSIS-PROTOCOL.md).

The Java reader consumes compiled data, so it does not need to understand new
source syntax that emits the same document format. Runtime 1.1.0 adds a Bouncy
Castle dependency for bundle cryptography; applications must include it at runtime.

## Bundle migration in compiler 1.5.0

New sealed bundles require `--key hex:<64 hexadecimal digits>`. Generate a key
with `abstract keygen` and keep it secret. Human passphrases are refused when
creating a bundle. The `--plain` option is unchanged.

Existing ABX1 files remain readable, including files created with the legacy
SHA-256 passphrase derivation. `unbundle --key` and Java's
`AbstractKeys.fromKeyMaterial(...)` retain that decoding rule. Repackage old
data with a fresh random key to migrate; upgrading the reader does not improve
the security of a previously distributed file.

Rust callers should use the fallible `bundle::try_encode` to handle unavailable
OS randomness. The existing `encode` signature remains available for source
compatibility. See the [bundle security model](../java/SECURITY.md) for limits.

## Evolution

Language syntax, output format and protocol changes must identify their own
compatibility impact and include regression tests. A compiler version bump alone
does not authorize changing document format 1 or analysis protocol 1.

Existing public Rust modules remain public in this release. Removing them would
break consumers, even when the modules look internal. New consumers should
start with the compilation functions and types re-exported at the crate root.
Any future API reduction needs an announced migration and a major version.

Release notes must call out security-related behavior changes, such as the
restriction on creating bundles with passphrases. Existing files are not
silently rewritten or reinterpreted.
