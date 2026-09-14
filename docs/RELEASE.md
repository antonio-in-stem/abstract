# Release

The compiler and VS Code extension have independent versions. For the current
release, tag the compiler as `v1.4.0`; the same GitHub Release workflow attaches
the VS Code extension package at version `1.7.2`.

Before creating the tag, run the commands and complete the evidence record in
[`ai/release-checklist.md`](ai/release-checklist.md) and
[`ai/release-evidence.md`](ai/release-evidence.md). The tag workflow verifies
the compiler tag against `Cargo.toml`, builds and tests the compiler on Ubuntu
and Windows, tests and packages the extension on Ubuntu, then creates a draft
GitHub Release with the artifacts and `SHA256SUMS.txt`.

It does not publish the extension to the Visual Studio Marketplace. Downloaded
artifacts still require the checks listed in the checklist before they are
announced as a release.
