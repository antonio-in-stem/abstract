# Abstract for Visual Studio Code

Write a family of objects without rewriting what they share.

Abstract separates the definition of an object from the values that make each
instance different. This extension brings that authoring workflow into VS Code:
schema-aware suggestions, explanations on hover, live validation, navigation and
checked refactoring for `.abt` definitions and `.ab` instances.

Compile the result as validated JSON, YAML or RAW for your application to use.

## Define once, vary what matters

Start with the possibilities for a product:

```abstract
schema Product {
    title: text(1..60)
    status: enum(draft, active, retired) = draft
    price: float(0.0..9999.0)
}
```

Then create concrete objects. A variant can reuse an existing object and state
only what changes:

```abstract
Product :: @id.starter
    title: Starter plan
    price: 12

Product :: @id.team
    &starter.*
    title: Team plan
    price: 29
```

Both objects remain subject to the same definition. Defaults, references,
constraints and compilation rules are resolved before Abstract produces the
output document.

## Author with the model in view

- Complete fields, enum values, references, nested structures and asset paths
  from the definitions in your project.
- Read syntax, constraints, defaults and evaluated values without leaving the
  file you are editing.
- Catch compiler errors against current editor buffers, including unsaved work.
- Go to definitions, find references and preview compiler-checked renames.
- Format indentation and use the optional Abstract file icons or Orbit Dark
  theme.
- Compile the current project to JSON, YAML or RAW from the command palette.

The extension also works as a useful editor without the compiler: highlighting,
syntax help and several authoring features remain available. Connect the compiler
for project validation, evaluated values, semantic navigation and compilation.

## Install

1. Obtain the Abstract compiler and the `.vsix` extension package from your
   Abstract release.
2. In VS Code, run **Extensions: Install from VSIX** and select the package.
3. Put the `abstract` compiler on `PATH`, or set **Abstract: Compiler Path** to
   its executable.
4. Open a project containing a `data/` directory and begin with an `.abt` or
   `.ab` file.

If you are new to the language, work through the
[six short exercises](../../docs/learn/README.md). For settings, workspace trust,
feature boundaries and development commands, see the
[extension authoring reference](docs/authoring.md).

The extension is by **Antonio M.** Its identifier is
`antonio-in-stem.abstract-language`.
