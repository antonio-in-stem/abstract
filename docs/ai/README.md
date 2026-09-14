# Abstract authoring for AI assistants

Use this directory when your task is to create or edit Abstract source files:
schemas and logic in `.abt` files, and instances in `.ab` files.

Read in this order:

1. [Writing Abstract](writing-abstract.md) for the authoring workflow and the
   mistakes most likely to make a valid-looking file fail.
2. The project's existing `.abt` files. They define the fields, types, enum
   members, defaults, version windows and rules that its `.ab` files must obey.
3. [Abstract 1.2 specification](../SPEC.md) when a construct is unclear. It is
   the normative language contract.
4. [Arithmetic in Abstract 1.2](../ARITHMETIC.md) before writing `calc(...)`.
5. [Grammar](../GRAMMAR.ebnf) when exact token boundaries or punctuation
   matter.

For examples, start with the mechanically checked
[smallest project](../examples/hello/) and then use the
[documentation example index](../examples/README.md) to find one feature at a
time. The separate [learning projects](../../examples/README.md) provide longer
guided examples. Copy a pattern only after reading the schema that gives it
meaning.

## Verify every authored change

Run these commands from the repository root, replacing `<project>` with the
project root, its `data` directory, or the relevant `.ab` files:

```sh
abstract lint <project>
abstract compile <project> JSON
```

`lint` validates without rendering a document. `compile` also lets you inspect
the actual values, defaults, ordering and version overlays. Use `--out <file>`
when you need a reviewable output file. Asset validation is part of a complete
check; use `--skip-assets` only when the assets are deliberately unavailable,
and say that the asset checks were skipped.

Do not claim that an Abstract edit works until the command exits successfully.
If it fails, use the diagnostic ID, source position and notes to correct the
source, then run the command again.
