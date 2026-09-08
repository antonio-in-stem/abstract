# Unsaved compiler diagnostics: validation ledger

Baseline: `0ebfcdba96c34cf9f57d9aa627f08919f96de724` on
`antonio-in-stem/abstract-vscode-intelligence`. This ledger describes the
follow-up to extension 1.1.0, verified on 2026-09-07 local time
(2026-09-08 UTC). Extension version: **1.2.0**. The compiler still identifies as
1.0.1; support is negotiated through the separate **analysis protocol 1**.
This is not a new language version or a claim of complete language-server support.

## Outcome and boundaries

Dirty file-backed instances and schemas are analyzed together by the Rust
compiler, with its normal asset checks and original project paths. The editor
does not copy an asset tree, write the source snapshot, or implement replacement
language validation. Existing compile/lint CLI behavior remains covered by the
existing tests. Older compilers explicitly provide saved-file diagnostics.

The public wire contract is in [ANALYSIS-PROTOCOL.md](../ANALYSIS-PROTOCOL.md).
Ownership and verification are split as follows:

| Boundary | Owner and contract | Executed evidence |
| --- | --- | --- |
| Editor text → stdin | JS frames exact UTF-8 text, rejects unpaired UTF-16 surrogates and protocol limits | Real request/response tests; malformed, truncated, trailing, duplicate and oversized inputs |
| Canonical path → source membership | Rust validates absolute paths, canonical duplicates, discovered links and eligible new sources | Foreign/ignored rejection, new source with no disk write, actual junction fixture |
| Snapshot → compiler phases | Shared project discovery substitutes overlays before disk UTF-8 decoding; unchanged compile pipeline checks schemas, instances and assets | Invalid UTF-8 disk schema replaced in memory; dirty schema and instance; present/missing real relative asset |
| Compiler position → editor range | Rust converts scalar points to UTF-16, using the analyzed text | Unit and CLI cases plus actual VS Code emoji ranges, disk BOM, typed BOM and CRLF |
| Request → publication | Each canonical project owns a generation and diagnostic entries; request ID and document versions must match | Rapid invalid/valid edits; obsolete results cannot restore old errors; two-project diagnostics and shared schema consumers |
| Compatibility → saved validation | Capability probe distinguishes real old E801 from tooling failure; dirty member blocks legacy lint and saved-only Compile | Preserved pre-protocol executable in both supported host versions; dirty shared schema prevents Compile |
| Workspace → process | Trust checked before probes/commands; argv without shell; cancellation targets direct child | Actual Restricted Mode hosts; process termination test; implementation review |

No Rust dependencies were added. This increment adds `src/analysis.rs`, small
source/discovery hooks, and the optional `analyze` command. The authoring model,
grammar and existing refactor algorithm are unchanged.

## Executed checks

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --bin abstract` | Passed |
| `cargo build --release --bin abstract` | Passed |
| `cargo test --quiet` | **548 passed, 0 failed, 21 ignored** |
| Explicit conformance corpus output | **301 cases: 292 expectations passed, 9 without an expectation, 0 failed, 0 pending** |
| `npm test` | **26 passed, 0 failed, 0 skipped, 0 canceled** |
| VS Code 1.92.0, trusted, real legacy binary supplied | **5 scenario groups passed, 0 skipped** |
| VS Code 1.121.0, trusted, real legacy binary supplied | **5 scenario groups passed, 0 skipped** |
| VS Code 1.92.0, actual Restricted Mode | **1 scenario group passed** |
| VS Code 1.121.0, actual Restricted Mode | **1 scenario group passed** |
| `npm audit --json` | 0 reported vulnerabilities; no runtime dependencies |
| `npm run package` | VSIX built, 20 archive entries |
| `git diff --check` | Passed |

The Rust total is 445 library tests, 26 binary tests, 43 CLI tests, 28 retained
compiler regression tests, four conformance harness tests, one documentation
example runner and one doctest. The 292 conformance expectations run inside a
harness test; they must not be added to 548 as if they were separate Cargo tests.
The 21 ignored cases in `tests/compiler_tests.rs` are inherited 0.2.0 assertions
explicitly marked invalid under 1.0, with specification references. This work
does not add or silently enable those obsolete expectations.

The nine corpus cases without expectations are `cli-project/cli-08`,
`docs-tests-examples/dte-05`, `dte-06`, `dte-27`,
`media-bundle-crypto/mbc-02`, `mbc-03`, `mbc-08`, `mbc-09`, and `mbc-14`.
They are evidence gaps, not passing golden comparisons.

One fixture repair was required: `docs-tests-examples/dte-29` explicitly needs
an empty `data/` directory. Git had not preserved that directory, changing its
expected E103 into E806 in a fresh checkout. An ignored-by-discovery `.gitkeep`
now preserves it. No expected diagnostic or language rule was changed.

There were **four successful final host runs and twelve scenario groups**.
Trusted runs exercise contextual providers, the existing compiler-vouched
refactor, dirty schema indexing, live schema/instance analysis, relative assets,
superseded requests, independent projects, a canonical schema linked into two
projects, saved-only Compile, real Unicode ranges, and real legacy fallback.
Restricted runs assert `workspace.isTrusted === false`, retain completion, and
suppress compiler diagnostics/commands. These are grouped integration scenarios,
not twelve statistically independent trials. No screenshots or human usability
measurements were collected.

The official `@vscode/test-electron` `runTests` helper unconditionally adds
`--disable-workspace-trust`; it cannot establish Restricted Mode evidence.
The runner therefore uses the official download helper but launches the test
host directly, with isolated profiles, explicit argv and a real trust setting.
Hosts exited successfully. Logs contain environment-level mutex/Jump List/chat
registry messages; they are not counted as failed extension assertions.

## Reproduction and artifacts

From the repository root:

```powershell
cargo fmt --check
cargo build --bin abstract
cargo build --release --bin abstract
cargo test --quiet
cargo test --test conformance conformance_corpus -- --nocapture
Set-Location editors/vscode
npm ci
npm test
$env:ABSTRACT_LEGACY_COMPILER_PATH = '<preserved pre-protocol executable>'
$env:ABSTRACT_TEST_RESTRICTED = '0'
npm run test:integration
$env:ABSTRACT_TEST_RESTRICTED = '1'
npm run test:integration
```

Unset `ABSTRACT_VSCODE_PATH` to use downloaded 1.92.0. Set it to the desired
installed executable and repeat both trust branches for another version. The
legacy binary must be built from the baseline in a separate checkout or
preserved before replacing the local build. Without that environment variable,
the trusted host reports an explicit compatibility skip.

Local deliverables relative to the checkout root:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `editors/vscode/abstract-language-1.2.0.vsix` | 33,547 | `36e5131dba3cea5d2a5889dd49350a1950c8e23c494055eb7eb3f41a7980f3de` |
| `target/release/abstract.exe` | 1,383,424 | `e26e22c2b521e461442a8b5e4062b282a0b5e44247bcec891f35021dd8a97437` |
| `target/legacy/abstract.exe` (pre-protocol) | preserved local binary | `f2dfb1e162aae5425fabc8c7475879e2142e2728ab2a99b017f3fb4e4cebbe95` |

The VSIX contains no compiler binary or production Node dependencies. Packaging
does not install into the user's regular VS Code profile. The archive and build
binaries are ignored local artifacts. Final local logs are
`target/analysis-cargo-test.log`, `target/analysis-conformance.log`,
`target/analysis-node-test.log`, and the four
`target/analysis-host-{minimum,current}-{trusted,restricted}.log` files.

The coordinator reviewed the framing, canonical discovery, shared-source membership,
UTF-16 conversion and publication guards, and repeated all 26 Node tests successfully.
Copies of all seven raw test/host logs and their hashes are retained in
[the repository evidence directory](evidence/vscode-analysis/manifest.json).

## Performance evidence

Command: `node test/analysis-performance.js` from `editors/vscode`, after the
release build. The committed [raw observation file](vscode-analysis-performance.json)
contains the compiler SHA-256, baseline revision, `workingTreeDirty: true`,
timestamp, environment and all twenty observations in acquisition order.
This means baseline plus the reviewed working-tree changes, not a claim that
the baseline commit alone produces this behavior or measurement.

Environment: Node v24.13.1, Windows 10.0.26200 x64, AMD Ryzen 5 7600. Synthetic
fixture: 501 sources, 500 instances, one actual asset and one source overlay.
Five warm-up iterations precede twenty measured sequential requests. Each
request performs canonical client discovery and encoding, starts a new release
compiler, runs full discovery/validation/asset checks, and parses the response.
Percentiles use the nearest-rank convention on a sorted copy of the observations.

| Component | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Client membership discovery + encoding | 64.110 ms | 74.756 ms | 74.788 ms |
| Compiler process + validation/assets + response parsing | 112.677 ms | 162.415 ms | 181.358 ms |
| Complete measured request | 178.600 ms | 234.902 ms | 241.698 ms |

The complete request excludes the 250 ms edit debounce, capability negotiation,
VS Code document snapshot/RPC/rendering, and untimed fixture construction. The
OS cache is warm. Background machine activity was not isolated. Twenty
sequential observations are not twenty independent trials, confidence intervals
or a population latency estimate. The earlier authoring-model microbenchmark
does not include this compiler work and must not be substituted for it.

## Remaining limits

- Analysis is one bounded request per process, not an incremental compiler or
  LSP server. Disk discovery still occurs in both the editor and compiler.
- Input/output bounds do not bound disk-project size or compiler heap use.
  Requested process timeouts and direct-child cancellation are not absolute
  deadlines for arbitrary wrappers or descendants retaining inherited pipes.
- Other disk sources and assets are not an atomic filesystem snapshot.
  Watchers cover workspace folders; external disk changes outside watcher
  coverage require another editor event or explicit analysis request.
- Untitled buffers lack filesystem identity. New overlay sources require an
  existing parent. Conflicting dirty aliases of one canonical source are
  rejected as duplicate overlays, rather than choosing one buffer silently.
- During checking, old project diagnostics are cleared. An empty list while
  the status says Checking is not a completed validation result. Discovery
  failures return `analyzed: false`; truncated results are explicitly labeled.
- Ranges cover the compiler's offending scalar point, not a complete token.
  Windows hosts were exercised; other operating systems need host evidence.
- Formatting, general refactors/compression, tuple/tag completion, rename,
  references and version-aware hover remain separate language-coverage work.
  Human authoring trials and independent review remain necessary usability
  evidence; automated tests do not replace them.
