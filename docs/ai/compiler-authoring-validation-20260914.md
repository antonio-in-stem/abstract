# Compiler and authoring validation — 2026-09-14

Scope: uncommitted release work based on `ffa9ad3`, compiler 1.3.0, Windows
x86-64. This record covers the Rust compiler, analysis protocol and automotive
corpus. The VS Code host and package gates are recorded separately by their
editor validation.

## Commands and results

| Command / check | Result |
| --- | --- |
| `cargo fmt --all` | exit 0 |
| `cargo check --all-targets` | exit 0, `abstract-lang v1.3.0` |
| `cargo test --all-targets` | exit 0; 607 passed, 21 ignored, 0 failed |
| `cargo build --release --locked --offline` | exit 0, optimized release |
| `target/release/abstract.exe --version` | `abstract 1.3.0` |
| release compile of `examples/automotive` as JSON | exit 0; byte-identical to `expected.json` |
| automotive lint and JSON/YML/RAW compile | exit 0 in all four runs |
| `git diff --check` | no whitespace errors; Git emitted only configured LF-to-CRLF notices |

The active-test breakdown from the captured full run is 460 library, 26 binary,
3 automotive, 47 CLI, 28 compiler integration, 4 conformance harness, 1
documentation examples, 17 public contract, 8 analysis CLI and 13 public
modifier tests. That totals 607. An earlier 601 summary is not directly
comparable because its exact snapshot and counting convention were not retained;
the current diff deletes no test and adds seven top-level integration tests
(three automotive and four analysis-protocol tests). The 21 ignored compiler tests
remain explicitly labelled obsolete expectations invalidated by language 1.0;
they are not counted as passing coverage.

The conformance harness passed its 301-case corpus. The automotive gates also
compiled all three output formats byte-for-byte, exercised a complete semantic
binding graph and checked the six expected diagnostic projects (`E413`, `E414`,
`E421`, `E431`, and two `E515` relations). Analysis tests cover field, instance
and scope-distinct loop identities; nested, tuple, tag, clone, reference and
interpolation occurrences; dynamic-path rename refusal; evaluated defaults,
derived values and overlays; compiler-diagnostic refusal; and verbatim
`9223372036854775807` / `-0.0` value lexemes.

## Release artifact

`target/release/abstract.exe`

- size: 1,738,240 bytes
- SHA-256: `D9E511B56A006AC3860E0E7B47C3934FC759CB72A1D03D700A268247131902CF`

The automotive car PNG was decoded as 1536×1024 and satisfies its exact image
constraint. The official Abstract logo PNG was decoded as 1200×1200; its
gallery field has no exact-dimension constraint. The local JPEG and text assets
passed the compiler's normal asset checks without `--skip-assets`.

## Limits of this evidence

These are local automated results on one Windows host. They are evidence for
the compiler and authoring contracts exercised by the checked-in tests, not an
external human audit or a hostile-host security evaluation. Appendix D's
optional bundle/runtime/distribution code kept its active Rust tests, but this
work did not recertify its broader security or deployment claims.
