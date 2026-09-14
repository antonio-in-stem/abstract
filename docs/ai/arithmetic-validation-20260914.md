# Abstract 1.2 arithmetic delivery

Baseline: `843ce85`. Compiler 1.4.0, VS Code extension 1.7.0.

## Implemented boundary

Explicit `calc(...)` provides checked arithmetic in derive values and condition
operands. Parsing, static types, evaluated values, editor bindings and public
dependency analysis share the compiler AST. Existing instance text and data
formats remain unchanged. Previously literal unquoted `calc(...)` in the new
logic positions must be quoted on migration. See `docs/ARITHMETIC.md`.

Shared schemas and their logic already provide reusable nested contracts.
Conditional shape rules demonstrate variants without adding union syntax.
Six exercises separate prompts, hints, starters and explained solutions.

## Verification

- Full `cargo test` passed: 616 tests including one doctest, 21 explicitly
  obsolete tests ignored. Two additional test-only cases were then added for
  shared aggregate/loop work and the 256-node limit; `cargo test --test arithmetic`
  passed all 10 arithmetic tests. The resulting suite has 618 passing tests.
- `cargo fmt --check`, `git diff --check`: passed.
- `cargo build --release --locked --offline`: built the optimized compiler.
- `scripts/check-arithmetic.py`: 40 integer results matched Python's independent
  arithmetic, and 11 invalid expressions failed with the expected diagnostic.
  Initial harness runs used the old release executable and then an incorrect
  float target for integer failure fixtures; neither was counted as a pass.
  The final run used compiler 1.4.0 with correctly typed fixtures.
- `npm test` in `editors/vscode`: 102 passed, zero failed.
- Real VS Code 1.121.0 and 1.92.0 integration runs passed, including syntax help,
  computed value 420 and compiler-owned field references inside arithmetic.
  Existing rename, diagnostics, formatting and value-hover gates also passed.
  Optional legacy compiler host coverage was skipped (no legacy path supplied).
- `npm run package`: produced `abstract-language-1.7.0.vsix`. Packaged metadata
  and changed source payloads were compared with the working source before
  installation. Installed id: `antonio-in-stem.abstract-language@1.7.0`.
- All six delivered solutions compile byte-identically to their recorded JSON.
  Existing example changes contain only compiler-version metadata updates.
- The existing documentation golden test now includes all six exercise
  solutions, excludes starters, and passed with `cargo test --test docs_examples`.

The focused adversarial review identified and fixed lossy mixed calc numeric
comparisons, nested call depth admission and a global lexer compatibility
regression. Arithmetic dependencies remain checked even in untaken branches.
This is automated engineering evidence, not independent human review.

## Measured performance

Command: `python scripts/benchmark-arithmetic.py`, serial, optimized Windows
build on AMD Ryzen 5 7600. Each sample starts a fresh process, compiles nested
invoice line products and their sum, and captures the full JSON. One warmup
and five measured samples per size; every total is checked independently.

| Invoice lines | Median complete compile | JSON bytes |
|---:|---:|---:|
| 100 | 7.63 ms | 10,774 |
| 1,000 | 14.84 ms | 105,299 |
| 5,000 | 42.60 ms | 525,408 |

Raw samples: [benchmark-windows.json](evidence/arithmetic/benchmark-windows.json).
These are end-to-end measurements on one machine, not isolated operator timings
or a universal latency guarantee. Host logs remain in Covenant
`out/abstract-editor-1.7.0-*.log`. No production source changed after the build;
the final additions were tests and documentation.
