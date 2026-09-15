# Release

The compiler, extension and Java reader have independent versions. The
[release manifest](../release-manifest.json) records the versions shipped
with a compiler tag. CI checks those values against the source.

## Prepare a tag

Run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md), including the minimum
Rust version, and review the [compatibility guide](compatibility.md). Merge only
after the required CI jobs pass. Use `v<compiler version>` as the tag; the release
workflow rejects a tag that disagrees with `Cargo.toml`.

The workflow builds and tests native binaries for Windows x64, Linux x64 and
ARM64, and macOS Intel and Apple Silicon. It tests the extension in the minimum
supported and stable VS Code hosts, packages a VSIX, and creates a draft GitHub
release with checksums and the compatibility manifest. A configured job is not
evidence that a platform passed; check the run for the release being prepared.

## Verify the artifacts

1. Confirm that the tag, manifest, compiler and intended extension versions match.
2. Check every native build, Java test and extension job for failures.
3. Before publishing the draft, locally verify its checksums, the Windows
   binary and the VSIX in a clean VS Code profile.
4. Publish the draft as a private prerelease, then dispatch **Verify downloaded
   release** with the tag. It downloads each native binary only on its matching
   platform, checks its named SHA-256 entry, manifest and version, then runs
   lint, compilation, bundle round-trip and tamper-rejection checks.
5. Promote the private prerelease only after all five verification jobs pass,
   then review installation instructions, migration steps and known limits.
   Record any unavailable platform checks in the release notes.

For a command-line dispatch, use the tag that is being released:

```sh
gh workflow run verify-release.yml --ref "$tag" -f tag="$tag"
```

In the GitHub UI, select the matching tag under **Use workflow from** before
running the workflow. Older releases retain their own workflow layout.

On Linux or macOS, run `sha256sum --check SHA256SUMS.txt` where available; on
macOS the equivalent is `shasum -a 256 --check SHA256SUMS.txt`. On Windows, use
`Get-FileHash -Algorithm SHA256 <downloaded-file>` and compare its digest with
the corresponding entry. Checksums detect corrupted downloads; trust also
requires obtaining the manifest and artifacts from the intended release.

The download-verification workflow uses only the selected artifact,
`SHA256SUMS.txt` and the release manifest in each job. The release must be
published as a private prerelease: a `contents: read` workflow token cannot
download draft assets.

## Distribution

The repository remains private. These workflows do not publish source or
packages to crates.io, Maven Central or the Visual Studio Marketplace.

The Cargo archive includes compiler source and a package README, not the editor,
website or full example corpus. CI rebuilds the extracted archive. Compiler
regression and conformance tests run from the repository, which contains their
fixtures. `cargo install --path . --locked` installs from an authorized checkout.
