---
name: m-project-github-workflow
description: Use when working with m-project GitHub repositories, creating branches, commits, pull requests, merges, or repository documentation. Enforces owner-authored Git history, PR-based main changes, m-project naming in public docs, and repository-safe import conventions.
---

# m-project GitHub Workflow

## Identity

- Use Git user name Antonio M. and email yosoyantoniomartinez@gmail.com for commits unless the user explicitly changes the configured identity.
- Do not claim or imply that repository contents, commits, pull requests, or version-control history were authored by an AI system.
- Keep commit messages plain and maintainer-style.

## Branches and Pull Requests

- Create work branches using the antonio-in-stem/ prefix.
- Do not push directly to main for normal work.
- Push the branch, open a pull request into main, and complete the merge/auto-accept step when the user requests an end-to-end GitHub flow.
- Prefer concise PR titles that describe the repository change, not the tooling used.

## Repository Text

- In new README files, repository descriptions, planning docs, and public-facing metadata, refer to the umbrella as m-project.
- Do not introduce legacy umbrella branding in repository metadata. Existing source code or imported legacy files may retain their original package names, namespaces, or text.

## Import and Organization Work

- Copy source content faithfully.
- Do not edit imported code unless a path or reference must be corrected for the new layout.
- Keep generated dependencies and local state out of commits: node_modules, target, build, dist, .gradle, .kotlin, .vite, databases, logs, caches, and private binaries.
- Put local-only reference material in .local-references/, and keep it ignored.
- Keep agent-facing material in docs/ai/ or .ai/skills/.
