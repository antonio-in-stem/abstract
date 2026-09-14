# Release

The compiler and VS Code extension have independent versions. For the current
release, tag the compiler as `v1.4.0`; the same GitHub Release workflow attaches
the VS Code extension package at version `1.7.2`.

Before creating the tag, run the verification commands in
[CONTRIBUTING.md](../CONTRIBUTING.md). The tag workflow verifies
the compiler tag against `Cargo.toml`, builds and tests the compiler on Ubuntu
and Windows, tests and packages the extension on Ubuntu, then creates a draft
GitHub Release with the artifacts and `SHA256SUMS.txt`.

Before publishing the draft:

1. Confirm that the tag, compiler version and intended extension version match.
2. Check that the Windows, Ubuntu and extension jobs passed.
3. Verify downloaded artifacts against `SHA256SUMS.txt`, run each compiler,
   and install the VSIX in a clean VS Code profile.
4. Review installation instructions, compatibility changes and known limits in
   the release notes.

The workflow does not publish the extension to the Visual Studio Marketplace.
