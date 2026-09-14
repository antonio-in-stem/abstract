# Release evidence record

Copy this template into a release PR or issue and complete it from the actual
run. Do not replace missing evidence with an assumption.

| Field | Recorded value |
| --- | --- |
| Release tag | |
| Commit SHA | |
| Compiler version (`Cargo.toml`) | |
| Extension version (`package.json`) | |
| Local `cargo test --locked` result | |
| Local release-build result | |
| Local extension unit-test result | |
| CI Ubuntu compiler run URL/result | |
| CI Windows compiler run URL/result | |
| CI extension unit-test run URL/result | |
| Tagged artifact workflow URL/result | |
| Linux artifact SHA-256 | |
| Windows artifact SHA-256 | |
| VSIX SHA-256 | |
| Isolated VSIX-install check | |
| Reviewer / date | |

The current release workflow creates GitHub Release attachments only. It does
not publish to the Visual Studio Marketplace and does not require a registry
token. Its GitHub token is supplied by Actions at runtime and is not printed by
the workflow.
