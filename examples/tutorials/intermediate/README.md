# Intermediate library: a small lending library

This project adds the relationships that make a lending library useful while
staying small enough to read in one sitting. It has five schemas and eight
records: reusable `Contact` data, two typed authors, two typed members, two
books, and two loans.

The source files are:

- [`data/templates/Library.abt`](data/templates/Library.abt): five schemas and
  two logic blocks.
- [`data/people/people.ab`](data/people/people.ab): two `Author` and two
  `Member` records.
- [`data/books/books.ab`](data/books/books.ab): two `Book` records.
- [`data/loans/loans.ab`](data/loans/loans.ab): two `Loan` records.

## 1. Reuse a nested shape

`Contact` is a schema of its own. Both `Author` and `Member` embed it with
`contact: $(Contact)`, so the same `email` and `city` shape is reused for both
kinds of record. `email` is optional. `city` defaults to `Mexico City`, which
is why Mina can provide only the value that differs from the default. Separate
schemas also make the ref types precise: a `Book.author` must name an `Author`,
while a `Loan.member` must name a `Member`.

## 2. Follow references and lists

`Book.author: ref(Author)` and `Loan.book: ref(Book)` / `Loan.member:
ref(Member)` are checked references to other record ids and schemas. For
example,
`earthsea` points at `ursula_le_guin`, and `mina_earthsea` points at both
`earthsea` and `mina`. A missing id is a compile error.

`Book.tags[1..4]` is a bounded list of enum values. The source uses the compact
`tags: fiction, poetry` form. A book gets 1–4 tags, each chosen from the enum.
`format` and `state` show more enums; `format`, `copies`, `state`,
`loan_period_days`, `renewal_allowed`, and `renewal_count` also demonstrate
defaults.

## 3. Read the logic

The `Book` logic block derives the required `tag_count` field from the list with
`length(.tags)`. Authors do not type this value; the compiler derives it before
checking the completed record.

The one `Loan` rule captures a real library invariant: a loan marked
non-renewable cannot already have renewal history. The returned sample uses
`renewal_allowed: false` with the default count of zero.

## 4. Compile it

From the Abstract repository root:

```powershell
$ABSTRACT = "target/release/abstract.exe"
& $ABSTRACT compile examples/tutorials/intermediate JSON
& $ABSTRACT compile examples/tutorials/intermediate YML
& $ABSTRACT compile examples/tutorials/intermediate RAW
```

The checked-in outputs are the exact baseline results of those commands:

- [`expected.json`](expected.json)
- [`expected.yml`](expected.yml)
- [`expected.raw`](expected.raw)

All outputs are UTF-8 without a byte-order mark. They contain the same eight
records, with refs represented by their referenced ids and defaults/derived
values included in the compiled document. To capture a fresh output for
comparison, pass `--out` to a scratch path outside this example's three
goldens.

## Try one deliberate error

Temporarily add `renewal_count: 1` below `renewal_allowed: false` in Noah's
loan in `data/loans/loans.ab`, then compile:

```powershell
& $ABSTRACT compile examples/tutorials/intermediate JSON
```

The `Loan` logic rule rejects the record with the message about a
non-renewable loan having renewal history. Remove the temporary line to return
to the valid example. This exercise shows that logic can enforce a relationship
between two already validated fields.
