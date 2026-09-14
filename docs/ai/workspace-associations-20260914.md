# Exercise workspace highlighting correction

The delivered multi-root workspaces omitted root `files.associations` settings.
With a user-level `*.ab` / `*.abt` association to Swift, folder-local settings
did not select Abstract. This made email text appear as Swift syntax.

Both checked-in exercise workspaces now select Abstract at workspace scope.
The delivery workspaces were repaired without changing exercise data or global
VS Code preferences. Compiler 1.4.0 and extension 1.7.0 are unchanged.

Verification on 2026-09-14:

- `cd editors/vscode; npm test`: 104 passed, including the exact contact example
  and workspace paths/settings.
- `npm run test:integration`: passed in real VS Code 1.92.0. The fixture starts
  with global Swift and folder-only Abstract, confirms Swift, applies workspace
  Abstract, confirms the open document changes language, and retests the
  existing workspace recovery command.
- TextMate resolution against installed Duotone MC 1.0.0 produces `#E06C75`
  for instance keys and `#98C379` for complete bare text values. This checks
  token/theme resolution, not a screenshot of the user's live window.

No language quoting change, grammar rewrite or extension release was necessary.
