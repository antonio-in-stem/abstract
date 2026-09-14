# Abstract learning path

These six exercises teach one problem at a time. Each has a
starter project to edit and a separate `solution/` project with a short guide.
Compile a starter while you work; a deliberate error is part of the prompt.

Use compiler 1.4.0 or newer from the repository root:

```powershell
$ABSTRACT = "target/release/abstract.exe"
& $ABSTRACT compile examples/exercises/01-task-schema/starter JSON
```

The solution projects are the checked answers. Their `expected.json` files are
reference output and are not rewritten by the commands above.

| Exercise | Problem | Main idea |
| --- | --- | --- |
| [01 — Describe a task](../../examples/exercises/01-task-schema/README.md) | Make a task record valid | A schema is an object contract |
| [02 — Ship a package](../../examples/exercises/02-package-rules/README.md) | Accept only known states and safe weights | enums, ranges and defaults |
| [03 — Reuse a contact](../../examples/exercises/03-reusable-contact/README.md) | Apply one contact rule in two places | `$(Contact)` and `logic Contact` |
| [04 — Evolve a lamp](../../examples/exercises/04-versioned-lamp/README.md) | Keep version 1 and 2 data in one project | compiled base and overlays |
| [05 — Model a powertrain](../../examples/exercises/05-powertrain-variants/README.md) | Enforce electric and gasoline shapes | constrained variants with existing logic |
| [06 — Total an invoice](../../examples/exercises/06-invoice-arithmetic/README.md) | Calculate line totals and an invoice total | exact integer arithmetic and aggregation |

Try the starter first. Read its hint only when stuck, and open `solution/`
after making an attempt. Arithmetic comes last so you can focus on calculations
after learning shapes, constraints and composition. Keep starter and solution
as separate projects; compiling their parent directory together duplicates IDs.
