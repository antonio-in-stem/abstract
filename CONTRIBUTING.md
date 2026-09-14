# Contributing

Keep language changes small and explicit. Describe the input, expected compiled
output or diagnostic, and compatibility impact in an issue or pull request.
The specification in `docs/SPEC.md` is the language contract; examples should
compile with the documented compiler version.

## Verify a change

```sh
cargo test --locked
cargo build --release --locked
cd editors/vscode
npm ci
npm test
npm run test:integration
npm run package
```

Extension tests use the debug compiler built by Cargo, or the executable in
`ABSTRACT_COMPILER_PATH`. Host tests launch a separate VS Code profile. Use
`ABSTRACT_VSCODE_PATH` to select an installed host; otherwise the runner uses
the minimum supported version.

Add a regression test for changed behavior. Keep tests deterministic and use
synthetic data and temporary resources. Do not commit credentials, private
project data, local machine paths or generated packages.

Compiler, language and extension versions are separate. Update the relevant
changelog and contract when behavior changes. See `docs/RELEASE.md` for release
checks. Contributions to code are under MIT; branding follows `BRAND.md`.
