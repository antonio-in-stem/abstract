# Contributing

Keep language changes small and explicit. Describe the input, expected compiled
output or diagnostic, and compatibility impact in an issue or pull request.
The specification in `docs/reference/specification.md` is the language
contract; examples should compile with the checked compiler.

## Verify a change

```sh
python3 scripts/check-release-manifest.py
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
ABSTRACT_COMPILER_PATH="$PWD/target/debug/abstract" mvn -B -f java/pom.xml verify
cd vscode
npm ci
npm test
npm run test:integration
npm run package
```

Extension tests use the debug compiler built by Cargo, or the executable in
`ABSTRACT_COMPILER_PATH`. Host tests launch a separate VS Code profile. Use
`ABSTRACT_VSCODE_PATH` to select an installed host; otherwise the runner uses
the minimum supported version.

On Linux, run host tests with `xvfb-run -a npm run test:integration`. CI covers
the minimum supported VS Code and stable, packages the VSIX, runs Java tests,
and checks the compiler with both stable Rust and the declared minimum version.
On Windows, set `$env:ABSTRACT_COMPILER_PATH` to the built `abstract.exe` before
running Maven. Without this variable the Java suite skips the cross-language
test; CI always sets it.

Add a regression test for changed behavior. Keep tests deterministic and use
synthetic data and temporary resources. Do not commit credentials, private
project data, local machine paths or generated packages.

Compiler, language and extension values are separate. Update the release
manifest and relevant contract when behavior changes. See `docs/releasing.md`
for release checks and `docs/compatibility.md` for migration policy. Use Conventional
Commits, such as `fix(parser): reject an incomplete range`. Contributions to
code are under MIT.
