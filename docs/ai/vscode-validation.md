# VS Code authoring increment: validation ledger

Baseline: Abstract `main` at `11285de`, compiler 1.0.1. Extension version: 1.1.0.
The compiler runtime and language specification are unchanged. This is an
increment toward complete editor support; the extension README states the
remaining capabilities.

The follow-up [analysis protocol validation ledger](vscode-analysis-validation.md)
records the compiler changes and unsaved-diagnostics coverage in extension 1.2.0.
The observations below remain the historical 1.1.0 baseline.

## Contracts and evidence

| Boundary | Contract | Verification |
| --- | --- | --- |
| Disk discovery → source snapshot | SPEC 2.3–2.4 layout, canonical link de-duplication, ignored directories | Real temporary filesystem, case variants, junction cycle, ignored sources |
| Source snapshots → editor model | UTF-16 locations, normalized field/instance IDs and exact schema names; current buffers replace disk | Model tests; unsaved-schema test in a real extension host |
| Model → completion/navigation | Resolve the active schema and nested field scope; never traverse a list assignment path | Model tests plus VS Code provider commands |
| Refactor → source edit | SPEC 5.4 body-prefix equivalence; preserve value/comment/annotation bytes | Compile before/after and compare stdout bytes for JSON, YML, RAW across a three-version fixture |
| Compiler stderr → diagnostics | Stable codes, related locations, per-project ownership; stale runs cannot publish | Real compiler diagnostics and two projects in a real extension host |
| Workspace → compiler process | Argument vector without shell; trusted workspace and saved buffers; 30 s/4 MiB process limits | Implementation inspection; integration command path |
| Packaging → VSIX | Source, manifest, grammar, themes and icons; no runtime Node dependencies | `vsce package --no-dependencies` file inventory |

The editor model is a tolerant structural index, not a second validator. It does
not infer version applicability or execute logic. Compiler-vouched refactoring
coverage currently means the single specified rewrite and the executed fixture;
it is not evidence for arbitrary transformations.

## Executed checks, 2026-09-07

- `cargo build --bin abstract`: passed, compiler 1.0.1 built from the baseline.
- `npm test`: 16 cases passed, 0 failed, 0 skipped. Tests include the actual
  TextMate/Oniguruma grammar and compiler byte comparison, not mocked outputs.
- Real VS Code 1.92.2 and 1.121.0 extension hosts: passed completion, definition, hover,
  outline, semantic tokens, dotted-path edit, unsaved schema indexing and
  isolation of compiler diagnostics across two projects. The profile and files
  were temporary. No screenshot evidence was collected.
- `npm audit`: 0 reported vulnerabilities after compatible development
  dependency updates. There are no production dependencies.

## Performance experiment

Command: `npm run benchmark`. Node v24.13.1, Windows 10.0.26200 x64,
AMD Ryzen 5 7600. Synthetic fixture: 501 source files, 50 schemas, 1,000 declared
fields and 500 instances. Ten warm-up requests followed by 90 measured requests;
each request alternates an unsaved buffer and computes a fresh snapshot and
completion list. No network or compiler work is included.

| Measurement | Observed |
| --- | --- |
| One cold filesystem discovery/index build | 133.120 ms |
| Changed-buffer snapshot + completion p50 | 0.397 ms |
| Changed-buffer snapshot + completion p95 | 0.590 ms |
| Changed-buffer snapshot + completion max | 0.674 ms |
| Process heap used at end | 6.79 MiB |

These are local observations, not platform guarantees. The hot measurements use
the cache and exclude VS Code RPC/rendering. The single cold observation is not
a cold-start percentile. Filesystem refresh, larger projects, multiple open
buffers, diagnostics cost and sustained extension-host responsiveness need
additional distributions before making a whole-editor latency claim.

## Next evidence needed

1. A compiler-owned overlay/analysis protocol for diagnostics on unsaved buffers,
   with stable structured ranges and cancellation. Avoid copying entire asset
   trees or silently changing asset resolution in a temporary compile.
2. Compiler-vouched refactor corpus for each proposed shorthand (tuple rows,
   tags, multi-path assignments, wildcards), including versions, defaults,
   clones and interpolation. Formatting needs a token-preserving contract.
3. Language-server coverage for tuple/tag contexts, logic paths and variables,
   references/rename, asset paths and version-aware hovers. Every new rule must
   cite SPEC/grammar and have positive and adversarial compiler examples.
4. Independent human authoring/review trials with task completion/error metrics;
   automated integration tests do not substitute for usability evidence.
