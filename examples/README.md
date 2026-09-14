# Learn Abstract through examples

Follow these projects in order. Each is independent: open one project folder in
VS Code, rather than combining their data directories. The first two use the
same library domain so each step introduces language concepts instead of a new
subject.

| Level | Project | What to learn |
| --- | --- | --- |
| Beginner | [A personal bookshelf](basic-library/README.md) | Separate a template from its instances; write text, integers, enums and defaults; read a validation error. |
| Intermediate | [A lending library](intermediate-library/README.md) | Connect objects with references, reuse nested structures, and validate relationships with schema logic. |
| Advanced | [An automotive catalogue](automotive/README.md) | Combine versions, overlays, clones, tuples, tagged lists, loops, asset checks and the broader language surface. |

Start with the README of the selected project, open its `.abt` template, then
its `.ab` data. Compile the unchanged project once before trying an edit.
`expected.json`, `expected.yml` and `expected.raw` are reference outputs, not
files that update automatically when you type.

Also available: [a product catalogue](product-catalog/README.md), combining
product variants, capabilities, release policy and catalogue metadata.

## Why there are negative cases

A negative case is a small project that is intentionally invalid. Its test
passes only when compilation fails with the documented diagnostic and does
not emit a partial successful document. For example, a number outside its
declared range must produce `E413`; an undeclared enum member must produce
`E414`.

Automotive keeps these projects under `negative-cases/`, outside its main
`data/` directory. They demonstrate error messages and guard against a future
compiler change accidentally accepting invalid data. The beginner and
intermediate guides introduce deliberate mistakes as small, undoable exercises.

## A useful learning rule

Learn explicit fields and references before compact tuple, wildcard or clone
forms. Shorter source is valuable when its meaning remains easy to explain.
Automotive is a coverage example, not the recommended shape of a first project.
