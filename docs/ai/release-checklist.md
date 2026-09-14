# Release checklist

This checklist is for a GitHub release of the reference compiler and the
bundled VS Code extension. It records checks that must be observed for the
specific tag; it is not evidence that an unrun platform or marketplace action
succeeded.

## Before creating the tag

- [ ] The intended compiler version in `Cargo.toml` is `1.4.0`.
- [ ] The intended extension version in `editors/vscode/package.json` is `1.7.2`.
- [ ] The release tag is `v1.4.0`, matching `Cargo.toml` exactly.
- [ ] `cargo test --locked` passes locally.
- [ ] `cargo build --release --locked --bin abstract` succeeds locally.
- [ ] `npm ci` and `npm test` pass in `editors/vscode`.
- [ ] `npm run package` produces `abstract-language-1.7.2.vsix`.
- [ ] The release notes name the independently versioned compiler and extension.
- [ ] No generated `target/`, `node_modules/`, local VSIX, credential, or customer material is staged.

## GitHub evidence

- [ ] The CI workflow has passed for both Ubuntu and Windows compiler jobs.
- [ ] The VS Code extension unit-test job has passed.
- [ ] The tagged release workflow has passed its compiler and extension artifact jobs.
- [ ] The draft GitHub release contains `abstract-linux-x64`, `abstract-windows-x64.exe`, the packaged VSIX, and `SHA256SUMS.txt`.
- [ ] Download each compiler artifact and check `abstract --version` reports `abstract 1.4.0`.
- [ ] Install the attached VSIX in an isolated VS Code profile and confirm the installed extension reports `1.7.2`.
- [ ] Publish the draft only after the recorded checks and release notes have been reviewed.

## Boundaries

The workflows build and test on their runner operating systems. A green run is
evidence for those hosted environments only; it does not establish support for
other platforms, Marketplace publication, long-term installer behavior, or a
security property.
