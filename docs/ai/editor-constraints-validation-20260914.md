# Editor constraint help and instance highlighting

Baseline: `b8bf1ef`. Extension release: 1.6.1. Compiler unchanged (1.3.0).

The schema default assignment operator now explains default validation and its
position before logic. Cardinality hover covers brackets, bounds and range dots,
and describes the actual minimum/maximum, including optional-field omission.
Numeric type constraints also provide help inside their parentheses.

Instance assignments distinguish field keys from bare values. Tuple columns and
cells have separate scopes, including multiline rows and nested body fields.
The scopes use conventional TextMate property, parameter and string categories;
the bundled Orbit Dark theme also maps bare values. User theme settings are not
changed. Text, enum and reference values share the bare-value scope: lexical
highlighting does not claim to infer their schema types.

Verification commands run from `editors/vscode`:

- `npm test`: 96 passed, zero failures. Real TextMate/Oniguruma tests cover
  nested fields, tuple columns/cells, numbers, booleans, comments, paths,
  interpolation and annotations. Hover tests cover default versus comparison
  and literal equals, all positions in cardinalities and numeric constraints.
- `npm run test:integration`, using the existing release compiler and installed
  VS Code 1.121.0: passed, including actual provider queries on default `=` and
  the numbers/dots in `[4..4]`, plus previous authoring regression checks.
- The same integration command with cached minimum VS Code 1.92.0: passed.
  Both host runs skipped the optional legacy compiler case (no legacy binary
  configured); it is unrelated to these syntax-only changes.
- `npm run package`: built `abstract-language-1.6.1.vsix` successfully.

Raw logs are retained locally in Covenant `out/abstract-editor-1.6.1-*.log`.
These are tokenization and extension-host checks, not a visual pixel test of
the owner's theme. Specific colors remain the active theme's choice.
