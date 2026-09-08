# Schema references and Rename checkpoint

The 1.2.3 extension adds compiler-owned schema references and Rename. The
compiler exports an opt-in complete schema-binding graph; the provider validates
source identities and a freshly compiled candidate before returning edits.
Source admission covers saved, dirty and new documents, projected rename growth
and bounded discovery. Fields, instance IDs and loop variables are not covered.

After both resource-admission corrections, the complete Node suite passed
**52/52 with zero skips**. This is an additional final gate beyond the targeted
and isolated VS Code 1.92.0 runs documented in
[the validation report](vscode-schema-symbols-validation.md). It did not rebuild
Rust, reinstall a normal user profile or repeat benchmarks. Public F2 handoff
and concurrent filesystem limits in that report still apply.

The final VSIX has 42,252 bytes and SHA-256
`64aa8bf2f7cf044733b0c2890c14417cb0d9d77e2a0d3c387af39abc7551622d`.
The release compiler has 1,409,536 bytes and SHA-256
`f4f1a45e6d37e4e345160de49ebe1da0f4a22e1dba151e2186357136ddd2d8c5`.
The private Covenant research repository preserves the source snapshots,
compilers, packages, failed attempts and final logs at
`docs/research/evidence/2026-09-08-abstract-schema-symbols/`. Its manifest SHA-256 is
`303128d05f766b8ae7dede1583bc4fe7f6f2adfcadb50caee900a961124b7b2e`.
The archive's 123 ZIP members were compared with their original bytes; that
readback is evidence integrity, not an independent human evaluation.

The archive retains raw tested source bytes. Git follows this repository's
existing text normalization, so CRLF files enter the commit with LF endings;
that normalization does not rewrite the frozen package or evidence.
