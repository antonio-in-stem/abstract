# Security reports

Report suspected security defects privately through the repository's
**Security → Report a vulnerability** page when available. Do not include
credentials or real customer data in a public issue. If private reporting is
unavailable, ask the maintainer for a private reporting channel without posting
exploit details.

Include the compiler/extension version, platform, a small synthetic reproducer,
expected behavior and observed result. Resource path escape, unbounded resource
consumption and incorrect handling of untrusted project files are relevant.

Use the current release. The compiler checks declared constraints; it is not a
sandbox for arbitrary downstream consumers or a complete image decoder. The
extension requires Workspace Trust before executing a configured compiler.
