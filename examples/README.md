# Learn Abstract through examples

The examples are grouped by purpose. Open one project folder in VS Code and
compile it before making changes. Each directory is an independent Abstract
project with its own `data/` tree.

## Tutorials

Follow the first three tutorials in order. The first two stay in the library
domain so each step can focus on new language concepts.

| Level | Project | What to learn |
| --- | --- | --- |
| Beginner | [A personal bookshelf](tutorials/beginner/README.md) | Separate a template from its instances; write text, integers, enums and defaults; read a validation error. |
| Intermediate | [A lending library](tutorials/intermediate/README.md) | Connect objects with references, reuse nested structures, and validate relationships with schema logic. |
| Advanced | [An automotive catalogue](tutorials/automotive/README.md) | Combine versions, overlays, clones, tuples, tagged lists, loops, asset checks and the broader language surface. |

The [product catalogue](tutorials/product-catalog/README.md) is a compact
additional tutorial for product variants, capabilities, release policy and
catalogue metadata.

Start with the selected tutorial's README, open its `.abt` templates, then read
its `.ab` data. The `expected.json`, `expected.yml` and `expected.raw` files are
reference outputs. They do not update automatically when you edit a project.

## Feature reference

The [feature projects](features/README.md) isolate individual syntax and
compiler behaviors in small, runnable directories. Use them when you want a
focused example of one feature or an exact expected output.

## Exercises

The [exercises](exercises/) provide starter projects and checked solutions for
hands-on practice.

## Why there are negative cases

A negative case is a small project that is intentionally invalid. Its test
passes only when compilation fails with the documented diagnostic and does
not emit a partial successful document. For example, a number outside its
declared range must produce `E413`; an undeclared enum member must produce
`E414`.

The automotive tutorial keeps these projects under `negative-cases/`, outside its main
`data/` directory. They demonstrate error messages and guard against a future
compiler change accidentally accepting invalid data. The beginner and
intermediate guides introduce deliberate mistakes as small, undoable exercises.

## A useful learning rule

Learn explicit fields and references before compact tuple, wildcard or clone
forms. Shorter source is valuable when its meaning remains easy to explain.
The automotive tutorial is a coverage example, not the recommended shape of a
first project.
