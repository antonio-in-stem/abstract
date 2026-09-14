# Abstract editor authoring validation — 2026-09-14

## Scope and artifacts

This record covers the Abstract VS Code extension 1.5.0 and release compiler
1.3.0 in this checkout. It records reproduced commands and observations; it is
not a claim about Visual Studio Marketplace publication or every future host.

The validated extension identity is `antonio-in-stem.abstract-language`. The
package uses Antonio M. as its author and uses the official Abstract product
art as `editors/vscode/icons/abstract-logo.png`. The file theme uses native SVG
glyphs for `.ab` and `.abt`.

## Unit and compiler-backed tests

From `editors/vscode`, with the final release compiler selected:

```powershell
$env:ABSTRACT_COMPILER_PATH = 'C:\Users\PC\Repositories\covenant\out\abstract-work\target\release\abstract.exe'
npm test
```

Result: **88 passed, 0 failed** in 2.027 seconds. The suite included:

- dirty source overlay analysis, cancellation, UTF-16 ranges, discovery and
  resource bounds;
- compiler byte-equivalence for formatting and accepted dotted-path rewrites
  in JSON, YAML/YML and RAW output;
- tuple, tagged-object, asset and inferred loop completions;
- compiler-bound schema, field, explicit instance and loop references/rename,
  including collision, stale snapshot, linked-source and source-growth checks;
- effective-value projection over compiler base/overlays, preserving the exact
  `9223372036854775807` and `-0.0` number lexemes.

## Real VS Code extension hosts

Actual installed VS Code 1.121.0:

```powershell
$env:ABSTRACT_COMPILER_PATH = 'C:\Users\PC\Repositories\covenant\out\abstract-work\target\release\abstract.exe'
$env:ABSTRACT_VSCODE_PATH = 'C:\Users\PC\AppData\Local\Programs\Microsoft VS Code\Code.exe'
npm run test:integration
```

Minimum supported VS Code 1.92.0:

```powershell
Remove-Item Env:ABSTRACT_VSCODE_PATH -ErrorAction SilentlyContinue
$env:ABSTRACT_COMPILER_PATH = 'C:\Users\PC\Repositories\covenant\out\abstract-work\target\release\abstract.exe'
npm run test:integration
```

Both hosts passed the integration assertions for completion, navigation,
hover, formatting/refactoring, live multi-project diagnostics, dirty asset and
schema overlays, association-conflict recovery, UTF-16/BOM handling, and
semantic references/rename. The regression fixture opened `.ab` as Swift,
used the explicit workspace recovery command, edited `power` to `101` without
saving, and observed compiler diagnostic **E413** while disk bytes remained
unchanged. Effective-value hover observed a compiler-derived `"standard"`
value, `legacy: 7` for versions 1–2, and field absence at version 3. Schema
hover showed its declared default.

The isolated host profiles were temporary and were disposed after each run.
The test output was captured in the Codex task transcript; no persistent host
log file was created.

## Public inventory host

The complete public-inventory host gate used the release compiler and an actual
pre-protocol-feature 1.1.0 binary:

```powershell
$env:ABSTRACT_COMPILER_PATH = 'C:\Users\PC\Repositories\covenant\out\abstract-work\target\release\abstract.exe'
$env:ABSTRACT_PUBLIC_OLD_COMPILER = 'C:\Users\PC\Repositories\covenant\out\research\abstract-public-inventory-20260908-01\baseline-01\abstract-1.1.0.exe'
npm run test:integration:public
```

Result: all **10** host assertions passed on VS Code 1.92.0, including dirty
source admission without writes, stale-navigation refusal, linked canonical
sources, separate export diagnostics, empty exposure, and refusal to present a
saved-source fallback as live inventory with the older compiler.

## Serial performance observations

The existing static authoring benchmark ran alone:

```powershell
npm run benchmark
```

Environment: Node v24.13.1, Windows 10.0.26200 x64, AMD Ryzen 5 7600. Fixture:
501 files, 50 schemas, 1,000 fields and 500 instances. One cold index took
139.096 ms. After 10 warmups, 90 changed-buffer snapshot plus completion
samples measured p50 **0.466 ms**, p95 **0.635 ms**, maximum **1.216 ms**;
reported heap use was 8.63 MiB.

The representative automotive compiler measurement used the dated delivery
project and release 1.3.0 executable. The exact Node harness below sent a valid
zero-overlay `ABANLZ01` frame to a fresh process for each observation. It ran
one warmup and seven serial samples per mode:

```powershell
node -e 'const cp=require("node:child_process"); const exe="C:/Users/PC/Repositories/covenant/out/delivery/abstract-20260914/abstract-compiler-1.3.0.exe"; const root="C:/Users/PC/Repositories/covenant/out/delivery/abstract-20260914/automotive"; const input=Buffer.alloc(16); input.write("ABANLZ01","ascii"); input.writeUInt32BE(77,8); input.writeUInt32BE(0,12); const run=(mode)=>{const start=process.hrtime.bigint(); const result=cp.spawnSync(exe,["analyze",root,"--stdio",mode],{input,maxBuffer:8*1024*1024,windowsHide:true}); const ms=Number(process.hrtime.bigint()-start)/1e6; if(result.status!==0) throw new Error(result.stderr.toString()); const value=JSON.parse(result.stdout); if(!value.analyzed||(mode==="--symbols"&&!value.bindings?.complete)||(mode==="--values"&&!value.values?.complete)) throw new Error("incomplete representative response"); return {ms,bytes:result.stdout.length,value};}; const measure=(mode)=>{run(mode); const samples=Array.from({length:7},()=>run(mode)); const sorted=samples.map(v=>v.ms).sort((a,b)=>a-b); const first=samples[0]; return {warmup:1,samples:7,medianMs:Number(sorted[3].toFixed(3)),minMs:Number(sorted[0].toFixed(3)),maxMs:Number(sorted[6].toFixed(3)),responseBytes:first.bytes,...(mode==="--symbols"?{sources:first.value.bindings.sources.length,symbols:first.value.bindings.symbols.length}:{instances:first.value.values.document.data.length,overlays:first.value.values.document.overlays.length})};}; process.stdout.write(JSON.stringify({compiler:"1.3.0 release",project:root,symbols:measure("--symbols"),values:measure("--values"),scope:"Serial fresh processes; each sample includes process startup, discovery, analysis and response serialization."},null,2)+"\n");'
```

| Mode | Median | Minimum | Maximum | Response/context |
| --- | ---: | ---: | ---: | --- |
| `--symbols` | 42.285 ms | 40.716 ms | 43.176 ms | 278,504 bytes; 9 sources; 134 symbols |
| `--values` | 34.330 ms | 33.264 ms | 35.314 ms | 52,725 bytes; 8 instances; 2 overlays |

These timings include process startup, discovery, analysis and response
serialization. They used a warm operating-system cache, are sequential
observations on one machine, and are not population estimates or attacker-cost
evidence. The static benchmark excludes VS Code RPC/rendering and compiler
lint. No Cargo task or other CPU benchmark ran in parallel.

## Intentional fail-closed boundaries

Effective-value hover requires a valid whole-project snapshot and compiler
capability `values: 1`; project errors and older compilers retain declaration
hover without presenting a partial effective value. Compiler-incomplete dynamic
field identities are not renameable. Implicit file-stem instance IDs navigate
and participate in references but require an explicit `@id` before token-based
rename. Reference graphs remain scoped to one discovered project. Untitled
Abstract buffers prompt Save As and report **Save required** until they have a
project identity.
