# Abstract authoring completion — 2026-09-14

Owner: Antonio M. Source baseline: `ffa9ad3`.

This delivery concerns Abstract, its VS Code extension and an automotive demonstration. Covenant runtime/protection work is outside this delivery.

## Acceptance

1. Opening a real Abstract project exposes compiler errors while editing, including an out-of-range value of 101 against 0..100. A conflicting file association must have a visible recovery path. User language preferences outside the project must not be silently rewritten.
2. Complete the documented authoring gaps: formatting; field, instance and loop navigation/references/rename; tuple and tagged-argument completion; asset paths; loop-variable inference. Operations must respect semantic scope, comments, strings, version windows and dirty documents.
3. Source compression must preserve compiled JSON, YAML and RAW bytes. A smaller source is useful only when it expresses the same document; an arbitrary globally minimal program is not an acceptance requirement.
4. Exercise the language through related automotive schemas and instances, with a feature-to-evidence matrix, real local assets, valid outputs and expected failures. Values, owners and brands are fictional. Market and steering position are modeled explicitly rather than treating all Europe as right-hand-drive.
5. Deliver generated Abstract branding and distinguishable .ab/.abt icons, wired into the extension. Attribute authorship to Antonio M. The existing Marketplace publisher identifier is separate from authorship; do not invent ownership of an unverified publisher account.
6. Build the compiler, test the extension in an actual VS Code extension host, package/read back the VSIX, install the delivered extension and open the automotive workspace with local compiler configuration. Preserve reproducible commands and results.

## Reproduced starting defect

The previously installed extension was 1.4.0. `abstract lint out/abstract-playground` reports E413 for `stats.power: 101`. The owner's VS Code settings associate `.ab` and `.abt` with Swift, so Abstract's language activation and providers do not run. The compiler is enforcing its range correctly. The playground workspace now explicitly associates these extensions with Abstract.

## Verification discipline

Compiler/schema implementation and editor implementation are separate workstreams. Build/test runs are coordinated. Failed experiments do not count as passing evidence. Previously obsolete pre-1.0 ignored tests are not silently reclassified as current passing tests. New limitations or unresolved failures remain visible in the final delivery record.

## References

- Language contract: [SPEC.md](../SPEC.md).
- VS Code language contribution and file association behavior: https://code.visualstudio.com/api/references/contribution-points#contributes.languages
- VS Code icon theme contribution: https://code.visualstudio.com/api/extension-guides/file-icon-theme
- Generated asset prompts: [PROMPTS.md](../../design/abstract-identity/PROMPTS.md).
