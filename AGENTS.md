# Agent Guide

## Repository Scope

This repository is part of the m-project workspace.

Owns the Abstract language, compiler, CLI, documentation, examples, editor support, and public language site.

## AI Agent Workspace

Use docs/ai/ for agent-facing notes, decisions, operating context, and handoff material. Keep AI-oriented documentation factual and useful for maintainers.

## GitHub Workflow Standards

- Use the configured user identity for all commits: Antonio M. <yosoyantoniomartinez@gmail.com>.
- Do not attribute commits, branch names, pull requests, release notes, or repository history to AI systems.
- Use branches under the ntonio-in-stem/ prefix for work intended to be pushed.
- Do not push directly to main for repository changes. Create a pull request into main.
- When the requested work is complete and checks are acceptable, the pull request may be approved/merged by the repository owner as part of the workflow.
- Use m-project in new public-facing repository descriptions, README text, and planning docs.
- Keep source imports faithful. Avoid code edits during organization work unless a path/reference must be corrected for the new layout.

## Local Reference Material

Large third-party source snapshots, private binaries, and legacy reference material belong in .local-references/ when useful locally. That directory is intentionally ignored and should not be pushed.
