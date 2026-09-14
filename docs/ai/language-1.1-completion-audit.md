# Abstract 1.1 language completion audit

Date: 2026-09-14

Starting revision: `ffa9ad3`

Release under test: compiler 1.3.0

This audit compared the normative [`../SPEC.md`](../SPEC.md) and
[`../GRAMMAR.ebnf`](../GRAMMAR.ebnf) with the parser, schema validator,
instance resolver, logic evaluator, output renderers, CLI and active
conformance tests. The readable positive integration is
[`../../examples/automotive/FEATURE-COVERAGE.md`](../../examples/automotive/FEATURE-COVERAGE.md),
which maps every specification chapter to a source fixture and an active test.

The existing compiler already implemented the Abstract 1.1 language forms.
The concrete missing compiler work was editor-facing semantic evidence, rather
than new source syntax:

- schema-only bindings did not identify fields, instances or lexical loops;
- dynamic field paths could not support a provably complete rename;
- occurrences did not expose exact authored spellings and ownership metadata;
- the editor could not request the compiler's effective defaulted, derived and
  versioned values without compiling separately.

Compiler 1.3.0 closes those gaps with negotiated `schemaBindings: 1` and
`values: 1` responses. Bindings are complete-or-unavailable, use precise UTF-16
ranges, retain authored spelling, distinguish declaration and reference roles,
and refuse rename for identities affected by dynamic dispatch. Evaluated values
reuse the ordinary P1–P5 result and return its base-plus-overlays envelope;
they preserve signed 64-bit integer and exact float lexemes and fail closed at
the response limit. [`../ANALYSIS-PROTOCOL.md`](../ANALYSIS-PROTOCOL.md) records
the wire contract.

The release gate ran `cargo test --all-targets`: 579 tests passed, zero failed,
and 21 tests stayed ignored because their fixtures assert behavior made invalid
by the stable 1.0 specification. The 301-case conformance corpus, documentation
examples, automotive JSON/YAML/RAW byte goldens, six automotive diagnostics,
analysis protocol tests and public-contract tests all passed. The release was
built with `cargo build --release --locked --offline`; its automotive JSON is
byte-identical to the checked-in 1.3.0 golden.

Appendix D's bundle, Java/runtime and distribution contracts remain existing
optional subsystems. Their active Rust coverage passed in this run, but this
language/authoring audit did not redesign or make new hostile-host security
claims about them.
