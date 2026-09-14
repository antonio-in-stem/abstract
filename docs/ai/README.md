# Agent workspace notes

Notes for an assistant or agent working **inside this repository** rather than
merely writing Abstract source. If your task is to author `.ab` / `.abt` files,
read [`../ai-primer.md`](../ai-primer.md) instead; this page is about the repo.

---

## 1. What is normative

| File | Status |
|---|---|
| [`../SPEC.md`](../SPEC.md) | normative. Defines the language, the diagnostics and the output bytes. |
| [`../GRAMMAR.ebnf`](../GRAMMAR.ebnf) | normative. The complete grammar; SPEC refers to its rule names. |
| everything else under `docs/` | explanatory. Must agree with SPEC; when it does not, SPEC is right and the doc is a bug. |

Consequences:

- Do not "fix" behaviour by editing a guide. Trace the rule to its SPEC section
  first. If SPEC is wrong, that is a specification change, not a docs change.
- Every documentation claim should be traceable to a SPEC section. The guides
  cite section numbers on purpose; keep the citations correct when you edit.
- Error identifiers are stable across 1.x. Never reuse an identifier for a
  different condition, and never invent one that is not in SPEC 10.

---

## 2. Repository map

```text
abstract/                 the repository root
  Cargo.toml              the abstract-lang crate; zero dependencies, by design
  src/                    compiler core and CLI
  tests/                  compiler behaviour tests
    conformance/          the conformance corpus; cases/ holds the case trees
  docs/
    SPEC.md               normative specification
    GRAMMAR.ebnf          normative grammar
    abstract-language.md  the guide
    technical-reference.md  the lookup reference
    raw-data.md           the compiled-document contract
    ai-primer.md          how to write correct Abstract
    ai/README.md          this file
    examples/             every documented example, as a compilable project
  examples/               independent teaching projects and exercises
  editors/                editor support
  java/                   optional runtime for sealed containers
  scripts/                install scripts
  CHANGELOG.md            what changed in 1.0.0
  README.md               install, quick start, layout
```

---

## 3. The verification loop

```sh
cargo test                                  # compiler behaviour tests
abstract lint docs/examples/<name>          # a documented example must lint clean
abstract compile docs/examples/<name> JSON  # compare with expected.json
```

Rules of the loop:

- **Never claim a change works without running it.** If the binary is not
  available in your environment, say so and hand back the exact commands you
  would have run, not a guess about their result.
- **Quote diagnostics verbatim.** A diagnostic carries an identifier, a path, a
  line and a column. Paraphrasing loses all four.
- **Compare bytes, not shapes.** Output is byte-exact by specification. Comparing
  parsed structures hides indentation, key order and number-spelling bugs, which
  are all part of the contract.
- The one normalisation permitted when comparing a document with a stored
  expectation is the string value of `abstract.compiler`, which SPEC 11.2
  replaces with `0.0.0` on both sides.

---

## 4. Documentation examples

Every worked example printed in `docs/` or rendered on the site exists as a
project under [`../examples/`](../examples/README.md):

```text
docs/examples/<name>/
  data/             sources
  assets/           the assets directory (empty when unused)
  expected.json     the exact bytes of `abstract compile <name> JSON`
```

When you add or change an example:

1. Write the project first, compile it, and take the output from the compiler —
   never hand-write the JSON and hope.
2. Copy the same bytes into whichever document shows it. A fragment shown in a
   document must be a literal excerpt of a project that compiles, not an
   approximation.
3. Add a row to `docs/examples/README.md` recording what it covers and where it
   is used.
4. Keep files `LF`-terminated with exactly one trailing newline, and keep image
   assets small — the header probe reads at most 64 KiB and never decodes
   pixels, so a few hundred bytes is plenty.

An example that needs `--skip-assets` to compile is a broken example. Check the
asset in.

---

## 5. Website

The former website is maintained outside this repository. Language documentation
lives in docs/; release downloads are distributed through GitHub Releases.

---

## 6. Guardrails

- **Concurrency.** Several agents may work in this tree at once. Touch only the
  files your task names; if a file you need is being edited elsewhere, report
  the conflict instead of merging blind.
- **Zero dependencies.** The crate has no dependencies and no dev-dependencies,
  by design. Do not add one to solve a problem the standard library can solve.
- **Determinism.** Nothing in the compiler may read the clock, the environment,
  a random source or the network, and no output ordering may come from
  filesystem enumeration. A change that introduces any of those breaks SPEC 7.6
  even if every test still passes.
- **No panics.** Every failure is a diagnostic with an identifier and, where a
  position is known, a position. `unwrap` on anything derived from input is a
  bug (SPEC 3.7).
- **Public wording.** Documentation and site copy describe the language and a
  generic project. Use `m-project` as the placeholder project name; do not name
  a private consumer of the language anywhere in published text.
- **No addressed notes in deliverables.** Documentation is written for its
  readers. Status reports, hand-offs and questions belong in your reply, not in
  a file.

---

## 7. Fast answers to questions that come up constantly

| Question | Answer | SPEC |
|---|---|---|
| Where does an id come from? | `@id.x` in the header, else the file stem | 5.3 |
| Can a schema declare `id`? | Only as `id: text` or `id: text(a..b)` | 4.11 |
| What decides output key order? | Schema declaration order | 8.3 |
| What decides object order? | `(template, id)` | 2.7 |
| Does `--skip-assets` change output? | Never | 9.5 |
| Does `exists` touch the disk? | Never | 6.7 |
| Is there a null? | No, in any format, for any type | 8.9 |
| Is an absent optional list `[]`? | No, it is omitted | 4.6 |
| Which version is `data`? | The maximum of the project range | 7.5 |
| How many overlays may match a version? | Any number, including none | 7.5 |
| Is RAW machine-readable? | No; it is a review format | 8.6 |
| Is there a lenient mode? | No | 5.12 |
