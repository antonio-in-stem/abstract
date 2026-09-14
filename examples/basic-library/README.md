# Basic library: a tiny bookshelf

This is a first Abstract project: one schema and two book records. It models a
personal shelf without references, cloning, loops, versions, or assets. The
small surface makes the relationship between a template and its instances easy
to see.

The source files are:

- [`data/templates/Book.abt`](data/templates/Book.abt) — the `.abt` template.
- [`data/books/shelf.ab`](data/books/shelf.ab) — the `.ab` instances.

## 1. Read the template

`Book.abt` declares one `Book` schema. Each field has a type and, where useful,
a constraint or default:

- `title` and `author` are bounded text values.
- `status` is an enum. Only `want_to_read`, `reading`, and `finished` are valid.
  Its default is `want_to_read`.
- `pages` is an integer from 1 through 2000.
- `favorite` is a boolean that defaults to `false`.

The template is the contract. The `.ab` file supplies values; it does not
invent new fields or enum members.

## 2. Read the instances

Each record starts with `Book :: @id.<name>`. The `Book` name selects the
schema, and `@id` gives the record its stable identifier. The compiler uses the
schema to validate every following field. `the_hobbit` supplies every field;
`a_wizard_of_earthsea` omits `favorite`, so the schema's `false` default is
emitted.

## 3. Compile it

From the Abstract repository root, replace `ABSTRACT` with the shipped compiler
path if yours differs:

```powershell
$ABSTRACT = "target/release/abstract.exe"
& $ABSTRACT compile examples/basic-library JSON
& $ABSTRACT compile examples/basic-library YML
& $ABSTRACT compile examples/basic-library RAW
```

The checked-in output files are the baseline results of those commands:

- [`expected.json`](expected.json) is convenient for programs.
- [`expected.yml`](expected.yml) is the same compiled document in YAML.
- [`expected.raw`](expected.raw) is the review-oriented RAW form.

The compiler writes UTF-8 text with no byte-order mark. All three outputs carry
the same two records and the same defaulted `favorite` value. To capture a
fresh output for comparison, pass `--out` to a scratch path outside these
golden files.

## Try one deliberate error

In `data/books/shelf.ab`, temporarily change the second record's status to
`status: misplaced`, then compile the project again:

```powershell
& $ABSTRACT compile examples/basic-library JSON
```

Compilation fails because `misplaced` is not one of the enum values declared by
`Book.abt`. Restore `status: reading` and the project compiles again. This is
the key lesson: enum validation comes from the `.abt` template, not from a
convention in the data file.
