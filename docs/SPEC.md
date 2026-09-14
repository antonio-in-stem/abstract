# Abstract 1.2 — Language Specification

Status: normative. This document defines the Abstract data language, version 1.2.
Version 1.2 adds explicit arithmetic expressions in logic through `calc(...)`;
the numeric contract, additional work charges and diagnostics are defined in
[ARITHMETIC.md](ARITHMETIC.md), a normative
companion to chapter 6. Document and overlay formats remain unchanged.
Version 1.1 adds the `@public` nomination modifier; existing data semantics and
document format remain unchanged. Its optional export profile is specified in
[PUBLIC-CONTRACT.md](PUBLIC-CONTRACT.md).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, MAY and OPTIONAL in this document are to be interpreted as described in RFC 2119.

A conforming implementation MUST accept every program this document declares valid, MUST reject every program this document declares invalid, and MUST produce exactly the output this document prescribes. Where this document prescribes an error identifier, a conforming implementation MUST report that identifier.

The companion file `GRAMMAR.ebnf` contains the complete grammar. Rule names written in `monospace` in this document (for example `path_assignment`) refer to rules in that file.

---

## 1. Overview and design principles

### 1.1 What Abstract is

Abstract is a schema-backed data language. A project consists of:

- **Template files** (`.abt`) that declare *schemas* (the shape and constraints of a class of objects), *logic* blocks (rules attached to a schema), and at most one *versions* declaration.
- **Instance files** (`.ab`) that declare concrete objects validated against those schemas.

A compiler reads both, validates every instance, evaluates logic, and emits one **compiled document** in JSON, YAML or RAW form. The compiled document is the only artifact consumers read; `.ab` and `.abt` files are the authored source.

### 1.2 Design principles

These four principles decide every open question in this document. When two readings of a rule are possible, the reading that better serves the earlier principle wins.

**P1 — Minimal.** The language has one way to express each thing. Features that can be expressed by composing existing features are not added. Abstract 1.0 has no imports, no free-key maps, no user-defined functions, no arithmetic, no nested lists, no per-schema version axes and no comments in output.

**P2 — Elegant.** Authored source is shorter and more readable than the JSON it compiles to. Common shapes (a required scalar, a keyed list, an asset path derived from the instance id) have compact spellings. Nothing in the surface syntax exists only to help the parser.

**P3 — Deterministic.** For a fixed set of input bytes, the compiled document is byte-identical on every machine, in every locale, under every filesystem enumeration order, and for every ordering of command-line arguments. Every ordering in the output is defined by this document, never by the environment. Compilation performs no network access and depends on no clock, no randomness and no environment variables.

**P4 — Strict.** Every construct is either defined by this document or an error with a stable identifier and a source position. There is no silent truncation, no silent coercion, no silently ignored token, no silently dropped statement, and no value that reaches the output without having been validated. When a program is ambiguous, it is rejected rather than resolved by a rule the author cannot see.

### 1.3 What compilation guarantees

A successful compilation guarantees all of the following:

1. Every emitted object was validated against a declared schema.
2. Every emitted field is declared by that schema (there is no strict/lenient mode switch in 1.0).
3. Every emitted `id` is unique across the project.
4. Every value has the type its schema declares: an `int` field is a JSON number with no fractional part, a `bool` field is a JSON boolean, a `text` field is a JSON string.
5. The compiled data does not depend on which validation checks were skipped: `--skip-assets` changes which errors are reported, never which bytes are emitted (§9.5).
6. The document is reproducible byte for byte (P3).

### 1.4 Document conventions

- Every example that shows compiled output shows the JSON form unless another form is named. JSON examples are the exact bytes the compiler emits, modulo the trailing newline, which is always present. A fragment written on one line to make a structure visible, and marked as a shape, is the exception: §8.4 fixes the exact bytes.
- Error identifiers have the form `E` followed by three digits. The first digit groups the error by chapter: `E1xx` files and projects, `E2xx` lexical, `E3xx` schemas, `E4xx` instances, `E5xx` logic, `E6xx` versions, `E7xx` output, `E8xx` command line. Identifiers are stable across 1.x releases: an identifier is never reused for a different condition.
- A JSON example that shows a single object rather than a whole document omits the envelope of §8.1; the object shown is exactly what appears inside `data`.
- An example that shows only part of a project — a schema without the schemas its `$(…)` and `ref(…)` types name, an instance without the instances its `ref` values resolve to, a logic block without its schema — assumes that the rest of the project is declared elsewhere and is valid. Only the construct under discussion is shown; every example is otherwise exactly what this document's rules produce.

---

## 2. Files and projects

### 2.1 File kinds and extensions

| Extension | Kind | Contents |
|---|---|---|
| `.abt` | template file | `versions_decl`, `schema_decl`, `logic_decl` |
| `.ab` | instance file | `instance` declarations |

Extensions MUST be compared case-insensitively (`Item.ABT` and `item.abt` are both template files). No other extension is a source file. A file whose extension is neither is never read, even when named explicitly on the command line (E806).

A template file MUST NOT contain instances; an instance file MUST NOT contain schemas or logic. Encountering `schema`, `logic` or `versions` at statement position in a `.ab` file, or an `instance_header` in a `.abt` file, is E210.

### 2.2 Encoding, byte-order mark, line endings

- Source files MUST be UTF-8. A byte sequence that is not valid UTF-8 is E102.
- A UTF-8 byte-order mark (`EF BB BF`) at offset 0 MUST be removed before lexing and has no other effect. A BOM anywhere else is an ordinary character and is E210 outside a quoted string.
- Line terminators are `LF` (`0A`) and `CRLF` (`0D 0A`). Both are one line terminator. A lone `CR` is E210. The last line need not be terminated.
- Line and column numbers in diagnostics are 1-based. A column counts Unicode scalar values, not bytes.

### 2.3 Project root, data directory, assets

Compilation starts from one or more *command-line roots*. For a root `R`:

1. Let `D` be `R` if `R` is a directory, otherwise the directory containing `R`.
2. Walk `D`, then its parent, then its parent's parent, up to the filesystem root. The first directory whose final component compares case-insensitively equal to `data` is the **data directory**. The walk goes **upwards only**: no directory below `D` is examined for the marker, and there is no search among `D`'s children.
3. If step 2 found no data directory **and** `R` is a directory, one further path is examined — and exactly one: the child of `R` whose final component compares case-insensitively equal to `data`. When it exists and is a directory, it is the data directory. When the filesystem presents more than one such child, that is E806 with `note: '{path}' contains more than one 'data' directory.`; nothing is chosen by sort order. This step is the project-root spelling of a command-line directory; it is never applied to the directory that contains a named file.
4. If a data directory was found, the **project root** is its parent and the **discovery root** is the data directory. Otherwise the project root and the discovery root are both `D`.
5. When `R` is a directory, `R` MUST be the data directory or the project root; any other directory is E806. This is what step 3 exists for, and it is the only restriction on which directories may be named.
6. The **assets directory** is `<project root>/assets`. It need not exist; it is consulted only for `file` and `image` validation (§5.9).

All roots given on one command line MUST resolve to the same project root; otherwise E807.

```text
pack/                 <- project root
  assets/
    textures/atlas.png
  data/               <- data directory, discovery root
    templates/Product.abt
    items/atlas.ab
```

`abstract compile pack` (the project root, resolved by step 3), `abstract compile pack/data` (the data directory, resolved by step 2 because `D` is itself the marker) and `abstract compile pack/data/items/atlas.ab` (resolved by step 2 from an ancestor) all resolve the project root to `pack/` **and** the discovery root to `pack/data/`, so all three compile the same source set.

`abstract compile pack/data/items` is E806 by step 5, with `note: name the project root or the 'data' directory.`: step 2 resolves the discovery root to `pack/data`, of which `pack/data/items` is neither the directory itself nor its parent, so the invocation would silently compile more than it names. Naming a `.ab` file inside a data directory stays legal at any depth: single-file mode selects output by file (§2.5).

A project that keeps sources outside a `data/` directory has no `data/` directory at all; the two layouts are never mixed. In that layout there is no upward marker to find, so the project root of a named `.ab` file is the directory that contains it: a project without a `data/` directory keeps its sources in one directory, or is compiled by naming its root. This is the only respect in which the two layouts behave differently.

### 2.4 Source discovery

The discovery root is walked recursively. During the walk:

- A directory MUST be skipped when its final component begins with `.` or compares case-insensitively equal to `node_modules`, `target`, `build` or `out`.
- Symbolic links and directory junctions MUST be resolved. The walker MUST keep a set of canonical directory paths already visited and MUST skip a directory whose canonical path is already in the set. This makes link cycles terminate and makes a file reachable by two paths appear once.
- A file is collected when its extension is `.ab` or `.abt` (case-insensitive) and its canonical path is not already collected. A collected file that cannot be read is E101.

Collected sources MUST be sorted by their project-root-relative path, with `/` as the separator, compared as a sequence of Unicode scalar values. This order is used for diagnostics and for nothing else; it does not determine output order (§2.7).

If discovery collects zero source files, compilation fails with E103. An empty directory is never a valid empty project.

### 2.5 Single-file and multi-file mode

When one or more `.ab` files are named on the command line:

- Discovery still collects the whole project (§2.4), so schemas, clone targets and `ref` targets resolve exactly as in a whole-project compile.
- The compiled document contains only the instances declared in the named files. All other instances are still parsed, still checked for duplicate ids, unknown templates and clone/ref resolution, but are not emitted.
- Naming a directory and a file in the same invocation is E807. Naming a `.abt` file is E806 (a template declares no instances, so it can never select output).

Compiled data for a given instance MUST be identical whether it was produced in whole-project mode or single-file mode. In particular, a clone source that is not itself emitted is still fully validated (§5.7).

### 2.6 Case rules, summarised

| Construct | Case sensitivity |
|---|---|
| File extensions (`.ab`, `.abt`) | insensitive |
| The `data` directory marker | insensitive |
| Ignored directory names | insensitive |
| Schema names | **sensitive**, matched exactly |
| Field names, enum members, tag names, path segments | insensitive (normalised, §3.3) |
| Instance ids, clone targets, `ref` values | insensitive (normalised, §3.3) |
| Asset path text | passed through unchanged; compared by the filesystem |
| Keywords (`schema`, `logic`, `derive`, …) | **sensitive**, lowercase only |

### 2.7 Ordering of instances in the compiled document

The `data` array and every `overlays[].data` array MUST be ordered by the pair `(template, id)`:

1. by `template`, comparing the schema name as a sequence of Unicode scalar values;
2. then by `id`, comparing the normalised id as a sequence of Unicode scalar values.

Because ids are unique project-wide (§5.3), the order is total. It does not depend on file layout, directory names, command-line order or filesystem enumeration order.

---

## 3. Lexical structure

### 3.1 Tokens

The lexer produces these tokens: identifiers, schema names, keywords, punctuation (`:` `::` `.` `,` `=` `(` `)` `[` `]` `{` `}` `@` `#` `&` `$` `*` `!` `?` and the operator spellings of §6.6), integer literals, float literals, quoted strings, bare text, and the logical line terminator `NL`.

Horizontal whitespace (space `U+0020`, tab `U+0009`) separates tokens and is otherwise insignificant. **Indentation carries no meaning.** Two programs that differ only in leading whitespace MUST compile identically. No other whitespace character may appear **between tokens** (E210). Inside a token the token's own rule governs: §3.5 fixes the character set of `bare_text` and of a `quoted_string`, and a non-ASCII whitespace character inside a value is an ordinary character of that value, never a separator.

**Longest match.** At each position the lexer takes the longest match, with one exception: a `.` immediately followed by another `.` is always the two-character token `..` and is never part of a float literal. Consequently `1..3` is `1`, `..`, `3`, and `1.5..2.5` is `1.5`, `..`, `2.5`.

**Adjacency.** The constructs below are single lexemes and MUST contain no internal whitespace:

- `[]` and `[<cardinality>]` in a field head (§4.3, §4.6);
- `::`, `..`, `${`, `$$`, `derive?`;
- the `(` that opens a parameterised type, a `ref(`, a `$(`, a `#tag` argument list, a tuple-array column list, or a `@since(` / `@removed(` annotation — the annotation's `(` MUST follow the keyword with no space, which is what distinguishes `@since(2)` from the header tag `@since` (§5.2);
- the `WIDTHxHEIGHT` size token of §4.4.7;
- the `#`, `@`, `&` or `$` that introduces a tag object, a header tag, a modifier, a clone or a variable, together with the identifier that follows it.

Whitespace inside any of these is E210, except before a parameterised type's `(`, which is E304. `?` is punctuation only as the last character of `derive?`.

### 3.2 Comments

A comment starts at `//` and runs to the end of the physical line. The `//` starts a comment only when it is at the start of a line or is immediately preceded by a space or a tab, and only when it is not inside a quoted string.

```abstract
// a whole-line comment
link: https://example.com          // a trailing comment; the URL is intact
note: "// not a comment"
```

An unquoted value MUST NOT begin with `//` and MUST NOT contain ` //` (E210). The second half is a consequence of the comment rule; **the first half is a rule in its own right** and holds wherever the value begins, including immediately after the `:`, where no comment rule fires. `label://x` is therefore E210 at the `//`, not the value `//x`. A protocol-relative URL, a root-relative POSIX path, and any text with a spaced double slash are written quoted:

```abstract
cdn: "//cdn.example.com/x.png"     // keeps its leading '//'
ratio: "50 // 2"                   // keeps ' // '
cdn://cdn.example.com/x.png        // E210: an unquoted value cannot begin with '//'
```

The rule keeps one spelling for one meaning: an author who moves a value onto its own line, or puts a space after the `:`, does not change what the file means.

An unquoted value whose text is emptied by comment stripping is E210, never an empty assignment.

There are no block comments. Comments never appear in compiled output.

### 3.3 Identifiers and normalisation

An `identifier` is one or more of `A-Z a-z 0-9 _ -`, MUST begin with `A-Z a-z 0-9 _`, and MUST NOT end with `-` (E207). Non-ASCII characters are not identifier characters (E206); this is deliberate, so that normalisation is total and locale-independent.

The three rules carry three diagnostics, and the first two are lexical, not semantic:

| Rule | Diagnostic |
|---|---|
| A character of the run is not `A-Z a-z 0-9 _ -` | E206 at the offending character, naming the whole run |
| The run begins with `-` | **E210** at the `-`, which is a token that cannot begin an identifier — not a character that cannot appear in one |
| The run ends with `-` | E207, naming the whole run |

`-name: 1` is E210, and so is `@-name.x`; `name-: 1` is E207. E206 is never reported for a `-`, because `-` is an identifier character; a message that named it as invalid would deny its own character list.

**Normalisation** maps an identifier to its canonical form:

1. ASCII-lowercase every character;
2. replace every `-` with `_`.

Normalisation is applied to: schema field names, enum members, path segments, header tag names, `#tag` names, `tag_arg` keys, multi-path keys, tuple column names, clone path segments, instance ids, `ref` values, and logic path segments. Two identifiers that normalise to the same text are the same identifier: `Max-Count`, `max_count` and `MAX_COUNT` all denote `max_count`.

Normalisation is NOT applied to: schema names, quoted string contents, bare text values, or asset path text.

Purely numeric identifiers are legal (`1`, `2`, `07`). They are ordinary names, not indices; see §4.9.

### 3.4 Schema names

A `schema_name` MUST begin with an ASCII letter and continue with ASCII letters, digits and `_`. It contains no `-`, no `.` and no whitespace. Schema names are compared **exactly**: `Product` and `product` are two different names, and a reference to a name that was not declared is E309 (or E401 from an instance header). A malformed schema name is E208.

### 3.5 Literals

Abstract's parser is **type-directed**: an unquoted value is not classified by the parser (§5.10). The literal grammars below define how the *validator* interprets bare text for a given schema type, and how the *logic* language reads literals in conditions and derive values.

**Integer literal.** An optional `-`, then one or more ASCII digits. The value MUST fit in a signed 64-bit integer; otherwise E211. E211 is reported wherever the literal appears, including in a value position whose type would be decided later, and takes precedence over E412 and E413 (§11.1). A leading `+` is not accepted. Leading zeros are accepted (`007` is 7). Underscores are not accepted.

**Float literal.** An optional `-`, then digits, then either `.` and digits, or an exponent, or both. The exponent is `e` or `E`, an optional sign, and digits. At least one digit MUST appear before the `.` and at least one after it: `.5` and `5.` are not float literals. The value MUST be finite; a literal that overflows to infinity is E211.

**Boolean literal.** Exactly `true` or `false`, lowercase.

**Quoted string.** `"` … `"`. A quoted string MUST open and close on the same physical line (E201). Inside it, `\` begins an escape:

| Escape | Produces |
|---|---|
| `\"` | `"` |
| `\\` | `\` |
| `\n` | `U+000A` |
| `\r` | `U+000D` |
| `\t` | `U+0009` |

Any other character after `\` is E202. There is no `\u` escape: to write a character, write the character. Windows paths therefore MUST be written with `/` or with `\\`.

A `quoted_string` is lexed as **one token**. The characters inside it are never brackets, comment starts, annotation tokens or list separators for the purpose of lexing (§3.6): `note: "a ] b"` closes no bracket, and `note: "a, b"` is one value.

**Bare text.** Any other unquoted run of characters, defined by `bare_text` in the grammar. Bare text keeps its exact characters (after trimming leading and trailing whitespace); no escape processing is applied to it, and no type is inferred from it at parse time.

**The character set of bare text.** A `bare_text` run admits any Unicode scalar value **except**:

- a C0 control (`U+0000`–`U+001F`), including the line terminators `U+000A` and `U+000D`, which end the logical line instead;
- `U+007F` DELETE;
- a C1 control (`U+0080`–`U+009F`), `U+0085` NEL included.

Any of them inside an unquoted value is E210 at the character, naming it as `control character U+XXXX`. Every other scalar value is an ordinary character of the value: **non-ASCII whitespace is literal**, so `U+00A0`, `U+2028` and `U+3000` inside a value are part of the text and never separate tokens (§3.1). The byte-order mark is the one further exception: only a BOM at offset 0 is removed (§2.2), and one anywhere else is E210 (§10.2).

A control character that an author really needs is written inside a `quoted_string`, which admits every scalar value except a line terminator (E201), with `\t`, `\n` and `\r` for the three that have escapes. Output escapes the whole excluded set in every format (§8.8), so a control character never reaches a consumer unescaped, whichever spelling produced it.

### 3.6 Logical lines and statement continuation

The lexer maintains a **value-bracket stack** for `(` `)`, for `[` `]`, and for the `{` `}` of a multi-path key list (§5.4). A `{` immediately preceded by `.` opens a multi-path key list; every other `{` is a **block brace**. Types MUST match: a `)` closing a `[` is E204; a closing bracket with an empty stack is E205; a non-empty stack at end of file is E203. Brackets that occur inside a `quoted_string` are neither pushed nor popped (§3.5).

**This rule applies to `{` as a token.** Inside a `bare_text` both `{` and `}` are ordinary characters that raise and lower the run's own bracket depth and are subject only to the balance requirement of §3.5. That is what makes the depth-0 comma of a brace pattern part of the value (§5.8), and it is why a `${` is not a block brace either: §3.1 makes `${` one lexeme, so its `{` belongs to that lexeme, and an unterminated one is E427 (§5.11), never E203.

**Braces in a bare value MUST balance**, whatever the field's declared type. A `{` left open when the run ends is E203 at the `{`; a `}` with no open `{` in the run is E205 at the `}`. A field whose value is meant to contain a literal, unbalanced brace is written quoted:

```abstract
icon: a}.{b        // E205 at the '}', then E203 at the '{'
label: "a}.{b"     // one value, two literal braces
```

The requirement is lexical and therefore uniform: a `text` field, for which §5.8 performs no expansion at all, is subject to it exactly as a `file` list field is. Only the *meaning* of a balanced group depends on the type.

- While the value-bracket stack is **empty**, a line terminator ends the logical line and produces `NL`.
- While the value-bracket stack is **non-empty**, a line terminator produces nothing: the logical line continues.

A `{` that opens a schema body (§4.2), a group body (§4.7), a logic block (§6.3) or an instance body block (§5.4) is a block brace. Block braces are matched against their `}` — an unmatched block brace is E203, and a `}` with no open block brace is E205 — but they are never pushed onto the value-bracket stack, so **a block brace does not join lines**. A block's `{` MUST be the last token of its logical line and its `}` MUST be the first token of a logical line.

There are exactly two further continuation rules, and both suppress the `NL`:

- **(a)** inside a header tag list (§5.2), when the logical line so far ends with `,`;
- **(b)** inside a logic block (§6), when the next token after the line terminator is `else`, so that `require …` / `else throw …` and `}` / `else {` may be written on two physical lines. The test is on the next **token**, not on the next physical line: blank lines and comment lines carry no token (§3.1, §3.2), so any number of them MAY sit between the `}` and its `else`, or between a `require` condition and its `else throw`, and the `NL` is still suppressed. The rule does not ask what *precedes* the `else`; it joins the lines wherever the next token is one. That is well formed only where §6.3 admits an `else`, and elsewhere the joined line is malformed and is reported as such (§6.3).

There is no other continuation mechanism. In particular a trailing `,` at value-bracket depth 0 does **not** continue a statement; it is E442.

```abstract
tags: [
    core,
    public
]                      // continues: the '[' is open

tags: core, public     // one statement, one line

tags: core,            // E442: trailing comma at depth 0
public

schema Label {         // the block brace does not join lines:
    caption: text      // this NL is produced normally
}
```

Continuation rule (a) lets a long header be wrapped:

```abstract
Product :: @id.atlas,
           @status.active
```

### 3.7 Nesting and size limits

An implementation MUST enforce these limits and MUST report the identifier named below when one is exceeded, naming the subject and the limit. The limits exist so that a hostile or corrupted source can never exhaust the stack, and so that no legal program can ask the compiler for unbounded work.

| Subject | Limit |
|---|---|
| Bracket nesting depth in any value | 64 |
| Segments in an assignment path, clone path or logic path | 64 |
| Nesting depth of schema groups (static), and of `$(Schema)` values in one compiled object (dynamic) | 64 |
| Nesting depth of `if` / `for` blocks in one logic block | 64 |
| Length of a clone chain (transitive clone depth) | 64 |
| Nesting depth of one compiled instance | 64 |
| Number of versions in the project range (`max - min + 1`) | 4096 |
| Loop iterations executed by the logic of one instance for one version | 1 000 000 |

The last two rows are the limits **not** reported as E209. A range too wide is a defect of the `versions` declaration itself, so it is E602 and its message names the bound (§4.12, §10.6). Logic work past its bound is a defect of what a program *does* rather than of how deeply it is written, so it is E523 and its message names the bound (§6.2, §10.5). Every other row is E209.

**How instance depth is counted.** The subject of the depth row is the resolved value tree of one instance, counted from **the instance object as level 1**; every object and every list one level down adds one. A scalar leaf adds nothing, and the document envelope — the top-level object, the `data` array, the `overlays` array — adds nothing, because it is fixed by §8.1 and is not something a source can nest. So an instance whose deepest chain is `child.child.…child.leaf` with 61 `child` objects is 62 levels deep and compiles, and a schema that nests groups 63 deep is 64 levels deep and compiles: the depth row and the group row of this table agree on the same structure.

The limit is checked once, in validation (§7.3 steps 3 and 7), and the diagnostic is positioned at the statement that reaches the limit, or at the instance header when the depth is reached through defaults, clones or logic. The output stage performs **no** depth check of its own: by the time a document is rendered its depth is already known to be within the limit.

**How logic work is counted.** Every other row of the table bounds the *shape* of a program; the last row bounds the *work* one demands, because nesting alone does not: seven `for` blocks nested over ten-element lists are seven levels deep, well inside every other row, and ask for eleven million iterations. A loop unit of work is **one execution of the body of a `for`** — one iteration. For programs without `calc`, nothing else is charged: `derive`, `derive?`, `require` and `if` each run at most once per enclosing iteration, so charging the iteration bounds the whole evaluation. The budget is spent by every `for` that runs while one instance is compiled for one version — the block of the instance's own schema and the block of every nested `$(Schema)` value alike (§6.11) — and it is fresh again for the next version and for the next instance, so the bound is on one instance-version and never on the project.

The check is made in §7.3 step 5, as the iteration is about to run, and the diagnostic is positioned at the `for` statement whose iteration crossed the bound. An implementation MUST stop that instance there: it MUST NOT run the remaining iterations, which is what makes the bound a bound and not a report. So seven `for` blocks nested over a literal list of ten elements demand 11 111 110 iterations and are E523, reported at the 1 000 001st; five of them demand 111 110 and compile.

An implementation MUST NOT abort, panic or crash on any input. Every failure MUST be a diagnostic with an identifier and, where a source position is known, a position.
---

## 4. Schemas (`.abt`)

### 4.1 Template file structure

Grammar: `template_file`, `template_item`.

A template file contains, in any order and any number: `schema_decl`, `logic_decl`, and at most one `versions_decl` (§4.12). Anything else at top level is E210. There is no "skip unknown line" behaviour: an unrecognised construct is always an error with a position.

```abstract
versions 1..2

schema Product {
    status: enum(draft, active, retired)
}

logic Product {
    require .status != "retired" else throw "Retired products are not shipped."
}
```

### 4.2 Schema declaration

Grammar: `schema_decl`.

```
schema <schema_name> {
    <field_list>
}
```

- `<schema_name>` MUST satisfy §3.4 (E208).
- Schema names MUST be unique across the whole project. A second declaration of the same name is E301 and names both declaration sites, each with `file:line:col`.
- A schema MAY declare zero fields.
- The declared order of fields is significant: it fixes the order of keys in compiled output (§8.3).
- The opening `{` MUST be on the same logical line as the schema name; the closing `}` MUST be alone on its logical line.

### 4.3 Field declarations

Grammar: `field_decl`, `scalar_field`, `group_field`, `field_head`, `field_modifier`.

A field is either a **scalar field**

```
<name>[ "[" [<cardinality>] "]" ] : <type_expr> [<modifiers>] [= <default>]
```

or a **group field**

```
<name>[ "[" [<cardinality>] "]" ] [<modifiers>] {
    <field_list>
}
```

- `<name>` is an `identifier` and is normalised (§3.3). Two fields in the same block whose names normalise to the same text are E302, reported at the second declaration, **regardless of their version annotations** (§4.12).
- `[]` immediately after the name marks a **list field** (§4.6), optionally carrying a cardinality (`[1..]`, `[2..4]`). Exactly one bracket pair is permitted; `name[][]` is E315.
- Modifiers are `@optional`, `@tag`, `@public`, `@since(n)` and `@removed(n)`. Each modifier MAY appear at most once per field (a repeat is E303). An unknown `@word` is E303.
- Modifiers appear after the type expression (or, for a group field, after the field head) and **before** the `=` of a default. They MUST NOT appear before the `:`; the left of `:` is exactly the field head. A modifier after a default is E303: `glow: bool = false @since(2)` is rejected, because a bare default value would otherwise be distinguishable from a trailing annotation only by look-ahead. Write `glow: bool @since(2) = false`.

**Public nomination.** `@public` records the author's nomination of exactly the
declared field. It takes no arguments: `value: text @public(1)` is E210. The
compiler retains the nomination and its source position in schema metadata;
absence of the modifier leaves the field un-nominated. `public` is not a new
reserved word and remains valid as an ordinary field name or enum member.

Nomination is permitted on all existing field types, lists and groups, subject
to their existing declaration rules. A group or nested-object field marked
`@public` does **not** implicitly nominate its children. Root envelope `id` and
`template` retain the restrictions of §4.11; `id: text @public` is E314.
The modifier does not alter requiredness, defaults, type/range validation,
cloning, interpolation, logic, version windows or compiled data/overlay bytes.
It is neither a data value nor an instruction to remove or conceal other data.

An exporter may apply an explicit, versioned public-contract profile to these
nominations. Ordinary parsing/compilation does not establish that a nominated
field is exportable, has an authorized consumer, or can be changed at runtime.
It does not skip `derive`/`require` checks or supply a runtime configuration API.

```abstract
schema DisplaySettings {
    caption: text(1..80) @public = "Welcome"
    scale: float(0.5..2.0) @public = 1.0
    quota: int(1..1000) @public = 64
    show_preview: bool @public = true
    tone: enum(quiet, bright) @public = quiet
}
```

### 4.4 Types

Grammar: `type_expr`.

There are nine types. A type keyword MUST be immediately followed by `(` when it takes arguments; `text (1..40)` with a space is E304, as is any keyword that is not one of the nine.

An argument list MUST NOT contain an empty entry: `enum(a,,b)` and `enum(a,)` are E306; `file(png,,jpg)` and `image(png,)` are E319. `enum()`, `file()`, `image()` and `ref()` are E317; `text()`, `int()` and `float()` are valid and unconstrained (§4.4.1–§4.4.3).

#### 4.4.1 `text`

Grammar: `text_type`. `text`, `text()` and `text(<int_range_list>)` are all valid; the first two are unconstrained.

Semantics: a string. When ranges are given, the **number of Unicode scalar values** in the string MUST fall in at least one range. This is a scalar count, not a byte count and not a grapheme-cluster count: a single emoji with a skin-tone modifier counts as 2.

```abstract
schema Label { caption: text(1..40) }
```
```abstract
Label :: @id.hello
    caption: "Hello, world"
```
```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [
    {
      "template": "Label",
      "id": "hello",
      "caption": "Hello, world"
    }
  ],
  "overlays": []
}
```

The value is quoted because an unquoted `,` always separates list items (§5.5): `caption: Hello, world` is a two-item list and, on this non-list field, E412.

#### 4.4.2 `int`

Grammar: `int_type`. Semantics: a signed 64-bit integer. When ranges are given the value MUST fall in at least one range.

```abstract
schema Run { replicas: int(1, 3..12) }
```
`replicas: 1`, `replicas: 3` and `replicas: 12` are valid; `replicas: 2` is E413.

#### 4.4.3 `float`

Grammar: `float_type`. Semantics: an IEEE-754 binary64 value that MUST be finite. Ranges are compared numerically; an integer bound is allowed (`float(0..1)`).

A `float` field accepts an integer literal as well as a float literal (`price: 19` yields `19.0`); an `int` field does **not** accept a float literal; see §5.10. Both are emitted as numbers, with `float` always carrying a fractional part or an exponent (§8.6).

#### 4.4.4 `bool`

Grammar: `bool_type`. Semantics: `true` or `false`. There are no ranges, and no default coercion from `0`/`1`/`yes`/`no`.

#### 4.4.5 `enum`

Grammar: `enum_type`. One or more members; each member is an `identifier` and is normalised. An empty member list, or two members that normalise alike, is E306.

Semantics: the value MUST normalise to a declared member. The **normalised** member is what is emitted, so `Active`, `active` and `ACTIVE` all compile to `"active"`.

```abstract
schema Product { status: enum(draft, active, retired) }
```
```abstract
Product :: @id.atlas, @status.Active
```
```json
{ "template": "Product", "id": "atlas", "status": "active" }
```
(Envelope elided; see §8.1 for the full document.)

#### 4.4.6 `file`

Grammar: `file_type`. One or more extensions are REQUIRED (`file()` is E317).

Extensions are **canonicalised** before they are compared, in declarations and in values alike: a single leading `.` is stripped, the text is ASCII-lowercased, and `jpeg` is mapped to `jpg`. No other spellings are folded. Two declared extensions that canonicalise alike are E319, so `file(jpg, jpeg)` is E319; and `image(jpeg 100x100)` accepts both `a.jpg` and `a.jpeg`.

Semantics: the value is an asset path (§5.9). Its extension MUST be one of the declared extensions (E420). When asset checks are enabled, the resolved file MUST exist (E421). `file` never inspects file contents.

#### 4.4.7 `image`

Grammar: `image_type`, `image_alt`, `size_token`. Each alternative is an extension, optionally followed by **at least one space or tab** and a size token. A size token is one lexeme with no internal whitespace (§3.1): `WIDTHxHEIGHT`, where each side is a decimal integer or `*` meaning "any". Supported formats: `png`, `jpg` (canonical spelling; `jpeg` canonicalises to it, §4.4.6), `gif`, `bmp`, `webp`. Any other extension is E307. `image(png128x128)` is E307, because the extension is the single run `png128x128`; `image(png 128 x 128)` is E308.

Semantics, in order:

1. The value is an asset path (§5.9).
2. Its extension MUST map to a supported format (E307 at schema time, E420 at instance time when the extension is not declared).
3. The extension MUST match one of the declared alternatives (E420).
4. When asset checks are enabled: the file MUST exist (E421); its header MUST identify the same format as the extension (E422); and its dimensions MUST satisfy at least one declared alternative whose extension matches (E423).

The compiler reads only a bounded prefix of the file and MUST NOT decode pixels; §4.4.7.1 fixes exactly what it reads. A file whose first bytes match a supported signature but which is too short to yield dimensions MUST be reported as E422 with `note: file is truncated.`, never as an unrecognised signature.

```abstract
schema Sticker {
    icon: image(png 128x128)
    hero: image(png 1920x1080, jpg 1920x1080) @optional
    wide: image(png 1024x*) @optional
}
```

#### 4.4.7.1 Header probing

When asset checks are enabled, the compiler determines an image's format and dimensions by reading a bounded prefix of the file. Probing MUST obey all of the following, so that E422 and E423 are reproducible on every platform.

- The compiler MUST read at most **65 536 bytes** from the head of the file and MUST NOT read further, whatever the file's size. A file whose format cannot be decided within that prefix is E422 with `note: no header found in the first 65536 bytes.`
- Format is decided by signature. Each format has its own byte requirement; the requirement of one format never gates another.

| Format | Signature | Bytes required | Dimensions |
|---|---|---|---|
| `png` | `89 50 4E 47 0D 0A 1A 0A` at 0 and `IHDR` at 12 | 24 | big-endian u32 at 16 (width) and 20 (height) |
| `gif` | `GIF87a` or `GIF89a` at 0 | 10 | little-endian u16 at 6 and 8 |
| `bmp` | `42 4D` at 0 | 26 | little-endian i32 at 18 (width) and 22 (height) |
| `webp`/VP8 | `RIFF` at 0, `WEBP` at 8, `VP8 ` at 12, `9D 01 2A` at 23 | 30 | 14-bit little-endian at 26 and 28 |
| `webp`/VP8L | `RIFF` at 0, `WEBP` at 8, `VP8L` at 12, `2F` at 20 | 25 | 14-bit packed in the four bytes at 21 |
| `webp`/VP8X | `RIFF` at 0, `WEBP` at 8, `VP8X` at 12 | 30 | 24-bit little-endian at 24 and 27, each plus one |
| `jpg` | `FF D8` at 0 | see below | the first SOF segment |

- A file that matches a signature but is shorter than that format's byte requirement is E422 with `note: file is truncated.` A file that matches no signature is E422 naming the first bytes found. A short file is never reported as an unrecognised signature, and an unrecognised file is never reported as truncated.
- BMP width MUST be greater than 0; a width of 0 or less is E422 with `note: malformed BMP width.` BMP height MAY be negative, which denotes a top-down bitmap; its absolute value is the height.
- Every decoded dimension MUST be at least 1, and at most 16 383 for VP8 and VP8L and at most 16 777 216 for VP8X; a value outside its format's range is E422 with `note: malformed canvas size.` VP8X carries no signature byte, so this range check is the only validation it can receive and MUST be applied.
- The JPEG walk starts at offset 2 and is bounded **by bytes read, not by a segment count**. Runs of `FF` fill bytes between segments are skipped by scanning forward, and that scan counts against the byte budget. The first `SOF0`–`SOF3`, `SOF5`–`SOF7`, `SOF9`–`SOF11` or `SOF13`–`SOF15` marker supplies height (u16 at marker + 5) and width (u16 at marker + 7). Reaching `SOS`, the end of the file, or the byte budget with no SOF is E422 with `note: no start-of-frame marker.`
- Every diagnostic, and every allowed-extension list in an E420 message, spells the format `jpg`; the spelling the schema used is not echoed.
- Probing MUST NOT decode pixel data, MUST NOT follow any offset outside the byte budget, and MUST NOT depend on the file's total length.

#### 4.4.8 `ref`

Grammar: `ref_type`. `ref(<schema_name>)`.

Semantics: the value is the **id of another instance** in the same project. The value is normalised as an id (§3.3). The referenced instance MUST exist (E431) and MUST have been declared with the named schema (E432). The referenced instance MUST also exist in the version being compiled: a reference to an instance whose window (§5.14) excludes that version is E431, with the message that names the version. The value is emitted as a JSON string; the referenced object is **not** inlined.

```abstract
schema Sticker { id_of_pack: ref(Pack) }
```
```abstract
Sticker :: @id.dragon
    id_of_pack: winter_2026
```
```json
{ "template": "Sticker", "id": "dragon", "id_of_pack": "winter_2026" }
```

A list of references is written `refs[]: ref(Pack)`. Wildcards (§5.6) do not apply to `ref` fields.

#### 4.4.9 `$(Schema)` — nested object

Grammar: `nested_schema_type`. `$(<schema_name>)`.

Semantics: the value is an object validated against the named schema, recursively, as a non-root object. Being a non-root object, it has no `id` and no `template` key, and its `@tag`-based shorthand is unavailable because a root schema may not declare `@tag` (§4.8). Forward and cross-file references are allowed; an unknown name is E309, reported at the type, at schema-validation time, not lazily at first use.

```abstract
schema Owner {
    team: text(1..50)
    contact: text(1..80)
}

schema Product {
    owner: $(Owner)
}
```
```abstract
Product :: @id.atlas
    owner.contact: systems@example.com
    owner.team: Knowledge Systems
```
```json
{
  "template": "Product",
  "id": "atlas",
  "owner": { "team": "Knowledge Systems", "contact": "systems@example.com" }
}
```
Note the key order inside `owner`: it follows `schema Owner`, not the assignment order (§8.3).

### 4.5 Ranges

Grammar: `int_range_list`, `int_range`, `float_range_list`, `float_range`.

A range list is a comma-separated list of parts. A part is either an **exact value** (`7`) or a **closed interval** (`2..15`). Both bounds are REQUIRED: `int(5..)` and `int(..5)` are E305. A value satisfies the type when it satisfies at least one part.

- The lower bound MUST be less than or equal to the upper bound. `int(10..5)` is E305 (reversed range), reported at the schema, not deferred to an instance.
- A `text` range MUST have both bounds greater than or equal to 0. `text(-5..-1)` is E318 (unsatisfiable constraint).
- Overlapping and duplicate parts are permitted; they neither widen nor narrow the constraint.
- Float bounds MUST be finite (E305).

### 4.6 List fields

A field whose head ends with a bracket pair holds an array. Every element is validated independently against the field's type. A single non-list value assigned to a list field is coerced to a one-element array **after** it is validated (§5.10).

- Nested lists are not part of the language: `name[][]` is E315 in a schema, and a list literal inside a list literal is E441 in an instance.
- An `@optional` list field that is absent is **omitted** from the output, exactly like any other absent optional field. It does not become `[]`. (This differs from 0.2.0; see Appendix A.)

**Cardinality.** The bracket pair MAY carry a cardinality, which constrains the number of elements a present value may have:

| Head | Meaning |
|---|---|
| `tags[]` | any number of elements, including none |
| `tags[2..]` | at least 2 |
| `tags[2..4]` | at least 2 and at most 4 |

- Both forms are written with `..`; the lower bound is REQUIRED and MUST be an integer greater than or equal to 0; the upper bound, when present, MUST be greater than or equal to the lower bound. Anything else — `tags[..4]`, `tags[3]`, `tags[4..2]`, `tags[-1..]` — is E323, reported at the field.
- A present value whose element count is outside the cardinality is E445, naming the count and the declared bounds. The check runs in §7.3 step 3 and again in step 7, so a list that logic shortened or lengthened is checked too.
- A required list field with no cardinality, or with a lower bound of 0, is satisfied by an explicitly empty list; `tags: []` is valid and emits `[]`. With a lower bound greater than 0 an empty list is E445.
- Cardinality constrains a value, never presence: an absent `@optional` list is omitted and is not E445.

### 4.7 Groups and list groups

Grammar: `group_field`.

A **group** is an inline nested object declared in place:

```abstract
schema Product {
    owner {
        team: text(1..50)
        contact: text(1..80)
    }
}
```

A **list group** is an array of such objects:

```abstract
schema Product {
    capabilities[] {
        id: enum(search, sync, export) @tag
        availability: enum(alpha, beta, stable) = stable
    }
}
```

Rules:

- A group field MUST NOT declare a default; `owner { ... } = x` is not expressible and `= ` after a group head is E312.
- A group MAY be `@optional`. An absent optional group is omitted; a present group has all of its own required children checked.
- Groups nest to the limit in §3.7 (E209).
- Prefer `$(Schema)` when the same shape appears in more than one schema; prefer a group when the shape exists only here. The two are validated identically.

**Presence.** A group value or a `$(Schema)` value is **present** when a clone, a header tag, a body statement or a `derive` wrote at least one value at or under its path. An absent optional group is omitted entirely: its children's defaults are not filled (§7.3 step 4) and its required children are not checked (§6.11). A `derive` or `derive?` that writes into an absent optional group makes it present (§6.4). An absent **required** group is E411, reported at the group.

### 4.8 `@tag`

`@tag` marks the single field of a group that receives the `#value` shorthand in instances (§5.5).

- `@tag` MUST appear on a field declared **inside a group or list group**. `@tag` on a field of a top-level schema is E311 ("never at root"), including on a schema that is only ever used through `$(Schema)`.
- At most one field per group may carry `@tag`; a second is E310.
- The tagged field MUST be a non-list field of type `text`, `int`, `bool` or `enum` (E322). `file`, `image`, `ref`, `$(Schema)` and group fields cannot be tags.
- The tagged field MAY carry a default, or MAY be `@optional`, but not both (§4.10, E320).

```abstract
capabilities[] {
    id: enum(search, sync, export) @tag
    availability: enum(alpha, beta, stable) = stable
}
```
```abstract
capabilities: [#search, #export(availability: alpha)]
```
```json
"capabilities": [
  { "id": "search", "availability": "stable" },
  { "id": "export", "availability": "alpha" }
]
```

The tag field need not be called `id`. With `key: enum(en_us, es_es) @tag`, the shorthand `#es_es` fills `key`, and nothing named `id` ever appears.

### 4.9 Numbered keys

A field name may be all digits. Numbered keys are the idiom for fixed positional groups:

```abstract
schema Pack {
    slots {
        1: $(Slot)
        2: $(Slot) @optional
        3: $(Slot) @optional
    }
}
```

A numbered key is an ordinary field name. It is addressed as `slots.1` from instances and `.slots.1` from logic. It is **not** an array index: there is no index syntax in assignment or clone paths (index syntax exists only for reads in logic conditions, §6.5). In compiled output a numbered key is a string key in JSON and a quoted key in YAML (§8.5), so no consumer sees it as a number. In RAW (§8.6) it is written bare, like every other key; RAW is a review format and is not machine-parsed.

### 4.10 `@optional` and defaults

- A field with no `@optional` and no default is **required**: it MUST have a value after logic runs, or E411.
- A field with a **default** is filled with that default when it is absent, before logic runs (§7.3).
- A field marked `@optional` and absent after logic runs is **omitted** from the output. Absence is uniform: there is no `null` in Abstract, in any format, for any type.
- A field MUST NOT carry both `@optional` and a default (E320): the default makes the field always present, so `@optional` could never take effect.

A default is a `value` (the same grammar instances use) and is validated against the field's declared type **at schema-validation time** (§7.2), not lazily when some instance happens to omit the field. A default that violates its own type is E313, reported at the default.

A default is **interpolated** (§5.11) at the moment it is filled in, against the variable table of the instance it is filled into. Because that table is built at §7.3 step 2 from authored values, a default may reference any root scalar field the instance itself authored, and `$id` always resolves. An unknown variable in a default is E425, reported at the default's declaration site with a note naming the instance.

```abstract
availability: enum(alpha, beta, stable) = stable
price: float(0..9999) = 0.0
featured: bool = false
tags[]: enum(core, public, internal) = [core, public]
```

A default is written with the same `value` grammar instances use, including a bare comma-separated list: `= a, b` and `= [a, b]` are the same two-element list, and `= a` is coerced to `[a]` for a list field exactly as an authored single value is (§5.10). On a **non-list** field a default that is a list — bracketed or bare — is E313 with `note: this field is not a list.` A group field may not have a default (§4.7).

### 4.11 Reserved names: `template` and `id`

`template` and `id` are **envelope keys**. Every compiled instance object begins with them (§8.2).

- A schema MUST NOT declare a field named `template` (E314), and `template` MUST NOT be assigned (E410). It is written by the compiler from the instance header and is never authored.
- Every schema has an **`id` field** whose value comes from the `@id` header tag or, in its absence, the source file stem (§5.3). It is never assigned by a body statement (E410) and is never defaulted.
- The `id` field is **implicitly declared as `id: text(1..64)`**. A schema MAY declare it explicitly, at root level, in exactly one of two spellings:

```abstract
schema Pack {
    id: text
    title: text(1..40)
}
```
```abstract
schema Sticker {
    id: text(3..24)
    title: text(1..60)
}
```

  `id: text` is unconstrained; `id: text(<int_range_list>)` constrains the number of Unicode scalar values in the id exactly as §4.4.1 constrains any `text` value. **Anything else on `id` is E314**: another type, a list head (`id[]`), any modifier (`@optional`, `@tag`, `@since`, `@removed`), and a default. Declaring `id` is therefore never a way to change what the id *is*, only a way to constrain and to document it; the normalisation of §3.3 and the identifier rule of §5.3 apply whether or not the field is declared.

- An id whose scalar count is outside the applicable range — declared, or the implicit `1..64` — is E413 at the instance, reported at the `@id` tag or, for a file-stem id, at the instance header.
- A declared `id` changes nothing about output: the id is emitted once, as the second key of the instance object (§8.2, §8.3), whether or not the schema declares the field.
- Inside a **group or list group**, `id` and `template` are ordinary field names with no envelope meaning, because a nested object carries no envelope (§4.4.9). `id: enum(search, sync, export) @tag` inside a group is legal and is the common spelling of a keyed list (§4.8).
- When a schema that declares `id` is used as a `$(Schema)` value, the `id` field does not exist in that non-root object: it is not required there, is never written and is never emitted (§4.4.9).

Migration impact is listed in Appendix A (A7).

### 4.12 The `versions` declaration and `@since` / `@removed`

Grammar: `versions_decl`, `since_annotation`, `removed_annotation`.

```abstract
versions 1..3
```

- A project MUST contain at most one `versions_decl`, in any `.abt` file, at top level. A second is E601, naming both sites.
- Both numbers MUST be integers greater than or equal to 1, and `min` MUST be less than or equal to `max` (E602).
- The range MUST cover at most **4096** versions: `max - min + 1 <= 4096`, otherwise E602 with a message that names the bound (§3.7, §10.6). The bound is on the **count**, never on the numbers, so `versions 100..163` is as legal as `versions 1..64`. Without it a four-line source could demand an unbounded amount of work from §7.4, which compiles every version in the range, and no implementation could both honour that and honour §3.7's "MUST NOT abort".
- When no declaration is present, the project version range is `1..1`.
- The project version range is a closed interval of integers; every version in it is compiled (§7.4).

A field annotated `@since(n)` exists in versions `v >= n`. A field annotated `@removed(n)` exists in versions `v < n`. Together, a field exists for `since <= v < removed`, where `since` defaults to the project minimum and `removed` defaults to `max + 1`.

- `n` MUST be within the project range (`min <= n <= max`), otherwise E603.
- `@removed(n)` MUST have `n` strictly greater than the field's effective `@since`, otherwise E604.
- For a field inside a group, the effective existence set is the intersection of its own set with every ancestor's set. An empty intersection is E605.

```abstract
versions 1..3

schema Item {
    name: text(1..40)
    glow: bool @since(2) = false
    legacy_tint: int(0..255) @removed(3) @optional
}
```

`glow` exists in versions 2 and 3. `legacy_tint` exists in versions 1 and 2. A statement that assigns `legacy_tint` and carries no annotation applies to exactly the versions in which the field exists, so it is normally written unannotated; an annotation is needed only to give one field different values in different versions, or to narrow a statement further than the field's own existence set (§5.13). A statement whose annotation range does not intersect the field's existence set is E430.

**A field is never removed and re-added.** A field's existence set is always one contiguous interval `[since, removed)`. Two declarations of the same name in one block are E302 regardless of their annotations. To express "present, then absent, then present", declare two differently named fields, or keep the field declared throughout and control its value with two annotated statements whose applicability sets are disjoint (§5.13).

**`$(Schema)` fields.** A `$(Schema)` field's annotations do **not** propagate into the referenced schema: that schema's fields carry only their own annotations, and one schema may be used at several sites under different annotations. **Both windows apply.** The effective existence set of a nested field is the intersection of the existence set of the `$(Schema)` field with the existence set of that field's own declaration, exactly as for a field inside a group: a nested value exists only in the versions in which both the field that holds the object and the field inside it exist. The existence set of a `$(Schema)` field is intersected with the union of the existence sets of the referenced schema's fields; if the result is empty — the referenced schema has no field at all in any version in which the field exists — it is E605, reported at the `$(Schema)` field with a note at the schema. In a version in which some of the referenced schema's fields do not exist, the object is compiled with exactly the fields that do exist, and a field that does not exist in that version is never required (§7.3 step 6).

**Every version must compile.** An instance is compiled once per version in which the instance exists (§5.14), and every check — required fields, ranges, cardinality, `require` — runs in each. A required field MUST have a value in every version in which it exists: annotating the only statement that supplies it is E411 in the versions the annotation excludes. Give a version-scoped field a default, mark it `@optional`, or write one statement per disjoint range so that the field's whole existence set is covered. Logic that reads a field which does not exist in every version MUST guard it, because an operand resolving to no value makes every comparison false (§6.8):

```abstract
if .glow exists {
    require .glow == false else throw "glow must start off."
}
```

Logic that *writes* a version-scoped field MUST guard it too, with the built-in `version` (§6.6) or with any other condition, because executing a `derive` against a field that does not exist in the version being compiled is E521 (§6.4). `legacy_tint` exists in versions 1 and 2, so a write to it is guarded by `version < 3`:

```abstract
if version < 3 {
    derive? .legacy_tint = 0
}
```

**Assets are not versioned.** Versions scope schema fields and instance statements only. A project that needs different bytes for different versions keeps them in separate asset directories and selects between them outside Abstract.

### 4.13 Schema-level errors, summarised

| Condition | Error |
|---|---|
| Duplicate schema name in the project | E301 |
| Duplicate field name in one block | E302 |
| Unknown or repeated `@modifier` | E303 |
| Unknown type keyword, or whitespace before `(` | E304 |
| Malformed, open-ended, non-finite or reversed range | E305 |
| Empty enum, or duplicate enum member | E306 |
| Unsupported image format | E307 |
| Malformed `WIDTHxHEIGHT` | E308 |
| Unknown schema name in `$(…)` or `ref(…)` | E309 |
| Second `@tag` in one group | E310 |
| `@tag` on a root-level field | E311 |
| Default on a group field | E312 |
| Default value invalid for the declared type | E313 |
| Field named `template`, or an `id` declaration other than `id: text` / `id: text(a..b)` | E314 |
| `name[][]` | E315 |
| Invalid field name | E316 |
| A parameterised type with no arguments | E317 |
| Unsatisfiable constraint (for example a negative `text` range) | E318 |
| Duplicate extension in `file(…)` / `image(…)` | E319 |
| Both `@optional` and a default | E320 |
| Recursive schema with no terminating case | E321 |
| `@tag` on a field of unsupported type or on a list field | E322 |
| Malformed list cardinality | E323 |
---

## 5. Instances (`.ab`)

### 5.1 Instance declaration and header

Grammar: `instance_file`, `instance`, `instance_header`.

An instance file is a sequence of instances. An instance is a header, then zero or more clone statements, then zero or more body statements and body blocks (§5.4):

```abstract
Product :: @id.atlas, @status.active
&base_product.*
    name: Atlas Search
    owner.team: Knowledge Systems
```

A logical line is an `instance_header` **if and only if** its first token is a `schema_name` and the token immediately following it is `::`. Nothing else can start an instance: a `::` inside a value is ordinary text, and a statement is never re-examined for a later `::`.

- The name before `::` MUST be a declared schema (E401), reported with the closest declared name when the edit distance is at most 2.
- A second `::` inside a header is E436.
- The token `::` occurs only as the second token of an instance header. A logical line whose first token is not a schema name and which contains a `::` token is E439, naming the text before it. A `::` inside a value is ordinary text and is never a token (§5.4), so `window: 12::30` is an assignment.
- A body statement or clone statement that appears before the first header in a file is E403.
- A header MAY end, after all of its header tags, with an **instance version window** — `@since(n)`, `@removed(n)`, or both (§5.14).
- Indentation of body statements is conventional, not meaningful (§3.1).
- One file MAY declare several instances. Each MUST have a distinct id (§5.3); since the file stem can serve only one of them, all but one MUST carry `@id`.

### 5.2 Header tags

Grammar: `header_tag_list`, `header_tag`.

A header tag is a compact assignment to a root field:

| Form | Meaning |
|---|---|
| `@name.value` | assigns the syntax value `value` to the root field `name` |
| `@name` | assigns the bare value `true` to the root field `name` |

- The tag name is normalised (§3.3).
- The value runs to the next depth-0 `,` or `@`, or to the end of the header. To include a literal `,` or `@` in a value, quote it: `@label."hi, there @world"`.
- A bare `@name` flag MUST target a `bool` field; on any other type it is E412.
- A header tag value is a `quoted_string` or bare text and **nothing else**: it can never be a list, a `#tag` object, a tuple row or a brace pattern, and `[`, `]`, `{`, `}` and `#` inside it are ordinary characters. Write those forms as body statements. Otherwise the value is interpreted by the field's declared type exactly like a body value (§5.10): `@count.12` on an `int` field yields the number 12; on a `text` field it yields the string `"12"`.
- The tag name ends at the first `.` after the maximal identifier run; everything after that `.` is the value, verbatim. A value that itself begins with `.` therefore needs the separator dot as well: `@icon../textures/a.png` assigns `./textures/a.png`, while `@icon./textures/a.png` assigns `/textures/a.png` and is E424. Asset values read better as body statements.
- A header tag whose normalised name is not a root field of the schema is E409. A header tag whose target field is a group, a list group or a `$(Schema)` field is E438: a header tag addresses exactly one root field and can never reach a nested one.
- `@id` written as a bare flag is E428: an instance's identity is never the boolean `true`.
- A header **tag** MUST NOT carry a `@since` / `@removed` annotation (E437): a version-scoped assignment goes in the body (§5.13). The header as a whole MAY end with an instance version window (§5.14), which is not a tag and scopes no single field.
- The two are distinguished lexically, never by look-ahead: `@since` and `@removed` are annotations when, and only when, the identifier is followed immediately by `(` (§3.1, adjacency). `@since(2)` at the end of a header is an instance window; `@since.2` is a header tag assigning `2` to a field named `since`, and a bare `@since` is a header tag flag on a `bool` field named `since`. A header tag's value ends at the next depth-0 `,` or `@` (the value rule above), so a window never runs into the value before it.
- The header tag list is the one construct that continues across physical lines on a trailing `,` (§3.6). A `,` that is not followed by another header tag is E442.
- Two header tags with the same normalised name, or a header tag and a body statement writing the same path, are E429.
- Header tags are assignments that run after every clone and before every body statement (§7.3 step 1), so a header tag always wins over a cloned value.
- **A header tag is an assignment, and the applicability rule of §5.13 reaches it.** A tag carries no annotation of its own, so its applicability set is the field's existence set (§4.12) intersected with the instance's window (§5.14). An empty set is **E440**, exactly as for the body statement that writes the same field: an assignment that writes nothing in every version is never silent. `Item :: @id.x, @glow @removed(2)`, where `glow` is `@since(2)`, is E440 at the tag, and stays E440 when it is rewritten as the body statement `glow: true`. A tag whose field exists in no version at all is already E605 at the schema (§4.12), which is reported first (§7.1).

### 5.3 Instance identity

Every instance has an `id`:

1. If the header carries `@id`, its value is the id. The value MUST be an `identifier` (E428); it is normalised (§3.3) and is never type-inferred, so `@id.42` yields the string `"42"` and `@id.Winter-Pack` yields `"winter_pack"`.
2. Otherwise the id is the **file stem** — the file name with its final `.ab` or `.abt` extension removed, compared case-insensitively — which MUST itself be a valid `identifier` and is normalised (E428). The stem is examined only for files that declare at least one instance with no `@id`, so `frost.v2.ab` is E428 only when some instance in it omits `@id`.

Rules:

- An id MUST NOT be empty (E428). `@id.` is E428.
- Ids MUST be unique across the whole project, across all templates. A collision is E402 and names both sites. Because ids are normalised, `Atlas` and `atlas` collide. Version windows do not partition the id space: two instances that share an id are E402 even when their windows (§5.14) are disjoint.
- The number of Unicode scalar values in the id MUST satisfy the schema's `id` declaration, or the implicit `id: text(1..64)` when the schema declares none (§4.11). A violation is E413, reported at the `@id` tag or, for a file-stem id, at the instance header.
- `id` MUST NOT be assigned by a body statement (E410). The id is fixed by the header or the file name and nothing else.
- The id is emitted as the second key of every object (§8.2) and is the value used by clones (§5.7), `ref` fields (§4.4.8) and output ordering (§2.7).

### 5.4 Body assignments and paths

Grammar: `body_item`, `body_statement`, `body_block`, `assignment`, `path_assignment`, `multi_path_assignment`, `path`.

```abstract
name: Atlas Search
owner.team: Knowledge Systems
limits.{soft, hard}: 10
```

- The left side ends at the first `:` at bracket depth 0 that is outside a quoted string. Everything after it is the value, verbatim; a later `:` (`window: 12::30`, `link: https://x`) belongs to the value.
- Each path segment is an `identifier` and is normalised. An empty segment (`a..b`, `.a`, `a.`) is E316.
- The value MUST NOT be empty: a statement with nothing after the `:`, or with nothing left after comment stripping (§3.2), is E210.
- Intermediate objects are created as needed. A path that traverses a value already holding a non-object (a scalar, a list or a `#tag` object) is E443.
- A path MUST NOT traverse a **list field**. When a non-final segment names a field the schema declares as a list, the statement is E443 with `note: '{field}' is a list; write its elements with a tuple array (§5.5) or with '#tag' shorthand.` There is no index syntax in an assignment, multi-path or clone path (§4.9): a segment of the form `name[0]` is E316.
- The final segment names the field written. Writing the same path twice in applicable versions is E429 (§5.13); this replaces 0.2.0's silent last-wins behaviour.
- A statement whose leading path segment is `template` or `id` is E410.

**Multi-paths.** `prefix.{a, b}: value` assigns the same value to `prefix.a` and `prefix.b`. The prefix MAY be dotted (`limits.tier.{soft, hard}: 10`). At least one key is REQUIRED (E433). Keys are single identifiers; a dotted key is E316. The value is parsed once and assigned to each key; the resulting assignments are subject to E429 like any others. The `{` of a multi-path key list is the one `{` that joins lines (§3.6), because it is written immediately after a `.`.

**Body blocks.** A path followed by a block brace is sugar for repeating that path as a prefix:

```abstract
Product :: @id.atlas
    owner {
        team: Knowledge Systems
        contact: systems@example.com
    }
```

means exactly

```abstract
Product :: @id.atlas
    owner.team: Knowledge Systems
    owner.contact: systems@example.com
```

- The `{` MUST be the last token of its logical line and the `}` MUST be the first token of a logical line (§3.6). A body block never joins lines.
- Blocks nest; a nested block's path is appended to the enclosing prefix. The resulting paths obey every rule above, including E429, E443 and the depth limit of §3.7.
- A body block contains body statements and further body blocks and nothing else. A clone statement inside a block is E404; an instance header inside a block is E210.
- A body block carries no annotation (E437): annotate the statements inside it.
- A body block is a spelling of a prefix, not a value. An empty block writes nothing, and leaves the group absent (§4.7).

### 5.5 Values

Grammar: `value`, `bare_list`, `list_value`, `single_value`, `tag_object`, `tag_arg`, `tuple_array_assignment`, `tuple_row`.

**Lists.** A list is written either with brackets or as bare comma-separated items:

```abstract
tags: [core, public, ai_ready]
tags: core, public, ai_ready
tags: [
    core,
    public
]
```

All three are the same list. A trailing comma is allowed **inside** brackets and is E442 at depth 0 (§3.6). An empty list is written `[]`. An empty item (`a,,b`) is E441.

A `,` at value-bracket depth 0 **always** separates list items, whatever the declared type of the target field; the parser has no type information (§5.10). A bare value that must contain a `,` MUST therefore be quoted: on a non-list `text` field, `caption: Hello, world` is a two-item list and is E412, while `caption: "Hello, world"` is the string `Hello, world`. E412 for this case carries `note: a ',' outside brackets separates list items; quote the value to include a literal comma.`

A value whose first non-whitespace character is `[` is **always** a list and never bare text: `tags: [core]` is the one-element list `["core"]`, never the string `[core]`. Inside a list, an element whose first non-whitespace character is `[` is E441: nested lists do not exist. An unbalanced or wrongly typed bracket in a value is E203/E204/E205. To write a value that begins with `[`, quote it: `note: "[draft]"`.

An unquoted value never begins with `#` or `"` either: a leading `#` always starts a tagged object, and a leading `"` always starts a quoted string, whose closing quote MUST end the value — trailing text after it is E210. To write a value that begins with `#`, quote it: `color: "#ff0000"`. E412 raised on a `Tag` value whose target is not a group carries `note: a leading '#' starts a tag object; quote the value to write a literal '#'.`

**Tagged objects.** `#name` and `#name(key: value, flag)` are shorthand for an object of a group that declares `@tag` (§4.8):

- `#name` sets the group's tag field to `name` (normalised).
- `key: value` sets the field `key`. The key MAY be a dotted path into the group's own nested groups (`#search(limits.soft: 10)`), and the value MAY be a bracketed list or a nested `#tag` object. A bare comma-separated list is not available inside an argument list, because a `,` there separates arguments.
- A `key` that names no field of the target group is E409, with `{context}` naming the group.
- A bare `flag` argument sets the field `flag` to `true` and is valid only when `flag` is a `bool` field (E412).
- An argument that is neither `key: value` nor a bare identifier is E419.
- Any text after the closing `)` is E210: nothing is silently discarded.
- `#name` used where the target is not a group, or where the group declares no `@tag`, is E418.
- Within one list value, two elements whose tag field normalises to the same text are E444. A keyed list has at most one element per key, which is what makes clone merging (§5.7) and consumer lookup well defined. The rule applies equally to `#name` shorthand elements, to tuple rows and to objects written with dotted paths, and it is applied after wildcard expansion (§5.6).

**Tuple arrays.** A tuple array declares column names once and lists rows:

```abstract
copy(key, value): (en_us, Welcome), (es_es, Bienvenido)
```
```json
"copy": [
  { "key": "en_us", "value": "Welcome" },
  { "key": "es_es", "value": "Bienvenido" }
]
```

- Columns are identifiers and are normalised; a repeated column is E417, and a column that names no field of the element group is E409.
- A tuple array MUST declare at least one column, and every row MUST contain at least one cell. `caps(): ()` is E416 with `note: a tuple array declares at least one column.`
- Rows MUST be separated by depth-0 commas; anything between two rows other than one comma is E210.
- Every row MUST have exactly as many cells as there are columns (E416, naming the expected and actual counts and the column list).
- A cell is a quoted string or bare text. A cell may not contain a list, a `#tag` object or a brace pattern; write those with dotted paths instead.
- The target field MUST be a list group or a list of `$(Schema)` (E412).
- Because rows are separated by depth-0 commas, an unbracketed tuple array is one logical line (§3.6). To spread rows over several physical lines, wrap the row list in brackets: the `[` continues the logical line, and a trailing comma inside brackets is allowed.

```abstract
copy(key, value): [
    (en_us, Welcome),
    (es_es, Bienvenido),
]
```

### 5.6 Enum wildcards

A value that ends with `*` is a **prefix wildcard** when, and only when, it is interpreted against an `enum` type. It expands, in the enum's declared order, to every member whose normalised name starts with the normalised prefix.

Wildcards are available in three places:

```abstract
flags: hat_*                        // enum list field
copy(key, value): (es_*, Bienvenido) // tuple cell whose column is an enum
capabilities: [#s*]                  // #tag shorthand, tag field is an enum
```

- Expansion is part of interpreting a value against its declared type (§5.10): it happens before per-element validation and before single-value-to-list coercion.
- A wildcard that matches **no** member is E415. Silent expansion to nothing does not exist.
- Because `*` is not an identifier character, an enum member can never end with `*`, so a wildcard can never shadow a member.
- For a tuple row or a `#tag` object, expansion clones the whole row/object once per match, preserving the other cells or arguments.
- A wildcard is legal in exactly the three positions above, because each of them replicates a **whole element** of a list: (a) an element of a value assigned to an `enum` **list** field; (b) a cell of a tuple row whose column is an `enum` field, the row being cloned once per match; (c) the name of a `#tag` shorthand whose tag field is an `enum`, the object being cloned once per match. In (b) and (c) the enum field itself is a non-list field; the replication happens at the row or element level. Anywhere else a trailing `*` is an ordinary character: on a non-list `enum` field assigned directly it leaves the value outside the declared vocabulary and is E414, and on a field of any other type it is part of the text.

```abstract
schema Page {
    copy[] {
        key: enum(en_us, es_es, es_mx) @tag
        value: text(1..80)
    }
}
```
```abstract
Page :: @id.home
    copy(key, value): (es_*, Bienvenido)
```
```json
"copy": [
  { "key": "es_es", "value": "Bienvenido" },
  { "key": "es_mx", "value": "Bienvenido" }
]
```

### 5.7 Clones

Grammar: `clone_statement`.

```abstract
Product :: @id.beacon, @status.draft
&atlas.*
&winter_theme.*
    name: Beacon Export
```

**Position.** Clone statements MUST appear immediately after the header, before any body statement (E404). A `&` line anywhere else is an error, not a statement that silently attaches to the next instance.

**Target.** `&<id>` and `&<id>.*` are full clones; `&<id>.<path>` is a partial clone of the subtree at `<path>`. The id is normalised, so clone references are case-insensitive. Rules:

- The target MUST exist (E405), reported with the closest existing id when the edit distance is at most 2.
- The target MUST have the same `template` as the cloning instance (E407). Cross-template clones do not exist in 1.0.
- The target MUST exist in **every** version in which the cloning instance exists: its window (§5.14) MUST contain the cloning instance's window, otherwise E440. A clone statement carries no annotation of its own (E437), so this is decided once from the two windows, before any version is compiled, and a clone never has to copy from an instance that is not there.
- A partial clone whose path is absent on the source contributes nothing for the version being compiled. It is E408 only when the path is absent in the source's authored object for **every** version in which the cloning instance exists — that is, when the path can never exist. The diagnostic names the versions checked.
- Cycles are E406, naming the cycle. Chains deeper than 64 are E209.
- Resolution MUST be memoised: the authored object of each instance is built at most once per compilation and per version, so a clone DAG never costs more than linear time.

**What is copied.** A clone copies the source's **authored data after its own clones**, before defaults, before interpolation and before logic. A field that the source only receives from a schema default or a `derive` is therefore not visible to a partial clone (E408 if named). This keeps cloning a purely syntactic, order-independent operation.

**Merging.** Clones are folded in source order into the (initially empty) authored object using the table below. The instance's own header tags and body statements are then applied by **plain assignment**: the value written at a path replaces whatever the clones left there, with no merging, including for a keyed list. The merge table governs clone-to-clone and clone-to-empty folding only; to extend a cloned keyed list rather than replace it, restate the whole list. E429 counts statements only: a clone and a statement writing the same path are not a duplicate assignment, which is what makes `&base.*` followed by overrides the normal way to write a variant. Folding a source value `s` onto an existing value `t`:

| `t` | `s` | result |
|---|---|---|
| absent | any | `s` |
| object | object | recursive merge, key by key |
| keyed list | keyed list | merge by key (below) |
| anything else | any | `s` replaces `t` |

A **keyed list** is a list field whose element type is a group or a `$(Schema)` declaring `@tag` (§4.8). Its elements are merged by the normalised text of their tag field: for a `#name(...)` element the key is `name`; for an object element it is the value of the tag field; an element with no determinable key is appended. Merging preserves the order of `t`, merges same-key elements recursively, and appends new keys from `s` in `s` order. Every other list replaces wholesale.

The merge is **schema-directed**: to fold two list values the compiler consults the schema of the field being folded, so that it knows whether the field is a keyed list and which field is the key. Cloning is order-independent in the sense that it never depends on file order, on defaults, on interpolation or on logic — not in the sense that it ignores the schema. Enum wildcards in element keys are expanded (§5.6) before folding, so `#s*` folds as the members it names and never under the literal key `s*`; and two elements of one authored list that share a key are E444 (§5.5), so folding can never produce a list with two elements under one key.

**Identity is never cloned.** `template` and `id` always come from the cloning instance. `&other.id` and `&other.template` are E410: the path exists on the source, but it is a reserved envelope key and cannot be written.

**Interpolation after cloning.** Because `$` variables are resolved after all clones are merged (§5.11), a cloned string such as `./textures/$id.png` resolves to the **cloning** instance's id.

### 5.8 Brace file patterns

A value of a `file` or `image` **list** field that contains a `{` is a brace pattern:

```abstract
images: ./textures/{hero,thumbnail}.png
```
```json
"images": ["./textures/hero.png", "./textures/thumbnail.png"]
```

- Interpolation (§5.11) runs **before** brace expansion. `${name}` is consumed by interpolation and is therefore never a brace group; brace expansion then examines only the `{` and `}` characters that were present in the source text, and a `{` or `}` that arrived from a variable's value is a literal character that never delimits a group. `images: ${dir}/{hero,thumbnail}.png` expands into two paths under the directory named by `dir`, whatever that value contains.
- Each `{ … }` group holds one or more alternatives separated by depth-0 commas. Alternatives are trimmed; an empty alternative is E435.
- Several groups produce the **cartesian product**. The leftmost group varies slowest:
  `./art/{red,blue}/{small,large}.png` yields `red/small`, `red/large`, `blue/small`, `blue/large`.
- Groups do not nest; a `{` inside a group is E435.
- One pattern holds at most **8** groups; a ninth is E435. The product of eight groups is already the largest asset list an author writes by hand, and the bound keeps a one-line source from demanding an exponential number of paths.
- A brace pattern in a value of a **non-list** `file`/`image` field is E434.
- For every other field type, `{` and `}` are ordinary characters with no expansion. A quoted value is never a pattern.

**Which diagnostic a brace gets.** The two rules are on different levels and never compete:

| Fault | Diagnostic |
|---|---|
| A `{` or `}` in the value that does not balance | E203 / E205, lexically, for a value of **any** type (§3.6) |
| A balanced pattern with an empty alternative, a nested group, or more than 8 groups | **E435**, on a `file`/`image` list field |
| A balanced pattern on a `file`/`image` field that is not a list | E434 |

E435 is therefore reserved for pattern-level faults: it is reported only where a pattern is expanded, and it never stands in for an unbalanced brace, which the lexer has already refused.
- Expansion happens before per-element validation, so every produced path is extension-checked and existence-checked individually.

### 5.9 Asset paths

`file` and `image` values name assets under the project's `assets` directory (§2.3). There is exactly one resolution rule:

> Strip one leading `./` or `.\` if present. Resolve what remains relative to `<project root>/assets`.

So `./textures/a.png` and `textures/a.png` both resolve to `<project root>/assets/textures/a.png`, and `./assets/textures/a.png` resolves to `<project root>/assets/assets/textures/a.png` — the `assets/` prefix is not special-cased.

A value MUST satisfy all of the following, or E424:

- it is relative: no drive prefix (`C:`), no UNC prefix (`\\host\share`), no leading `/` or `\`;
- it contains no `..` segment;
- it is not empty and contains no NUL.

`/` and `\` are both accepted as separators and are equivalent; `/` is RECOMMENDED so that sources are platform-independent. The resolved path is never allowed to leave the assets directory, so a compiler can never be induced by a source file to probe an arbitrary path.

Diagnostics MUST print the resolved path in project-relative form using `/`, never a canonical or extended-length platform path.

E421 and E424 MUST both carry `note: assets live in '<project root>/assets'; the project root is the parent of the 'data' directory when one exists, and the compiled directory otherwise.` This is the only place an author who put `assets/` in the wrong directory is told the rule.

### 5.10 Type-directed value interpretation

The parser produces **syntax values**; it infers no types. The validator interprets each syntax value according to the declared type of the field it is assigned to. Syntax values are:

| Syntax value | Produced by |
|---|---|
| `Quoted(text)` | a `quoted_string`, escapes already decoded |
| `Bare(lexeme)` | `bare_text`, exactly as written, trimmed |
| `List([...])` | `list_value` or a depth-0 comma list |
| `Tag(name, args)` | `tag_object` |
| `Object(fields)` | dotted paths, multi-paths, tuple rows |

**Interpretation.** *Interpreting* a syntax value against a declared type is one deterministic function that performs, in this order: brace expansion (§5.8), enum wildcard expansion (§5.6), `#tag` expansion and normalisation (§5.5), per-element type, range and enum checking, and finally single-value-to-list coercion (§4.6) and the cardinality check (§4.6). Interpretation is **idempotent**: applying it to an already-interpreted value returns that value unchanged. It happens wherever this document says a value is validated. In particular §7.3 step 3 replaces every authored syntax value with its interpreted value, so logic (§6) always reads interpreted values, and §7.3 step 7 re-runs interpretation over the whole object as a check.

The table below is exhaustive. Every combination not listed is **E412 type mismatch**.

| Declared type | `Bare(l)` | `Quoted(t)` | `List` | `Tag` | `Object` |
|---|---|---|---|---|---|
| `text` | the lexeme `l` verbatim | `t` | list field only | E412 | E412 |
| `int` | `l` MUST be an integer literal (§3.5) → number; else E412 | **E412** — quoting does not coerce | list field only | E412 | E412 |
| `float` | `l` MUST be a float or integer literal → number | **E412** | list field only | E412 | E412 |
| `bool` | `l` MUST be exactly `true` or `false` | **E412** | list field only | E412 | E412 |
| `enum` | normalise `l`; MUST be a member (E414); `*` suffix expands (§5.6) | normalise `t`; MUST be a member (E414) | list field only | E412 | E412 |
| `file` | path text; braces expand on list fields (§5.8) | path text, never expanded | list field only | E412 | E412 |
| `image` | as `file`, plus header probing | as `file` | list field only | E412 | E412 |
| `ref` | normalise `l` as an id; MUST resolve (E431) | same as `Bare` | list field only | E412 | E412 |
| `$(Schema)` | E412 | E412 | list field only | E412 | validate against that schema |
| group | E412 | E412 | list field only | tag shorthand (§5.5) | validate against the group |

Additional rules:

- **Quoted numbers stay text.** `caption: "19.5"` on a `text` field emits the string `"19.5"`. There is no path by which a quoted value becomes a number or a boolean.
- **Unquoted version-like text stays text.** `1.21.5` is not a float literal (§3.5), so on a `text` field it is the string `"1.21.5"` and on a `float` field it is E412.
- **List coercion.** For a list field, a `List` is validated element by element; any other syntax value is validated once as a single element and then wrapped in a one-element array. Coercion happens after validation, never before.
- **Nested lists** are E441 (§4.6).
- The `id` field accepts only the header-derived value; see §5.3.

### 5.11 Interpolation

Interpolation rewrites text values using **root scalar variables**.

**Variables.** After all clones are merged and all of the instance's own statements are applied — and before defaults, before validation and before logic — the authored object's **root** fields whose syntax value is `Bare` or `Quoted` form the variable table. The variable's name is the field's normalised name; its text is the lexeme (for `Bare`) or the decoded content (for `Quoted`). `id` is always a variable. Nested fields, list fields, `Tag` and `Object` fields are not variables.

**Syntax.** Inside every `Bare` and `Quoted` value of the instance, at any depth:

| Form | Meaning |
|---|---|
| `$name` | the value of `name`; `name` is the maximal following run of identifier characters |
| `${name}` | the value of `name`, with explicit boundaries |
| `$$` | a literal `$` |

Rules:

- Substitution is a **single pass** over the original text. Substituted content is never rescanned, so a value that itself contains `$` is inserted verbatim. The result of substitution is **not** re-checked: a `$` produced this way is ordinary text and is neither E425 nor E426. Chained interpolation therefore does not work — write the base value in each field, or use a clone.
- Because `$name` consumes the maximal identifier run, there is no prefix ambiguity: `$id_large` refers to `id_large`, not to `id` followed by `_large`. Use `${id}_large` for the latter.
- An unknown variable is **E425**. A misspelling never survives into output.
- A `$` that is not followed by an identifier character, `{` or `$` is E426. An unterminated `${` is E427.
- Interpolation applies to values only: not to paths, not to field names, not to header tag names, not to schema files.
- The variable table is built **per version**: a field written only by a statement that does not apply to the version being compiled is not a variable in that version, and referencing it there is E425. A value that references a version-scoped field MUST therefore carry an annotation whose applicability set is contained in that field's, for example `label: torch-$tint  @removed(2)`.
- Interpolation MUST NOT be implemented by round-tripping through an in-band sentinel character: no character of the input may be given a meaning this section does not give it. `$$` is recognised by the same single left-to-right pass that recognises `$name` and `${name}`.
- The variable table is built once per instance and per version, and each string is rewritten in one left-to-right pass, so interpolation costs time linear in the total length of the instance's text values plus the size of the table.

```abstract
Product :: @id.atlas
    image: ./textures/$id.png
    label: ${id}_display
    note: "$$99 special"
```
```json
{
  "template": "Product",
  "id": "atlas",
  "image": "./textures/atlas.png",
  "label": "atlas_display",
  "note": "$99 special"
}
```

### 5.12 Strictness

An instance MUST assign only fields its schema declares. Every deviation is an error:

| Condition | Error |
|---|---|
| Unknown field name (with a suggestion when the edit distance is at most 2) | E409 |
| Assignment to `template` or `id` | E410 |
| Required field still absent after logic | E411 |
| Value shape does not match the declared type | E412 |
| Value outside the declared ranges | E413 |
| Value not in the declared enum | E414 |
| Wildcard matched no member | E415 |
| Tuple row arity mismatch | E416 |
| Duplicate tuple column | E417 |
| `#tag` where the target group has no `@tag` | E418 |
| Two statements writing the same path in the same version | E429 |
| Statement applies to no version in which its field exists | E430 |
| Header tag on a group, list group or `$(Schema)` field | E438 |
| `::` outside an instance header | E439 |
| Statement, header tag or clone outside the instance's version window | E440 |
| Nested list, or empty list item | E441 |
| Trailing comma at bracket depth 0 | E442 |
| Path crosses a value that is not an object, or traverses a list | E443 |
| Two elements of a keyed list share a key | E444 |
| List element count outside the declared cardinality | E445 |

The `note: declared fields: {list}.` carried by E409 and E505 lists exactly the fields declared by the schema or group **in scope**, in declaration order, normalised. At root level it never lists `template` or `id`, whether or not the schema declares `id` (§4.11): neither can be assigned (§5.3, §6.4), so naming them would advertise a field the author is forbidden to write. In a group or `$(Schema)` context the list is that group's or that schema's own fields, never the root's, and a group field that happens to be named `id` is listed like any other (§4.11).

There is no lenient mode in Abstract 1.0. `--allow-unknown` does not exist (Appendix A).

### 5.13 `@since` and `@removed` on statements

Grammar: `annotation_list` at the end of `body_statement`.

Given the `Item` schema and the `versions 1..3` declaration of §4.12:

```abstract
Item :: @id.torch
    name: Torch
    glow: true               @since(2)
    legacy_tint: 200
```

The annotation on `glow` is required, because `glow` exists in versions 2 and 3 and the statement is meant for both. The assignment to `legacy_tint` needs none: the field itself exists only in versions 1 and 2, and the statement's applicability set is clipped to the field's existence set.

- Annotations are recognised only as the trailing tokens of a body statement, spelled exactly `@since(<uint>)` and `@removed(<uint>)`. The annotation is removed from the statement **before its value is formed**, so the value of `note: deprecated @removed(3)` is exactly `deprecated`. Because the match is on that exact token shape and never on any other `@word`, a value may end with `@x(1)` or with an unparenthesised `@since` without being annotated. To end a value with the literal text `@since(2)`, quote the whole value: `note: "deprecated @since(2)"`.
- `@since(n)` makes the statement apply to versions `v >= n`; `@removed(n)` to versions `v < n`. Both may appear, in either order; each at most once (E303).
- **The annotations themselves obey §4.12's two rules, on a statement exactly as on a field or on an instance header (§5.14).** `n` MUST be within the project range, otherwise **E603**; and `@removed(n)` MUST have `n` strictly greater than the statement's effective `@since` (the project minimum when the statement carries none), otherwise **E604**. Both are checked before the applicability set is formed, so `glow: true @since(3) @removed(2)` is E604 and never E430, and a statement is never described by a range it does not have. E603 is reported in preference to E430 and E440 (§11.1).
- The statement's **applicability set** is the intersection of its annotation range, the project version range, the existence set of the field it writes (§4.12), and the window of the instance that contains it (§5.14). An empty applicability set is an error: the statement can never take effect. It is **E430** when the intersection of the first three is already empty — the statement contradicts the field — and **E440** when the first three intersect but the instance's window excludes the result — the statement contradicts its own instance. E430 is reported in preference to E440 (§11.1). The same requirement reaches a **header tag**, which is an assignment with no annotation of its own and therefore an empty set only through its instance's window: E440 (§5.2).
- Two statements writing the same path whose applicability sets intersect are E429. Two statements writing the same path with disjoint applicability sets are the supported way to give a field different values in different versions.
- Annotations MUST NOT appear on header tags, clone statements or body blocks (E437). The one construct outside a body statement that carries them is the instance header itself, where they scope the whole instance (§5.14). A clone applies to every version in which its instance exists, but the *content* it copies is the source's authored object for the version being compiled (§7.3 step 1), so a clone of a version-scoped field is itself version-scoped without any annotation; version-specific overrides are written as annotated body statements.

### 5.14 Instance version windows

Grammar: `annotation_list` at the end of `instance_header`.

An instance header MAY end, after all of its header tags, with `@since(n)`, `@removed(n)`, or both, in either order. They scope the **whole instance**, not any single field. With the `Item` schema and the `versions 1..3` declaration of §4.12, in one instance file:

```abstract
Item :: @id.lantern @since(2)
    name: Lantern

Item :: @id.candle @removed(3)
    name: Candle
```

- The **window** of an instance is the intersection of its annotation range with the project version range, exactly as §4.12 defines a field's existence set: `@since(n)` gives `v >= n`, `@removed(n)` gives `v < n`, and an unannotated header gives the whole project range. `n` MUST be within the project range (E603) and `@removed(n)` MUST have `n` strictly greater than the effective `@since` (E604), so a window is never empty. Each annotation MAY appear at most once (E303).
- An instance is compiled once for every version in its window and not at all for any other version (§7.3). In a version outside its window the instance simply does not exist: it contributes no object to `D(v)` (§7.4), it is not validated, its `require` statements do not run, and no diagnostic about it is raised for that version.
- Every body statement's applicability set is intersected with the window (§5.13). A statement whose own range falls entirely outside the window is E440, naming both ranges.
- A clone source MUST exist wherever the cloning instance does (§5.7, E440), and a `ref` value MUST name an instance that exists in the version being compiled (§4.4.8, E431).
- Ids remain unique project-wide (§5.3): a window narrows when an instance exists, never who owns its id. To replace one object with another across a version boundary, give the two instances different ids and let the overlay `removed` list retire the first (§7.5).
- Windows are the only construct that changes *which* objects a version contains. Everything else about an instance — its template, its id, its key order — is the same in every version in which it exists (§7.4).

In the example above, `lantern` is compiled for versions 2 and 3 and `candle` for versions 1 and 2. The base document (§7.5) is version 3, so it carries `lantern` and not `candle`. Reducing them by §7.5 step 1:

- `lantern`'s version 2 object is identical to the base's — `glow` exists in both versions and takes its default, and `legacy_tint` is optional and absent — so that run is discarded. Its only entry is a **removal** over `1..1`, where it does not exist.
- `candle` has no base object at all, and its two objects differ, because `glow` exists only from version 2. It contributes two **replace** entries, over `1..1` and `2..2`.

Grouping by range gives two overlays: `1..1`, whose `data` holds `candle` as it is in version 1 and whose `removed` holds `lantern`; and `2..2`, whose `data` holds `candle` as it is in version 2 and whose `removed` is empty. A consumer running version 1 applies the first overlay only, and so adds `candle` and deletes `lantern`.
---

## 6. Logic (`.abt`)

### 6.1 Binding

Grammar: `logic_decl`.

```abstract
logic Product {
    ...
}
```

- The name MUST be a declared schema (E501), reported with the closest declared name when the edit distance is at most 2.
- A schema MAY have at most one logic block; a second is E502, naming both sites.
- A logic block MAY appear in a different `.abt` file from its schema.
- Logic is attached to a schema, not to an instance file, and runs for every instance of that schema and for every nested object validated against that schema (§6.11).

### 6.2 Statements

Grammar: `logic_statement`.

| Statement | Effect |
|---|---|
| `derive <path> = <expression>` | writes a value onto the object being evaluated |
| `derive? <path> = <expression>` | writes only when the path currently has no value |
| `require <condition> else throw "<message>"` | fails compilation with `<message>` when the condition is false |
| `if <condition> { … } else if <condition> { … } else { … }` | branches; exactly one branch body runs |
| `for $<name> in <iterable> { … }` | runs the body once per element; the iterable MUST be a list field or a literal list (E509) |

Statements execute in source order. Only `derive` and `derive?` mutate; `require`, `if` and `for` never write.

A statement's logical line MAY be broken before an `else`: `}` and `else`, and a `require` condition and its `else throw`, MAY each be written on two physical lines, with any number of blank lines and comment lines between them (§3.6 rule (b), §6.3).

The work a `for` may demand is bounded. §3.7 limits the loop iterations the logic of one instance may execute for one version to 1 000 000, counting one unit for each execution of a `for` body; a program that crosses the bound is **E523** and its message names the bound (§3.7, §10.5).

The elements of a **literal list** iterable are interpreted without a target type, by the literal grammars of §3.5: an integer or float literal is a number, `true` and `false` are booleans, a `quoted_string` is a string, and any other bare text is a string, normalised as an identifier when it is one. Iterating a list field that resolves to no value, or to an empty list, runs the body zero times and is not an error.

```abstract
logic Product {
    derive .shipping_class = standard
    derive? .release_wave = 1

    if .status == "active" {
        require .owner.contact exists
            else throw "Active products need an owner contact."
    } else if .status == "retired" {
        derive .visibility = hidden
    } else {
        derive .visibility = internal
    }

    require not .flags contains "banned"
        else throw "Banned products cannot ship."
}
```

### 6.3 Blocks and `else`

- A block opens with `{` at the end of the statement's logical line and closes with `}`.
- `}` and a following `else` MAY share a physical line (`} else {`) or MAY be written on two lines (`}` then `else {`), and a `require` condition and its `else throw` MAY likewise be written on two physical lines: inside a logic block a line terminator immediately before `else` does not end the logical line (§3.6, rule (b)).
- **Blank lines and comment lines MAY sit between the `}` and the `else`.** Rule (b) of §3.6 tests the next token, and neither a blank line nor a `//` comment line produces one, so any number of them may separate a `}` from its `else`, a `}` from its `else if`, and a `require` condition from its `else throw`. This is not a third continuation rule: it is what "the next token is `else`" means.
- An `else` that does not immediately follow a closing `}` of an `if` is E517: an `else` that begins a logical line of its own — at the top of a block, or after a block that is not an `if` — has no `if` to attach to. An `else` that follows some **other** statement is a different fault: rule (b) joins it to that statement rather than ending the line, so the malformed logical line is reported where it is malformed, as E210 at the `else` or at the token the statement was still expecting. Neither spelling is ever silently accepted.
- Blocks nest to the limit in §3.7 (E209), and the iterations their `for` statements execute are bounded by the work limit of §3.7 (E523).

```abstract
logic Item {
    if .status == "active" {
        derive .visibility = public
    }

    // the else may sit past a blank line and a comment
    else if .status == "retired" {
        derive .visibility = hidden
    }

    else {
        derive .visibility = internal
    }

    require .name exists

        // and so may the else of a require
        else throw "An item needs a name."
}
```

### 6.4 `derive` and `derive?`

Grammar: `derive_statement`, `derive_expr`.

`derive <logic_path> = <derive_expr>` writes the expression's value at `<logic_path>` on the object being evaluated.

- The target MUST be a `root_path` (it starts with `.`); a loop-relative target is E505.
- Every segment MUST name a field declared by the schema in scope (E505). Missing intermediate group and `$(Schema)` objects are created.
- The target MUST NOT be `.template` or `.id` (E504).
- The target MUST NOT traverse a list field, and MUST NOT use an index (E506). Logic cannot rewrite one element of an array.
- The target MUST NOT use a loop variable as a segment: `derive .slots.$slot.mode = fixed` is E520. Loop variables are read positions (§6.5) and expression values, not write positions, so that every write target can be checked against the schema at P3. To write several fixed fields, write one `derive` for each.
- The right-hand side is a `derive_expr`. A leading adjacent `calc(` selects
  the numeric expression grammar in [ARITHMETIC.md](ARITHMETIC.md). Otherwise
  it has one of these five forms:
  - a `logic_path` (`.owner.team`, `$slot.mode`), which contributes its resolved value. **The type follows the source**: the written value keeps the type of the value the path resolved to, and is not re-read as text. A path that resolves to no value makes the statement write nothing and report nothing; a path that projects several values is E519.
  - `length(<path>)`, which yields a number under §6.10;
  - the built-in `version`, which yields the integer version being compiled (§6.6);
  - a loop variable (`$n`), which keeps the variable's type: `derive .slot_count = $n` on an `int` field writes a number;
  - any other `value` (§5.5) — a literal, or text carrying `$` interpolation — interpreted by the target field's declared type exactly like an authored value (§5.10), including list coercion, enum normalisation and wildcard expansion, and interpolated as in §5.11 when it is a string containing `$`.
- Apart from the explicit `calc(...)` form, the right-hand side is read as one of the first four forms when its first token is `.`, a loop variable, the keyword `length` or the keyword `version`; otherwise it is a value. To write the literal text `.owner.team`, `length(.tags)`, `calc(1 + 2)` or `version` into a `text` field, quote it.
- The result MUST be assignable to the target field's declared type, or E412. `derive .slot_count = length(.slots)` on an `int` field writes a number; `derive .label = .title` on a `text` field copies the title; `derive .schema_version = version` on an `int` field writes the version being compiled.
- A `derive` inside a `for` body executes once per element and writes its target each time; the last write wins. Abstract has no accumulation operator and no list append: a list is assigned in the instance, or derived once from a single expression. A `derive` whose target and value both ignore the loop variable is therefore equivalent to the same statement written once outside the loop.
- A `derive` or `derive?` that writes into an absent `@optional` group makes that group present (§4.7), and the group's own defaults and required fields are then filled and checked in §7.3 step 7.
- When a `derive` or `derive?` **executes** and its target field does not exist in the version being compiled (§4.12, §5.14), it is **E521**. Nothing in the language is silent: a write that cannot land is an error, not a skipped statement. The error is raised at execution, so a statement that never executes never raises it — a statement in a branch that is not taken, or in the body of a `for` over an empty list, writes nothing and reports nothing. Version-scoped logic is therefore guarded:

```abstract
if version >= 2 {
    derive .glow = true
}
```

  The guard may be any condition; `version` (§6.6) is the direct spelling, and `.field exists` guards a read (§6.7). E521 names the field, the version being compiled and the versions in which the field exists.

`derive?` performs the same write only when the path currently resolves to no value. Because defaults are applied **before** logic (§7.3), a `derive?` whose target field declares a default could never fire; that is E518. `derive?` is therefore meaningful exactly for required fields not yet supplied and for optional fields with no default.

### 6.5 Paths, projections and indices

Grammar: `logic_path`, `root_path`, `loop_path`, `logic_segment`.

- `.a.b` starts at the object being evaluated.
- `$x.a` starts at the value bound to the loop variable `$x`.
- A segment MAY be a loop variable: `.slots.$slot.mode`. The variable's value is converted to a segment by taking its text (numbers and booleans use their literal spelling, strings are normalised) and is then used as a field name. A path containing a loop variable as a segment is checked in two stages. In P3, every **literal** segment MUST name a declared field of the schema in scope at that point (E503); the schema in scope after a dynamic segment is the set of declared types of the fields at that position, and every following literal segment MUST be declared by at least one of them (E503). At run time, a dynamic segment whose text names no declared field resolves to **no value** and is never an error.
- A segment MAY carry an index: `.items[0].id`. Indices are 0-based, apply to list fields, and are valid only for reading. An out-of-range index resolves to no value; it is not an error.
- Every segment MUST name a field declared by the schema in scope, considering the **union** of all versions (E503). A field that exists in some versions and not others resolves to no value in the versions where it does not exist; it is never an error.

**Projection.** A path that crosses a list field without an index resolves to one value per element:

```abstract
.capabilities.id contains "search"
```

resolves `.capabilities.id` to the `id` of every element and is true when **any** of them equals `search`.

A projected path is a legal operand only of `contains` and of `exists`:

- `<projected path> contains <scalar>` is true when at least one projected value equals the scalar under §6.8;
- `<projected path> exists` follows §6.7.

`==`, `!=`, `<`, `<=`, `>` and `>=` require each operand to resolve to **at most one** value. An operand whose path crosses a list field without an index can resolve to several, and is E519 — a static check, made in P3 from the schema alone and independent of any instance. `require not .caps.id contains "banned"` is therefore the only spelling of "no element is banned"; `.caps.id != "banned"` is rejected rather than silently meaning "some element is not banned".

### 6.6 Expressions and precedence

Grammar: `condition`, `or_condition`, `and_condition`, `not_condition`, `comparison`, `operand`.

| Level | Construct | Associativity |
|---|---|---|
| 1 (tightest) | `( … )`, `length( … )`, path, loop variable, the built-in `version`, literal | — |
| 2 | postfix `exists` | — |
| 3 | `==` `!=` `<` `<=` `>` `>=` `contains` | non-associative |
| 4 | prefix `not`, `!` | right |
| 5 | `and`, `&&` | left |
| 6 (loosest) | `or`, `\|\|` | left |

- `and` binds tighter than `or`: `a or b and c` is `a or (b and c)`. Parentheses override.
- `not` applies to a whole comparison: `not .a == .b` means `not (.a == .b)`, and `not .flags contains "banned"` means `not (.flags contains "banned")`. To negate an operand, parenthesise it: `(not .a) == .b`.
- Comparisons do not chain: `a < b < c` is E512.
- A condition MUST evaluate to a boolean. A bare path is a valid condition only when it names a `bool` field; anything else (a bare `text` field, a bare `length(...)`, a bare literal) is E513. Abstract has no truthiness.
- A malformed condition is E511, whose message quotes the condition as written and names the reason. Exactly two reasons are reachable, and together they are the whole of E511: a token that **cannot begin an operand** where an operand is due — `.a === "b"`, whose third `=` lands in operand position, `if .a == {`, and a condition that starts with `and` — and a token that is **not an operator** where the condition should have ended or continued, as in `.a == "b" .c` and `(.a) xor (.b)`.
- An unbalanced parenthesis is **not** E511. `(` and `)` are value brackets (§3.6), so an unclosed `(` is E203, a `)` with nothing open is E205 and a `]` closing a `(` is E204; each is reported while the file is lexed, before any condition is parsed, and §11.1's precedence keeps it (P1 precedes P3). A condition can therefore never reach the parser with its parentheses out of balance.
- `!` is the spelling of `not`; `!=` is always the inequality operator and is never parsed as `!` followed by `=`.

**The `version` built-in.** `version` is an operand whose value is the **integer version currently being compiled** (§7.4). It is the only built-in value in the logic language.

- It is spelled as the bare word `version`, with no `.` and no `$`. A field is always addressed with a leading `.`, so `version` and `.version` are two different things: the built-in, and a declared field named `version`. A quoted `"version"` is the literal text.
- Its type is `int`, so it compares with numbers (`version >= 2`), may be compared with a `.path` that resolves to a number, and may be written by a `derive` onto an `int` field (§6.4). Bare `version` is not a condition (E513): Abstract has no truthiness.
- It is available in every condition and in every `derive_expr`, at any depth, inside `if`, `for` and `require` alike.
- It is not a variable: it is never bound, never shadowed, and `$version` is E510 unless some enclosing `for` binds a loop variable of that name.
- Because P4 compiles each version separately (§7.4), `version` is a constant within one evaluation, and logic remains a pure function of the object, the schemas and the version (§6.12).

```abstract
logic Item {
    if version >= 2 {
        derive .glow = true
        require .name exists else throw "A version 2 item needs a name."
    }
}
```

### 6.7 Presence: `exists`

`<operand> exists` is a presence test on compiled data. **It never touches the filesystem**, for any field type, in any mode. It is true when:

- the path resolves to at least one value, **and**
- it is not the case that the path resolves to exactly one value that is an empty list.

Everything else is true. In particular an absent optional field is false, an empty list is false, an empty string is **true** (it is a present value), and a projected path that yields at least one value is true.

On-disk existence of `file` and `image` assets is a schema check (§4.4.6, §4.4.7), not a logic operator, and is controlled by `--skip-assets` (§9.5). This separation is what makes compiled data independent of `--skip-assets`.

### 6.8 Comparison and string semantics

- **Absent operands.** An operand that resolves to **no** value — an absent optional field, a field that does not exist in the version being compiled, an out-of-range index, or a path through an empty list — makes every comparison, `contains` and ordering test in which it appears **false**, including `!=`. It is never an error. Only `exists` (§6.7) distinguishes absence from a false comparison, so logic that must run in versions or instances where a field may be absent guards it with `exists` (§4.12).
- Both operands of a comparison MUST resolve to at most one value; an operand that can project several is E519 (§6.5).
- `==` and `!=` compare two **scalars** (string, number, boolean). Comparing a list or an object is E513.
- Two strings are equal when they are the same sequence of Unicode scalar values. Comparison is **exact**: case and `-`/`_` differences matter, and no folding of any kind is applied. Enum values are stored normalised, so `.status == "active"` is the correct spelling for an enum field, and `"Active"` never matches — it is simply false, not an error. Exact comparison is the only rule under which a `text` field compares as its own bytes.
- Numbers compare numerically; an `int` and a `float` may be compared. A number is never equal to a string: `.count == "2"` is `false`, not an error, because both operands are scalars.
- Booleans compare to booleans.
- `<`, `<=`, `>`, `>=` require both operands to be numbers (E513). There is no lexicographic ordering of strings and no coercion of numeric-looking strings.
- `contains`:
  - left is a list → true when at least one element equals the right operand under the rules above;
  - left is a string and right is a string → true when the right is a substring of the left, compared exactly;
  - any other combination → E513.

### 6.9 Variables

The only variables are loop variables, introduced by `for $name in …`. They are spelled with a leading `$` everywhere they are used. A bare word is never a variable: in `for $slot in [1,2,3]`, `slot` alone is not bound.

- A quoted string in a condition is always literal text. `"$kind"` is the five-character string `$kind`, never the value of a loop variable named `kind`; to compare against a variable, write `$kind` unquoted. Interpolation (§5.11) applies to `derive` values and to `throw` messages only, never to conditions.
- A loop variable is in scope inside its own block only.
- Shadowing is E516: a nested loop MUST NOT reuse an enclosing loop's variable name.
- Loop variables may be used as operands, as dynamic path segments (§6.5), in `derive` values, and in `throw` messages. A `$name` used as an operand or as a path segment that no enclosing loop binds is E510.
- Root fields are addressed as paths (`.name`), not as variables, inside logic. A `$name` inside a `derive` value or a `throw` message that no enclosing loop binds resolves against the **current** object at the moment the statement runs — after defaults and after every earlier logic statement — not against the authored table of §5.11. It MUST name a root field whose current value is a scalar: a name that is not a declared root field is E503 at P3, and a declared root field with no current value is E425 when the statement runs.
- **Interpolating a non-scalar is E522.** Interpolation inserts *text*, so a `$name` **inside** a `derive` value or a `throw` message MUST name a scalar. When the name is bound — by an enclosing loop, or by a root field of the current object — but its value is a list, an object or a tag object, there is no text to insert, and the reference is **E522** at the statement. It is never dropped silently and never falls through to a same-named root field. A loop over a list of groups binds the group objects, so with `for $c in .caps`:

```abstract
derive .label = cap-$c                     // E522: $c is an object
require .name != "" else throw "bad $c"    // E522, for the same reason
derive .label = $c.id                      // fine: a path (§6.5), not interpolation
```

  The rule is about interpolation only. A loop variable written as the **whole** right-hand side is the fourth `derive_expr` form of §6.4 and keeps its own type, so it is E412 against a scalar field rather than E522. A loop over a list of scalars is unaffected in either position.

### 6.10 Functions

`length(<path>)` is the function available directly in ordinary logic expressions.
Additional numeric functions are available inside `calc(...)` (see
[ARITHMETIC.md](ARITHMETIC.md)).

- On a list value it yields the number of elements.
- On a string value it yields the number of Unicode scalar values.
- On a path resolving to several projected values it yields the number of values.
- On a path that resolves to no value it yields `0`. Absence is not an error here (§6.5).
- E514 is a **static** check, made in P3: the argument's *declared* type MUST be a list field or a `text` field, or a loop variable bound to a list or to text. `length()` of an `int`, `float`, `bool`, group, `$(Schema)` or `ref` field is E514 before any instance is read.
- `length(...)` is an operand, never a whole condition (§6.6).

### 6.11 Evaluation order

For one object — a root instance or a nested group, `$(Schema)` value or list element — and one version, the order is fixed. It is the same order as steps 3 to 7 of §7.3, seen from one object:

1. authored value type checks (§5.10) — every value the author wrote is validated against its field's declared type;
2. defaults — every absent field that declares a default is filled and the default is validated;
3. logic — nested first (below), then this schema's block;
4. required-field check — every required field of the object MUST now have a value, else E411. The check is applied recursively to every **present** group value and every present `$(Schema)` value, after that nested object's own logic has run; an absent optional group is not entered (§4.7);
5. full re-validation — the whole object, including everything logic wrote, is validated again from scratch: types, ranges, enums, tag normalisation, list coercion, asset checks and unknown-field rejection.

Step 4 runs after step 3 so a `derive` may satisfy a required field. Step 5 runs after step 3 so nothing logic writes can escape validation; in particular a `derive` to a field the schema does not declare is impossible (E505 at parse time) and a `derive` of an out-of-range value is E413.

**Nested schema logic.** Before a schema's own logic runs, logic runs for every nested object validated against a *named* schema: for each field of type `$(Other)` in schema declaration order, and for each element of a list of `$(Other)` in element order, `logic Other` is evaluated with `.` bound to that nested object. Inline groups are not named schemas and have no logic.

### 6.12 Determinism

Logic evaluation is a pure function of the authored object, the schema set and the version being compiled. It performs no I/O, does not observe the filesystem (§6.7), does not observe wall-clock time, locale or environment, and visits statements, branches, loop elements and nested objects in the fixed orders defined above. Two runs on the same bytes MUST produce the same result, including the same first failing `require`.

`require` failure aborts the instance immediately with E515 and the author's message; no later statement in that instance runs. Compilation as a whole fails.

Every logic diagnostic carries the position of the statement in its `.abt` file and a note naming the instance and the version being compiled. A diagnostic raised inside a `for` body additionally carries `note: at index {i}, item {item}.`, counting from 0 in iteration order — one such note for each `for` the statement is inside, written outermost first, so that the last one names the innermost loop.

### 6.13 What `derive` may write, summarised

| Target | Allowed |
|---|---|
| A declared scalar field of the schema in scope | yes |
| A declared field inside a group or `$(Schema)`, creating missing intermediate objects | yes |
| A list field, as a whole value | yes |
| One element of a list (`.items[0].x`) | no — E506 |
| `.template`, `.id` | no — E504 |
| A field the schema does not declare | no — E505 |
| A field that does not exist in the version being compiled | no — E521 when the statement executes; a statement that does not execute writes nothing and reports nothing |
| A path containing a loop-variable segment | no — E520 |

and what it may write:

| Right-hand side | Allowed |
|---|---|
| A literal value, a `$` variable, or a string with interpolation | yes |
| A value read from another path of the same object, or `length(...)` | yes |
| The built-in `version` | yes — the integer version being compiled |
| A path that projects several values | no — E519 |
| A `$` **interpolated into text** whose value is a list, an object or a tag object | no — E522 (§6.9); the same variable written as the whole right-hand side keeps its own type |
---

## 7. Compilation pipeline, versions and overlays

### 7.1 Phases

Compilation is a fixed sequence of phases. A phase runs only if every earlier phase succeeded. Within a phase, diagnostics MUST be reported in the deterministic order defined for that phase, and an implementation MAY report several diagnostics from the same phase before stopping.

```
P0  discovery          collect and sort source files            (§2.4)
P1  lex + parse        one AST per source, with positions       (§3, §4, §5, §6)
P2  project tables     schemas, logic blocks, versions, instances
P3  schema validation  every schema and logic block is checked  (§7.2)
P4  per-version build  compile every instance for every version
                       in which it exists                       (§7.3, §7.4)
P5  overlay reduction  base document + changed objects          (§7.5)
P6  rendering          JSON, YAML or RAW                        (§8)
```

**P1 is one phase.** An implementation that reports several diagnostics reports the lexical and the syntactic ones together, in the single order §11.1 fixes, so a lexical defect late in a file never hides a syntactic defect earlier in it. A line the lexer could not tokenize contributes only its lexical diagnostic: whatever the parser then makes of the recovered tokens on that line is a consequence of the same defect, not a second one.

### 7.2 P2 and P3 in detail

**P2 — project tables.** Built in the source order of §2.4:

- `schemas`: schema name to declaration. A repeat is E301.
- `logic`: schema name to logic block. A repeat is E502; an unknown name is E501.
- `versions`: at most one declaration (E601), validated (E602). Absent means `1..1`.
- `instances`: normalised id to declaration. A repeat is E402. Every instance's template MUST be a declared schema (E401). Every id MUST be an identifier (E428); an id that is not one names nothing and therefore takes no part in the uniqueness (E402) or length (E413) checks. Every instance's window (§5.14) is validated against the project range (E603, E604), and every id against the applicable `id` range (E413, §4.11). Every body statement's own annotations are validated the same way and against the same two identifiers (E603, E604; §5.13). Once every window is known, each clone source's window MUST contain its cloning instance's (E440), and each **assignment** — every body statement and every header tag — MUST have a non-empty applicability set (E430, then E440; §5.2, §5.13). These checks cover **every** discovered instance, not only the ones selected for output (§2.5).

**P3 — schema validation.** For every schema, in the source order of §2.4, and within a schema in field declaration order:

- field names, modifiers and types are well formed (E302–E322);
- `$(Schema)` and `ref(Schema)` targets resolve (E309);
- `@tag` placement and arity are legal (E310, E311, E322);
- every default is validated against its own field's type exactly as an authored value would be (E313);
- `@since` / `@removed` are within the project range and non-empty (E603, E604, E605);
- list cardinalities are well formed (E323);
- group nesting depth is within the limit of §3.7 (E209); a cycle of `$(Schema)` references is E321 when **every** edge of the cycle is a required, non-list `$(Schema)` field, because no finite object satisfies it. A cycle in which at least one edge is `@optional` or a list field is legal, and instances of it are bounded by the instance depth limit of §3.7 (E209), which is checked while the object is built.

For every logic block, in the same order: the schema exists (E501); every **literal** path segment names a declared field, and a path with a loop-variable segment is checked as §6.5 prescribes (E503); every `derive` target is legal (E504, E505, E506, E518, E520); every `require` has an `else throw` with a quoted message (E507, E508); every condition is well formed and boolean (E511, E512, E513); no operand of a comparison can project several values (E519); every `length()` argument has a list or `text` declared type (E514); every loop variable is unshadowed (E516).

P3 depends on no instance. A project with zero instances is still fully schema-checked; `abstract lint` on such a project reports every schema defect.

### 7.3 P4 — compiling one instance for one version

For version `v` and instance `I`, in the order of §2.7. `I` is compiled for `v` only when `v` lies in `I`'s window (§5.14); when it does not, none of the steps below runs, `I` contributes no object to `D(v)` (§7.4), and nothing about `I` is validated or reported for that version.

1. **Authored object.** Start from an empty object. Fold `I`'s clone statements in source order (§5.7). Then apply `I`'s header tags in header order, and then `I`'s body statements in source order, keeping only statements whose applicability set contains `v` (§5.13). Header tags and body statements are applied by **plain assignment** at their path, never by the clone merge of §5.7. `template` and `id` are set from the header and the file name (§5.3) and are never taken from a clone. A header tag, clone path or body statement whose leading segment is `template` or `id` is **E410**: it is refused here, writes nothing, and therefore takes no part in P2's duplicate-assignment check (E429) either.
2. **Interpolation.** Build the variable table from the authored object's root scalar fields and resolve every `$` reference in the authored object (§5.11). The table is retained for step 4, where defaults are interpolated against it.
3. **Authored type checks and interpretation.** Interpret every present value against the declared type of its field (§5.10) — brace expansion, enum wildcard expansion, `#tag` normalisation, per-element type, range and enum checks, then list coercion and the cardinality check — considering only fields that exist in `v` (§4.12), and replace each authored syntax value with its interpreted value. An assignment to a field that does not exist in `v` cannot occur, because such statements are rejected before P4 (E430, E440).
4. **Defaults.** Walk the object top-down. For every field that exists in `v`, whose parent object is **present** (§4.7), that is absent, and that declares a default: interpolate the default (§5.11), then set and validate it. An absent `@optional` group or `$(Schema)` field is not walked into, so defaults inside it are not filled and it stays absent. The root object is always present.
5. **Logic.** Evaluate nested schema logic, then this schema's logic (§6.11). The loop iterations every block run here executes are charged against this instance-version's work budget, and crossing it is E523 (§3.7).
6. **Required-field check.** Every field that exists in `v` and is neither `@optional` nor defaulted MUST have a value (E411).
7. **Full re-validation.** Interpret and validate the whole object again (§5.10): types, ranges, enums, cardinality, `#tag` normalisation, list coercion, `ref` resolution, unknown-field rejection, and — unless `--skip-assets` is in force — `file` and `image` on-disk checks. Interpretation is idempotent, so nothing produced in step 3 changes here; in particular a brace pattern expanded in step 3 is now an ordinary path and is not expanded again.
8. **Key ordering.** Reorder the object as §8.3 prescribes.

The depth limit of §3.7 is enforced here, and only here: steps 3 and 7 walk the resolved tree, counting the instance object as level 1 and every object and list below it as one further level, and report E209 at the statement that reaches the limit, or at the instance header when nothing narrower is known. No later step re-checks it (§3.7).

Steps 3, 4, 6 and 7 are **recursive**: they are applied to every present nested object — group values, `$(Schema)` values and every element of a list of either — in schema declaration order and element order, parent before child, using the schema or group that declares them and the same version `v`. Step 5 orders nested logic as §6.11 prescribes. A value that logic writes into a nested object is defaulted and required-checked during step 7, which re-runs steps 3, 4 and 6 over the whole tree; step 7 is therefore the only step whose failure can be caused by logic.

Clone sources are compiled with the same procedure and are memoised per `(instance, version)`. A clone source that is not itself emitted is nevertheless fully validated, so single-file mode and whole-project mode agree (§2.5).

### 7.4 Versions

Let the project range be `lo..hi` (§4.12). P4 runs once for every `v` in `lo..hi`, in **ascending** order of `v`, producing a document `D(v)`: the list of compiled instance objects, ordered by §2.7. The order matters only for diagnostics: the first failure reported is the one in the lowest version.

Facts that follow from the rules:

- `D(v)` contains exactly the instances whose window (§5.14) contains `v`, in the order of §2.7. An instance with no window annotation is in every `D(v)`; version annotations scope fields, statements and whole instances, and nothing else.
- An instance's `template` and `id` are identical in every `D(v)` that contains it.
- The number of objects in `D(v)` varies with `v` only through instance windows; their relative order never does, because §2.7 orders by `(template, id)` alone.
- A field annotated `@since(n)` is absent from `D(v)` for `v < n`; a field annotated `@removed(n)` is absent for `v >= n`.

### 7.5 P5 — base document and overlays

The compiled document carries `D(hi)` as `data` — the **base** — and carries every earlier version that differs from it as **whole replacement objects** and **removed ids**. Grouping is per instance, not per document: each instance is reduced on its own, and instances that reduce to the same version range share one overlay.

**Structural equality.** Two objects are structurally equal when they have the same keys in the same order, the same kinds, the same scalar values, the same lengths, and are equal element by element and key by key. Two numbers are structurally equal when they render identically under §8.7, so `0.0` and `-0.0` are not equal. Write `⊥` for "this instance does not exist in this version" (§5.14); `⊥` is equal to `⊥` and to nothing else.

**Step 1 — per-instance runs.** For each instance `I`, in the order of §2.7, let `O(I, v)` be `I`'s compiled object in `D(v)`, or `⊥` when `v` is outside `I`'s window, and let `B(I)` be `O(I, hi)` — what the base carries for `I`.

Scan `v` from `lo` to `hi-1` and split that interval into **maximal runs** on which `O(I, v)` is constant under structural equality. Discard every run whose value equals `B(I)`; each surviving run is one entry:

| Run value | `B(I)` | Entry |
|---|---|---|
| an object | a different object, or `⊥` | **replace** `I` with that object over the run's range |
| `⊥` | an object | **remove** `I`'s id over the run's range |
| equal to `B(I)` | — | discarded: the base already says it |

An entry's object is a **complete instance object**, rendered exactly as it would be in `data` (§8.2, §8.3), including `template` and `id`. There is no field-level delta and no `null`: a field that does not exist in `v`, or that has no value in `v`, is simply absent from the object.

Because the runs of one instance partition `lo..hi-1`, an instance contributes at most one entry per version, and the ranges of the entries of one id are **disjoint**.

**Step 2 — grouping into overlays.** Collect every entry from step 1 and group them by their range. Each distinct range that has at least one entry produces exactly one overlay:

```json
{ "versions": { "min": 1, "max": 2 }, "data": [ … ], "removed": [ … ] }
```

- `versions` is the range, with `min` and `max` in that order.
- `data` holds the objects of that range's **replace** entries, ordered by `(template, id)` as §2.7 prescribes.
- `removed` holds the ids of that range's **remove** entries, ascending as sequences of Unicode scalar values. Ids are unique project-wide (§5.3), so the order is total.
- All three keys are always present, in this order. Either array MAY be empty, but not both: a range with no entry produces no overlay.

Overlays are ordered by `versions.min`, then `versions.max`, then the first id in `data` and then the first id in `removed`. Because step 2 emits one overlay per distinct range, the first two keys already make the order total; the id tie-breaks can never be reached and are stated only to close the definition.

The ranges of two overlays MAY overlap — one instance may change at version 2 while another exists only up to version 1 — but **for any single id, the ranges of the overlays that mention it are disjoint**, which is what step 1 guarantees. An id is mentioned by an overlay when it appears in that overlay's `data` or in its `removed`.

**Step 3 — consumer contract.** To obtain the document for version `v`: start from `data`, then apply **every** overlay whose range contains `v` — there may be none, one or several:

1. for each object in the overlay's `data`, replace the base object with the same `id`, or add it when the base has none;
2. delete every object whose `id` appears in the overlay's `removed`.

Nothing is merged and no field is deleted, so a consumer needs only a lookup by `id`, and an object's key order is the order of the object it took. Because the overlays that mention one id have disjoint ranges, no id is touched twice and the order in which a consumer applies the overlays does not matter. A consumer that wants the canonical order sorts the result by `(template, id)` (§2.7); a consumer that only looks objects up by `id` need not.

**Worked example.**

```abstract
versions 1..3

schema Item {
    name: text(1..40)
    glow: bool @since(2) = false
    legacy_tint: int(0..255) @removed(3) @optional
}
```
```abstract
Item :: @id.torch
    name: Torch
    legacy_tint: 200
```

`O(torch, 3)` = `{template, id, name, glow:false}` (no `legacy_tint`: it does not exist in 3), and this is `B(torch)`.
`O(torch, 2)` = `{template, id, name, glow:false, legacy_tint:200}`.
`O(torch, 1)` = `{template, id, name, legacy_tint:200}` (no `glow`).

Scanning `torch` over `lo..hi-1` = `1..2`: `O(torch, 1)` and `O(torch, 2)` are not structurally equal, so the interval splits into the two runs `1..1` and `2..2`, and neither run's value equals `B(torch)`. `torch` therefore contributes two replace entries, with two distinct ranges, so there are two overlays (shape shown compactly; §8.4 fixes the exact bytes):

```json
"overlays": [
  {
    "versions": { "min": 1, "max": 1 },
    "data": [ { "template": "Item", "id": "torch", "name": "Torch", "legacy_tint": 200 } ],
    "removed": []
  },
  {
    "versions": { "min": 2, "max": 2 },
    "data": [ { "template": "Item", "id": "torch", "name": "Torch", "glow": false, "legacy_tint": 200 } ],
    "removed": []
  }
]
```

A consumer running version 1 applies the first overlay and replaces `torch` with its object; one running version 3 matches no overlay and uses `data` unchanged. `removed` is empty in both because `torch` carries no instance window: it exists in every version (§5.14).

### 7.6 Determinism guarantees

For a fixed set of source bytes, one compiler MUST emit byte-identical output regardless of:

- the order in which the filesystem enumerates directories;
- the order and spelling of command-line roots that resolve to the same project (§2.3);
- the platform, path separator convention, locale, time zone or environment;
- whether `--skip-assets` was passed (§9.5);
- whether the compile was whole-project or single-file, for the instances common to both (§2.5).

Two different implementations, or two releases of one implementation, differ only in the `abstract.compiler` string of §8.1; every other byte is fixed by this document.

Compilation MUST NOT read any file other than the discovered sources and the `file`/`image` assets referenced by validated values, MUST NOT write any file other than the one named by `--out`, and MUST NOT access the network.

---

## 8. Output formats

### 8.1 The document envelope

Every compiled document has exactly three top-level keys, in this order:

| Key | Value |
|---|---|
| `abstract` | an object describing the document itself (below) |
| `data` | the array of compiled instance objects for version `max`, ordered by §2.7 |
| `overlays` | the array of overlays (§7.5), possibly empty |

The `abstract` object has exactly three keys, in this order:

| Key | Value |
|---|---|
| `format` | the document format number, exactly `1`, emitted as a JSON number |
| `compiler` | the semantic version of the implementation that produced the document, as a string |
| `versions` | an object with `min` and `max`, in that order: the project version range (§4.12) |

Each element of `overlays` has exactly three keys, in this order:

| Key | Value |
|---|---|
| `versions` | an object with `min` and `max`, in that order: the version range this overlay covers |
| `data` | the complete instance objects that replace or add to the base over that range, ordered by §2.7 |
| `removed` | the ids of the instances the base carries that do not exist over that range, ascending as sequences of Unicode scalar values |

Either array MAY be empty; an overlay whose `data` and `removed` are both empty is never emitted (§7.5).

`format` changes only when the shape of the document changes; a consumer that understands format 1 understands every document produced by a 1.x compiler. `compiler` identifies the producer and is the only part of a compiled document that is not a function of the source bytes, which is why §11.2 normalises it before comparing goldens.

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [],
  "overlays": []
}
```

An empty `data` array is reachable in exactly two ways, and both of them are **successful** compiles that emit the envelope above with `data` and `overlays` both empty:

- **single-file mode** (§2.5), when the named files declare no instances;
- **a whole-project compile that discovers source files and declares no instance.** Discovery found source files and none of them declares an instance. What those files *do* declare does not matter: a project of `.abt` files that declare schemas and logic is the ordinary shape of it, and a project whose one source is empty, holds nothing but comments, or holds nothing but a `versions` declaration reaches the same document — the project range of the envelope is read from that file like any other (§4.12). §7.2 blesses such a project explicitly: P3 checks every schema and every logic block against no instance at all, so `abstract lint` on it reports `abstract: ok` and exits 0, and `abstract compile` on it emits the empty document.

A project with **no source files at all** is neither: discovery collects nothing, which is E103 (§2.4, §10.1), and no document is emitted. An empty directory is not an empty project.

### 8.2 The instance object

Every compiled instance object begins with:

| Key | Value |
|---|---|
| `template` | the schema name from the header, exactly as declared |
| `id` | the normalised instance id (§5.3) |

followed by the schema's fields. `template` and `id` are produced by the compiler; no authored construct can change them (§4.11, §5.3). A schema that declares `id` explicitly (§4.11) changes nothing here: the id is emitted once, in this position, and the declaration only constrains its length.

### 8.3 Key ordering

Object keys are emitted in this order:

1. for an instance object: `template`, then `id`;
2. then every field the schema declares, in **declaration order**, skipping fields that are absent and skipping a declared `id` field, which was already emitted in step 1 wherever it stands in the declaration;
3. recursively, a group value and a `$(Schema)` value order their keys by the declaration order of that group or that schema.

There are no other keys: unknown fields cannot reach the output (§5.12). Key order therefore never depends on assignment order, clone order, or the order in which logic wrote values.

### 8.4 JSON

- Indentation is two spaces per level. A non-empty object or array puts each member on its own line; the closing bracket is at the parent's indentation.
- An empty object renders as `{}` and an empty array as `[]`, on one line.
- The separator between a key and its value is `": "`; members are separated by `,` at end of line.
- The document ends with exactly one `LF`. There is no BOM and no trailing whitespace on any line.
- Keys are JSON strings, escaped as in §8.8.

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "price": 19.5,
      "featured": true,
      "tags": [
        "core",
        "public"
      ]
    }
  ],
  "overlays": []
}
```

### 8.5 YAML

The YAML form is a block-style rendering of the same tree.

- Indentation is two spaces per level.
- **Every mapping key is double-quoted**, always, including `data`, `id` and numeric field names. This makes a field named `1`, `yes`, `no`, `on` or `null` unambiguous.
- Scalar strings are double-quoted and escaped as in §8.8. Numbers and booleans are emitted bare.
- A non-empty sequence is written as block items indented two spaces past its key; each item begins with `- `. For an object item, the first key follows `- ` on the same line and the remaining keys align under it.
- An empty sequence is `[]` and an empty mapping is `{}` on the key's line. An empty mapping that is a sequence item is written `- {}`, and an empty sequence that is a sequence item is written `- []`.
- The document has no `---` marker and ends with exactly one `LF`.

```yaml
"abstract":
  "format": 1
  "compiler": "1.0.0"
  "versions":
    "min": 1
    "max": 1
"data":
  - "template": "Product"
    "id": "atlas"
    "price": 19.5
    "featured": true
    "tags":
      - "core"
      - "public"
"overlays": []
```

### 8.6 RAW

RAW (`.abraw`) is a review format: the same data with unquoted keys and wider indentation, for diffs and code review. It is **not** intended to be machine-parsed, and no Abstract tool reads it back.

- Indentation is four spaces per level.
- The envelope object is rendered without its outer braces: one `key: value` entry per line at indentation 0, in envelope order, separated by `,` at end of line.
- Keys are written bare, without quotes. Every key is an identifier or an envelope key, so no escaping is needed.
- An object is **always** multi-line: `{`, then one `key: value` per line at the next indentation level separated by `,`, then `}` at the object's own indentation.
- An array whose elements are all scalars is inline: `[a, b, c]`. An array with at least one object or array element is multi-line: `[`, one element per line at the next level separated by `,`, then `]`.
- An empty array is `[]`; an empty object is `{}`.
- Scalars use the same spelling and escaping as JSON (§8.7, §8.8), including double quotes around strings.
- The document ends with exactly one `LF`.

```text
abstract: {
    format: 1,
    compiler: "1.0.0",
    versions: {
        min: 1,
        max: 1
    }
},
data: [
    {
        template: "Product",
        id: "atlas",
        price: 19.5,
        featured: true,
        tags: ["core", "public"],
        capabilities: [
            {
                id: "search",
                availability: "stable"
            }
        ]
    }
],
overlays: []
```

### 8.7 Number formatting

**Integers.** Rendered in base ten with no leading zeros (except the single digit `0`), with `-` for negative values and never `+`. The full signed 64-bit range is representable.

**Floats.** Rendered as the **shortest decimal string that round-trips** to the same binary64 value, then formatted as follows:

- If the value is `0`, render `0.0` (or `-0.0` for negative zero).
- Otherwise, if `1e-6 <= |value| < 1e21`, render in plain positional notation. When the shortest form has no fractional digits, append `.0`, so a float is always distinguishable from an integer (`3` renders as `3.0`).
- Otherwise render in scientific notation: one digit, `.`, at least one fractional digit, `e`, an explicit `+` or `-`, and the decimal exponent with no leading zeros (`1.0e+21`, `1.5e-07` is written `1.5e-7`).

Every rendered float is valid JSON and valid YAML. Non-finite values cannot occur: literals that are not finite are rejected at §3.5 and ranges must be finite (§4.5). If an implementation ever reaches a non-finite value it MUST fail with E701 rather than emit an invalid token.

### 8.8 String escaping

In all three formats, a string is emitted between double quotes with exactly these escapes:

| Character | Escape |
|---|---|
| `"` | `\"` |
| `\` | `\\` |
| `U+0008` | `\b` |
| `U+0009` | `\t` |
| `U+000A` | `\n` |
| `U+000C` | `\f` |
| `U+000D` | `\r` |
| `U+007F`, and any other C0 or C1 control character | `\u00XX`, lowercase hexadecimal |

The escaped set is exactly the set §3.5 excludes from bare text — every C0 control, `U+007F`, and every C1 control — plus the quote and the backslash, and it is the same in all three formats. `U+007F` is escaped although it is in neither control range, because YAML excludes it from `c-printable` and a literal one would make an otherwise valid document unreadable (§11.3 requires the suite to parse what it compares).

Every other Unicode scalar value, including all non-ASCII text, is emitted literally as UTF-8. `/` is never escaped.

### 8.9 Absent optional fields

An absent optional field is **omitted** from the output in every format, for every type, including list types. Abstract has no null: no format ever emits `null`, `~` or an empty value for a field. A consumer distinguishes "absent" from "empty" by key presence, and an empty list is a present value spelled `[]`.

**Group presence.** A group value or a `$(Schema)` value is *present* when at least one of its own fields has a value after §7.3 step 7, and *absent* otherwise (§4.7). An absent `@optional` group is omitted; an absent required group is E411, reported at the group. An empty object therefore never appears anywhere in a compiled document: every instance object carries `template` and `id`, every `abstract` object carries three keys, and a group with no values is omitted rather than emitted as `{}`. The `{}` rendering of §8.4, §8.5 and §8.6 is specified for completeness and is unreachable in a conforming document.
---

## 9. Command-line interface

### 9.1 Invocation

```text
abstract <command> [positional …] [flags …]
```

The argument parser is strict. Every token MUST be recognised: an unknown command is E801, an unknown flag is E802, a flag missing its value is E803, an extra or unrecognised positional is E804. Nothing is ever silently ignored. Flags may appear anywhere among the positionals.

There is no "direct form" (no `abstract file.ab Template.abt JSON`): a command word is always required. See Appendix A.

**Extended-length arguments.** An input path MAY be given in the Windows extended-length spelling, with the prefix `\\?\` (or `\\?\UNC\` for a share). The prefix is a request to the platform's path resolver, not part of the path an author wrote, so it is **accepted and stripped** before anything else looks at the argument: `\\?\C:\packs\winter` names the same project as `C:\packs\winter`, and `\\?\UNC\host\share\packs` the same as `\\host\share\packs`. Because the prefix is gone by the time the project is resolved, it never reaches a diagnostic (§9.8). This is a rule about **arguments only**; the same spelling inside a `file` or `image` value is an absolute path and is E424 (§5.9).

### 9.2 Commands

| Command | Synopsis |
|---|---|
| `compile` | `abstract compile <path>… [FORMAT] [--out <file>] [--skip-assets] [--max-errors <n>]` |
| `lint` | `abstract lint <path>… [--skip-assets] [--max-errors <n>]` |
| `templates` | `abstract templates <path>` |
| `init` | `abstract init <directory>` |
| `--help`, `-h`, `help` | print usage to stdout, exit 0 |
| `--version`, `-V`, `version` | print `abstract <semver>` to stdout, exit 0 |

**`compile`** resolves the project (§2.3), compiles it, and renders the document. `<path>…` is either exactly one directory or one or more `.ab` files (§2.5); mixing them, or naming paths that resolve to different project roots, is E807. A named path that does not exist, or whose extension is neither `.ab` nor `.abt`, is E806. A named **directory** MUST be the project root or the data directory (§2.3); a directory that lies inside the data directory is E806, and there is no downward search for a project from anywhere else.

**`lint`** performs exactly the same work as `compile` — including asset checks unless `--skip-assets` — and renders nothing. It writes nothing to stdout and, on success, one line `abstract: ok` to stderr.

**`templates`** prints the schema names declared in the project, one per line, sorted as sequences of Unicode scalar values, to stdout. It runs phases P0–P3 (§7.1), so a project whose schemas are broken fails instead of printing a partial list.

**`init`** creates a project scaffold in `<directory>`. The directory MUST NOT exist or MUST be empty (E813). The scaffold MUST be a valid 1.0 project and MUST exercise the constructs an author meets first: one `.abt` with a `versions` range; a schema using `text`, `int`, `float`, `bool` and `enum`, a group, a list group with `@tag`, one `@optional` field, one default and one `ref`; a `logic` block with one `require` and one `derive`; two instances, the second cloning the first; and one `image` asset under `assets/`. `abstract compile <directory>` on a fresh scaffold MUST succeed with no flags, and `abstract lint <directory>` MUST print `abstract: ok`.

**FORMAT** is `JSON`, `YML`, `YAML` or `RAW`, compared case-insensitively; `YML` and `YAML` name the same format. It is optional; when it is absent the format is JSON.

The last positional of `compile` is the FORMAT when, and only when, at least one other positional is present **and** it compares case-insensitively equal to one of the four keywords. Otherwise the last positional is an input path. Therefore:

| Invocation | Meaning |
|---|---|
| `abstract compile pack` | compile `pack`, format JSON |
| `abstract compile pack JSON` | compile `pack`, format JSON |
| `abstract compile json` | compile the directory `json`, format JSON — a sole positional is always an input path |
| `abstract compile a.ab b.ab RAW` | compile two files, format RAW |
| `abstract compile pack XML` | E805 |

When at least one other positional is present, a last positional that is neither a format keyword nor a valid input path is E805. When it is the sole positional it is judged as an input path only, so a bad spelling there is E806. A directory whose name is a format keyword can be compiled only as the sole positional; naming it alongside another path is not expressible.

`compile`, `lint` and `templates` with no positional at all are E806.

### 9.3 Flags

| Flag | Meaning |
|---|---|
| `--out <file>` / `--out=<file>` | write the document to `<file>` instead of stdout |
| `--skip-assets` | skip on-disk asset checks (§9.5) |
| `--max-errors <n>` / `--max-errors=<n>` | report at most `n` diagnostics (default 20) |

- A flag given twice is E811.
- `--out` with no following value, or `--out=` with an empty value, is E803.
- A flag's value MUST NOT begin with `-`: `--out --skip-assets` is E803, not a request to write a file named `--skip-assets`. A destination that genuinely begins with `-` is written `--out=-name` or `--out ./-name`.
- `--max-errors` requires a decimal integer from **1 to 10000** inclusive; anything else — a non-integer, `0`, a value above the bound, or an integer too large to represent — is E812, whose message names the bound. There is no "no limit" spelling: `--max-errors 10000` is more diagnostics than any run produces in practice, and a bounded flag can never be handed a number the implementation must then refuse in its own words.
- `--max-errors` bounds reporting only. Compilation stops after the `n`-th diagnostic, and the final line on stderr is `abstract: note: stopping after {n} errors; more may remain.` The diagnostics reported are the first `n` in the deterministic order of §11.1, so the flag never changes which diagnostic comes first and never changes the exit code.
- There is no `--allow-unknown` in 1.0 (Appendix A).

### 9.4 `--out`

- The destination is used exactly as written; no extension is appended or checked.
- The destination is refused with E808 when it is a discovered source file, when it lies inside the data directory (if the project has one, §2.3), or when its extension is `.ab` or `.abt` and it lies inside the discovery root. A compiler never overwrites its own inputs and never writes a file it would read on the next run.
- Every other destination is allowed, whichever layout the project uses: `<project root>/out.json` is always acceptable, and `<data directory>/out.json` never is.
- The parent directory MUST exist; a write failure of any kind is E810 and the process exits 3.
- When `--out` is given, **nothing** is written to stdout. One informational line naming the written path and the format is written to stderr.
- The bytes written to the file are exactly the bytes that would have gone to stdout.

### 9.5 `--skip-assets`

`--skip-assets` skips exactly these checks and nothing else:

- for `file` values: the on-disk existence check (E421);
- for `image` values: the on-disk existence check (E421), the header probe (E422) and the dimension check (E423).

It does **not** skip: the extension check (E420), the path-confinement check (E424), or any type, range, enum, reference, required-field or logic check. It has no effect on `exists`, which never touches the filesystem in any mode (§6.7).

**The compiled bytes are identical with and without `--skip-assets`.** The flag changes which errors are reported, never what is emitted. An implementation that lets the flag alter output is non-conforming.

### 9.6 Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | compilation failed: at least one `E1xx`–`E7xx` diagnostic was reported |
| 2 | usage error: an `E8xx` diagnostic other than E810 |
| 3 | output could not be written (E810) |

### 9.7 stdout and stderr

- stdout carries **only** the rendered document (`compile` without `--out`), the schema list (`templates`), or the usage/version text. It is never mixed with diagnostics.
- stderr carries every diagnostic, every note and every informational line.
- If stdout is closed by the reader (a broken pipe, as with `| head`), the process MUST terminate quietly with exit code 0 and MUST NOT report an error.
- Output is written as UTF-8 with `LF` line endings on every platform, with no BOM.

### 9.8 Diagnostic format

```text
<path>:<line>:<col>: error[<ID>]: <message>
  note: <note text>
```

- `<path>` is the source path relative to the project root, using `/` separators. Absolute, canonical and extended-length platform paths MUST NOT appear — including when the invocation itself used the extended-length spelling, whose `\\?\` prefix is stripped from the argument before the project is resolved (§9.1).
- `<line>` and `<col>` are 1-based; `<col>` counts Unicode scalar values.
- When a diagnostic has no meaningful column, the form `<path>:<line>: error[<ID>]: …` is used; when it has no position, `<path>: error[<ID>]: …`; when it concerns no file, `abstract: error[<ID>]: …`.
- `<message>` is one sentence ending with a period.
- Zero or more `note:` lines follow, indented by two spaces, each naming a second position (`<path>:<line>:<col>: note: …`) when it refers to one — for example the first declaration site of a duplicate.

**Substitutions.** `{context}` is the dotted path of the object that contains the field, rooted at the instance id: the root object is the bare id (`atlas`), a group or `$(Schema)` value is `atlas.owner`, and an element of a list is `atlas.copy[2]`, with a 0-based index. `{field}` is the normalised field name. `{kind}` is one of `text`, `int`, `float`, `bool`, `enum`, `file`, `image`, `ref`, `object`, `list`, `absent`. A list substitution (`{members}`, `{list}`, `{columns}`) renders its items in declaration order, each without quotes, separated by `, `. `{versions}` renders a set of versions as ascending maximal ranges, `1..2` for a range and `3` for a single version, separated by `, `. `{v}` is a single version, as a decimal integer.

`{value}` renders a scalar exactly as §8.7 and §8.8 would render it in JSON, with one exception that E413 fixes explicitly. A `text` range constrains the value's **length**, not the value, so in E413 against a `text` range — and in E413 against an instance id, whose subject is the `id` declaration of §4.11 — `{value}` is the **number of Unicode scalar values in the value**, written as a decimal integer, unquoted; the text itself does not appear in the message at all. The count is the one §4.4.1 constrains and the one `<col>` above uses: a combining mark counts as one and an astral character counts as one, and neither bytes nor UTF-16 code units are ever counted. Against an `int` or a `float` range `{value}` is the number itself, rendered by §8.7. A five-character value against `text(1..3)` is therefore `Range mismatch at atlas.tier: 5 is not in 1..3.`, and an eight-character id against `id: text(1..4)` is `Range mismatch at abcdefgh.id: 8 is not in 1..4.`

`{ranges}` renders a range list (§4.5) part by part in declaration order, separated by `, `. An interval renders as `{min}..{max}`. An **integer part whose two bounds are equal renders as the bare number**, because `2..2` and the exact value `2` are the same constraint and must not read as two different ones: `text(1..3, 8, 12..14)` renders as `1..3, 8, 12..14`, and both `text(2..2)` and `text(2)` render as `2`. A float part always renders both bounds, in the spelling of §8.7, so `float(1.0..1.0)` renders as `1.0..1.0`.

**Suggestions.** A diagnostic that offers `Did you mean '{suggestion}'?` computes it as follows. The algorithm is normative because suggestions appear in the byte-exact `err.txt` of §11.2.

1. The candidate set is the one named by the identifier's entry in §10: declared field names of the schema or group in scope (E409, E505), declared schema names (E309, E401, E501), declared enum members (E414), or the instance ids declared with the relevant template (E405, E431).
2. The distance is the Levenshtein distance between the normalised offending text and the normalised candidate, counting Unicode scalar values.
3. A candidate is discarded when its distance exceeds 2, or exceeds half the length of the offending text.
4. Among the survivors the smallest distance wins. Ties are broken by declaration order where the candidate set has one (fields, enum members), and otherwise by comparing candidates as sequences of Unicode scalar values, smallest first.
5. When no candidate survives, the `Did you mean` clause is omitted entirely; the rest of the message is unchanged.

### 9.9 Optional tooling

`bundle` and `unbundle` (sealed `.abx` containers), the Java runtime and the editor extension are optional tooling shipped with the reference implementation. They operate on an already-compiled document and MUST NOT change compilation semantics. A conforming Abstract implementation is not required to provide them, and this specification does not define them. Appendix D lists what the reference implementation MUST satisfy where it does ship them.

---

## 10. Diagnostics catalogue

Every identifier is stable: within 1.x an identifier is never reused for a different condition. Identifiers not listed here are reserved. In the message templates, `{…}` marks a substitution.

### 10.1 E1xx — files and projects

| ID | Condition | Message template | Example |
|---|---|---|---|
| E101 | A discovered source file cannot be read | `Cannot read source file '{path}': {reason}.` | file deleted between discovery and read |
| E102 | A source file is not valid UTF-8 | `Source file '{path}' is not valid UTF-8 (first bad byte at offset {offset}).` | a Latin-1 encoded `.ab` |
| E103 | Discovery collected no source files | `No .ab or .abt files were found under '{path}'.` | `abstract compile ./empty` |

### 10.2 E2xx — lexical

| ID | Condition | Message template | Example |
|---|---|---|---|
| E201 | A quoted string is not closed on its line | `Unterminated string literal; strings must open and close on the same line.` | `note: "hello` |
| E202 | Unknown escape after `\` in a string | `Unknown escape '\\{char}' in a string literal; valid escapes are \\" \\\\ \\n \\r \\t.` | `path: "C:\pack"` |
| E203 | End of file with brackets still open | `Unclosed '{bracket}' opened at {line}:{col}.` | `tags: [core` |
| E204 | A closing bracket does not match the open one | `Mismatched bracket: expected '{expected}' to close '{open}' opened at {line}:{col}, found '{found}'.` | `tags: [core)` |
| E205 | A closing bracket with nothing open | `Unexpected '{bracket}'.` | `tags: core]` |
| E206 | A character that cannot appear in an identifier | `Invalid character '{char}' in identifier '{text}'; identifiers use A-Z a-z 0-9 _ -.` | `TAMAÑO: 3` |
| E207 | An identifier ends with `-` | `Identifier '{text}' must not end with '-'.` | `size-: 3` |
| E208 | A malformed schema name | `Invalid schema name '{text}'; schema names start with a letter and contain letters, digits and '_'.` | `schema My Thing {` |
| E209 | A limit from §3.7 is exceeded, except the version-count bound, which is E602 | `{subject} exceeds the limit of {limit}.` | 3000 nested groups |
| E210 | A token or construct that is not valid here | `Unexpected {found} here; expected {expected}.` | `schema` inside a `.ab` file; `-name: 1`, whose `-` cannot begin an identifier (§3.3); `label://x`, an unquoted value beginning with `//` (§3.2); a control character in a value (§3.5), spelled `control character U+XXXX` |
| E211 | A numeric literal is malformed or out of range | `'{text}' is not a valid {kind} literal.` | `count: 99999999999999999999` |

### 10.3 E3xx — schemas

| ID | Condition | Message template | Example |
|---|---|---|---|
| E301 | Duplicate schema name | `Schema '{name}' is already declared.` + note at the first site | two `schema Item` |
| E302 | Duplicate field name in one block | `Field '{name}' is already declared in this block.` + note + `note: annotations do not make two declarations of one name distinct.` | `Name:` and `name:` |
| E303 | Unknown or repeated field modifier | `Unknown field modifier '{text}'.` / `Modifier '{text}' is repeated.` | `@optionnal` |
| E304 | Unknown type, or whitespace before `(` | `Unknown type '{text}'; expected text, int, float, bool, enum, file, image, ref or $(Schema).` | `text (1..40)`; `caps[]: { … }` |
| E305 | Malformed, open-ended, non-finite or reversed range | `Invalid range '{text}'; ranges are 'value' or 'min..max' with min <= max.` | `int(10..5)` |
| E306 | Empty enum, or duplicate member | `enum(...) must declare at least one member.` / `Duplicate enum member '{name}'.` | `enum(red, red)` |
| E307 | Unsupported image format | `Unsupported image format '{ext}'; supported: png, jpg, gif, bmp, webp.` | `image(tga)` |
| E308 | Malformed `WIDTHxHEIGHT` | `Invalid image size '{text}'; expected WIDTHxHEIGHT where each side is a number or '*'.` | `image(png 128)` |
| E309 | Unknown schema in `$(…)` or `ref(…)` | `Unknown schema '{name}' referenced here.` (+ `Did you mean '{suggestion}'?`) | `$(prodcut)` |
| E310 | Second `@tag` in one group | `Group '{name}' already has a @tag field '{first}'.` + note | two `@tag` |
| E311 | `@tag` on a root-level field | `@tag is only allowed on a field inside a group; '{name}' is a root field of schema '{schema}'.` | `status: enum(a,b) @tag` at root |
| E312 | Default on a group field | `A group field cannot declare a default.` | `owner { … } = x` |
| E313 | Default invalid for its own type | `Default value {value} is invalid for {schema}.{field}: {reason}.` | `n: int(1..10) = 999` |
| E314 | Field named `template`, or an `id` declared other than as `id: text` / `id: text(a..b)` | `'template' is a reserved envelope key and cannot be declared as a field.` / `'id' may only be declared as 'id: text' or 'id: text(min..max)' with no modifiers and no default; found {found}.` | `template: text`; `id: int`; `id: text @optional` |
| E315 | Doubled list marker | `'{name}[][]' is not valid; a field is a list or it is not.` | `tags[][]: text` |
| E316 | Invalid field name or empty path segment; reported at the first token of the left side, naming the whole of it. A left side whose **first** token cannot begin a path is E210 at that token instead | `Invalid field name '{text}'.` | `a..b: 1`; `note #comment: hello`. `(a, a): (1, 2)` is E210 |
| E317 | A parameterised type with no arguments | `{type}(...) requires at least one argument.` | `file()` |
| E318 | Unsatisfiable constraint | `Constraint '{text}' on {schema}.{field} can never be satisfied.` | `text(-5..-1)` |
| E319 | Duplicate extension | `Duplicate extension '{ext}' in {type}(...).` | `file(png, png)` |
| E320 | `@optional` together with a default | `{schema}.{field} declares both @optional and a default; a default already makes the field present.` | `n: int @optional = 1` |
| E321 | Recursive schema with no terminating case | `Schema '{a}' requires '{b}' which requires '{a}'; the structure can never be built.` | `schema A { child: $(A) }` |
| E322 | `@tag` on an unsupported field | `@tag requires a non-list field of type text, int, bool or enum; '{name}' is {kind}.` | `icon: image(png) @tag` |
| E323 | Malformed list cardinality | `Invalid list cardinality '{text}'; write '[]', '[min..]' or '[min..max]' with 0 <= min <= max.` | `tags[4..2]` |

### 10.4 E4xx — instances

| ID | Condition | Message template | Example |
|---|---|---|---|
| E401 | Unknown template in a header | `Unknown template '{name}'.` (+ `Did you mean '{suggestion}'?`) | `Prodcut :: @id.a` |
| E402 | Duplicate instance id | `Duplicate instance id '{id}'.` + note at the first site | two `@id.atlas` |
| E403 | Statement before the first header | `A statement cannot appear before the first instance header.` | assignment on line 1 |
| E404 | Clone statement not directly after the header | `A clone must appear immediately after the instance header, before any assignment.` | `&base.*` after a body line |
| E405 | Unknown clone target | `Unknown clone target '{id}'.` (+ suggestion) | `&atals.*` |
| E406 | Clone cycle | `Clone cycle detected: {id} -> {id} -> {id}.` | mutual clones |
| E407 | Clone across templates | `Instance '{id}' is a '{other}'; a clone source must use the same template '{template}'.` | cloning a `Pack` from a `Sticker` |
| E408 | Clone path absent on the source in every version in which the cloning instance exists | `Clone path '{path}' does not exist on instance '{id}' in any version.` + `note: versions checked: {versions}.` | `&hero.stats` where `stats` is never assigned |
| E409 | Unknown field | `Unknown field '{name}' in {context}.` (+ `Did you mean '{suggestion}'?`) + `note: declared fields: {list}.` | `rarty: 3` |
| E410 | Assignment to `template` or `id` | `'{name}' is a reserved envelope key and cannot be assigned.` | `id: other` |
| E411 | Required field absent after logic | `Missing required field {context}.{field}.` + `note: this field exists in versions {versions} and has no value in version {v}.` | omitted `name` |
| E412 | Value shape does not match the type | `Type mismatch at {context}.{field}: expected {type}, found {found}.` | `count: "5"` |
| E413 | Value outside declared ranges, including an instance id outside the `id` range (§4.11) | `Range mismatch at {context}.{field}: {value} is not in {ranges}.` — against a `text` range and against an id, `{value}` is the value's scalar count, not its text (§9.8) | `replicas: 99`; a 70-character id against the implicit `id: text(1..64)`, which reads `70 is not in 1..64` |
| E414 | Value not an enum member | `Enum mismatch at {context}.{field}: '{value}' is not one of {members}.` (+ suggestion) | `status: activ` |
| E415 | Wildcard matched nothing | `Wildcard '{prefix}*' matches no member of {members}.` | `flags: cor_*` |
| E416 | Tuple row arity mismatch | `Tuple row {n} has {found} values but {field} declares {expected} columns ({columns}).` | `(a, b, c)` for 2 columns |
| E417 | Duplicate tuple column | `Duplicate tuple column '{name}'.` | `copy(key, key)` |
| E418 | `#tag` where no `@tag` exists | `{context} has no @tag field, so '#{name}' shorthand cannot be used here.` | `caps: [#x]` |
| E419 | Malformed `#tag` argument list | `Invalid argument '{text}' in '#{name}(...)'; arguments are 'key: value' or a bare boolean flag.` | `#a(: 1)` |
| E420 | Extension not declared | `Extension '{ext}' is not allowed at {context}.{field}; allowed: {list}.` | `icon: a.tga` |
| E421 | Asset missing on disk | `File not found for {context}.{field}: '{path}' (resolved to 'assets/{rest}').` | missing texture |
| E422 | Image content does not match the extension | `Image content mismatch at {context}.{field}: '{path}' is {actual}, not {expected}.` (+ `note: file is truncated.`) | GIF renamed `.png` |
| E423 | Image dimensions not allowed | `Image size mismatch at {context}.{field}: '{path}' is {w}x{h}, expected one of {list}.` | 64x64 for `png 128x128` |
| E424 | Asset path escapes the assets directory | `Asset path '{path}' must be relative to assets/ and must not be absolute or contain '..'.` | `../../secret.png` |
| E425 | Unknown interpolation variable | `Unknown variable '${name}'.` (+ suggestion) + `note: variables are the root scalar fields of this instance.` | `$slugg` |
| E426 | Malformed `$` | `'$' must be followed by a name, '{' or '$'.` | `price: 5 $ each` |
| E427 | Unterminated `${` | `Unterminated '${'.` | `label: ${id` |
| E428 | Invalid or empty instance id | `Invalid instance id '{text}'; ids are identifiers (A-Z a-z 0-9 _ -).` | `@id.` |
| E429 | Two statements write the same path | `{path} is assigned twice for version(s) {versions}.` + note at the first site | `name:` twice |
| E430 | Statement applies to no version | `This statement can never apply: {field} exists in versions {a}, the statement is annotated {b}.` | `legacy: 1 @since(3)` |
| E431 | `ref` target does not exist, or does not exist in this version | `Unknown reference '{id}' at {context}.{field}.` (+ suggestion) / `Reference '{id}' at {context}.{field} does not exist in version {v}.` + `note: instance '{id}' exists in versions {versions}.` | `pack: no_such`; a `ref` to an instance declared `@since(2)`, compiled for version 1 |
| E432 | `ref` target has the wrong schema | `Reference '{id}' at {context}.{field} is a '{actual}', expected a '{expected}'.` | pointing at a `Sticker` |
| E433 | Empty multi-path key list | `A multi-path must name at least one key.` | `a.{}: 1` |
| E434 | Brace pattern on a non-list asset field | `Brace patterns expand into several paths and require a list field; {context}.{field} is not a list.` | `icon: a{1,2}.png` |
| E435 | Malformed brace pattern: an empty alternative, a nested group, or more than 8 groups. An **unbalanced** brace is E203 / E205 at the lexical level and is never E435 (§3.6, §5.8) | `Invalid brace pattern '{text}': {reason}.` | `a{b,}.png`; `a{b{c,d},e}.png`; a pattern with nine groups |
| E436 | Second `::` in a header | `An instance header contains exactly one '::'.` | `A :: @id.x :: y` |
| E437 | Annotation on a header tag, a clone or a body block | `@since/@removed are only allowed on a body statement or at the end of an instance header.` | `&base.* @since(2)` |
| E438 | Header tag on a nested field | `Header tag '@{name}' targets {kind} '{field}'; a header tag assigns one root scalar field. Write a body statement.` | `@owner.team` |
| E439 | `::` outside an instance header | `'::' starts an instance header, but '{text}' is not a schema name.` | `my thing :: @id.x` |
| E440 | An assignment — a body statement or a header tag — or a clone falls outside the instance's version window | `This statement applies to versions {a}, but instance '{id}' exists only in versions {b}.` / `Clone source '{source}' exists in versions {a}, but instance '{id}' exists in versions {b}.` + note at the instance header | `glow: true @since(3)` in an instance declared `@removed(3)`; the header tag `@glow` on the same instance |
| E441 | Nested or empty list item | `Nested lists are not supported.` / `Empty list item.` | `[[a]]`, `a,,b` |
| E442 | Trailing comma at depth 0 | `Trailing comma; a statement does not continue onto the next line.` | `tags: a,` |
| E443 | Path crosses a non-object or a list | `Cannot assign '{path}': '{prefix}' already holds {kind}.` / `Cannot assign '{path}': '{prefix}' is a list.` (+ `note: '{field}' is a list; write its elements with a tuple array or with '#tag' shorthand.`) | `owner: x` then `owner.team: y` |
| E444 | Duplicate key in a keyed list | `{context}.{field} already has an element whose {tag} is '{key}'.` + note at the first element | `capabilities: [#search, #search]` |
| E445 | List cardinality violated | `{context}.{field} has {found} elements; {field} declares {expected}.` | `tags: []` on `tags[1..]` |

**E422's `{actual}`.** When the file's bytes name a different format, `{actual}` is that format, or `not an image ({first bytes})` when they name none. When the **signature matched** the declared extension and the header is nevertheless unusable — truncated, a malformed BMP width, a malformed canvas size, no start-of-frame marker (§4.4.7.1) — `{actual}` is the word `unreadable`, and the prescribed note names the check that failed:

```text
error[E422]: Image content mismatch at frost.icon: 'a.bmp' is unreadable, not bmp.
  note: malformed BMP width.
```

`{actual}` is never the declared format: a message reading `is bmp, not bmp` denies its own condition, and `{expected}` already names what was declared.

### 10.5 E5xx — logic

| ID | Condition | Message template | Example |
|---|---|---|---|
| E501 | Logic block for an unknown schema | `Unknown schema '{name}' in logic block.` (+ suggestion) | `logic Prodcut {` |
| E502 | Second logic block for one schema | `Schema '{name}' already has a logic block.` + note | two `logic Item` |
| E503 | Path names an undeclared field | `Unknown path '{path}' in logic for '{schema}'.` (+ suggestion) | `.raritty` |
| E504 | `derive` targets `template`/`id` | `derive cannot write '{name}'.` | `derive .id = x` |
| E505 | `derive` target is not a declared field, or is loop-relative | `derive target '{path}' is not a declared field of '{schema}'.` | `derive .new = 1` |
| E506 | `derive` traverses or indexes a list | `derive cannot write through a list; '{path}' crosses list field '{field}'.` | `derive .items[0].x = 1` |
| E507 | `require` without `else throw` | `require must be followed by 'else throw "message"'.` | bare `require .x exists` |
| E508 | Throw message is not a quoted string | `A throw message must be a quoted string.` | `else throw 42` |
| E509 | `for` over a non-list | `'{path}' is not a list; for iterates lists and literal lists.` | `for $x in .name` |
| E510 | Unbound loop variable | `Unbound variable '${name}'.` | `$slot` outside its loop |
| E511 | Malformed condition: a token that cannot begin an operand, or a token that is not an operator where one is due (§6.6) | `Invalid condition '{text}': {reason}.` | `.a === "b"`, `(.a) xor (.b)` |
| E512 | Chained comparison | `Comparisons cannot be chained; use 'and'.` | `.a < .b < .c` |
| E513 | Operand type not allowed | `Operator '{op}' cannot compare {left} and {right}.` | `.tags == .other` |
| E514 | Invalid `length()` argument | `length() requires a list or text value; '{path}' is {kind}.` | `length(.count)` |
| E515 | A `require` failed | `{schema} logic: {message}` + a note at the instance header + `note: compiling version {v}.` | the author's own message |
| E516 | Loop variable shadowing | `Loop variable '${name}' is already bound by an enclosing loop.` | nested `for $i` |
| E517 | Misplaced `else` | `'else' must follow the closing '}' of an if block.` | stray `else {` |
| E518 | `derive?` on a defaulted field | `derive? can never apply to '{field}': the schema declares a default, which is filled before logic runs.` | `derive? .wave = 3` |
| E519 | An operand can project several values | `Operator '{op}' requires a single value; '{path}' crosses list field '{field}'. Use 'contains'.` | `.caps.id != "x"` |
| E520 | Loop variable in a `derive` target | `A derive target cannot contain the variable '${name}'; write the field name.` | `derive .slots.$slot.mode = x` |
| E521 | A `derive` executed against a field absent in this version | `derive cannot write {context}.{field} in version {v}; the field exists in versions {versions}.` + a note at the instance header + `note: guard the statement, for example with 'if version >= {n}'.` | `derive .glow = true` in version 1, where `glow` is `@since(2)` |
| E522 | An interpolated `$name` in a `derive` value or a `throw` message is bound to a value that is not a scalar | `Variable '${name}' is not a scalar; it is {kind}.` + `note: interpolation inserts text; read a scalar field of it instead.` | `derive .label = cap-$c` inside `for $c in .caps`, where `caps` is a list of groups |
| E523 | The logic of one instance executed more loop iterations for one version than §3.7 allows | `Logic work limit exceeded at {context}: the logic executed more than 1000000 loop iterations in version {v}.` + a note at the instance header + one `note: at index {i}, item {item}.` per `for` enclosing the one the position names, outermost first, and so at most one fewer than the block-nesting limit of §3.7 | seven `for` blocks nested over a literal list of ten |

### 10.6 E6xx — versions

| ID | Condition | Message template | Example |
|---|---|---|---|
| E601 | More than one `versions` declaration | `The project already declares a version range.` + note | two `versions` lines |
| E602 | Invalid version range: malformed or reversed, or covering more than 4096 versions (§3.7, §4.12) | `Invalid version range '{min}..{max}'; both are integers >= 1 and min <= max.` / `Invalid version range '{min}..{max}'; a project range covers at most 4096 versions.` | `versions 0..3`, `versions 3..1`; `versions 1..2000000000` |
| E603 | Annotation outside the project range | `Version {n} is outside the project range {min}..{max}.` | `@since(9)` in `1..3` |
| E604 | `@removed` not after `@since` | `@removed({r}) must be greater than @since({s}).` | `@since(3) @removed(2)` |
| E605 | Empty existence set | `{schema}.{field} exists in no version: {reason}.` | child outlives its group |

### 10.7 E7xx — output and public-contract export

| ID | Condition | Message template | Example |
|---|---|---|---|
| E701 | A value cannot be rendered | `Value at {context}.{field} cannot be represented in {format}.` | a non-finite float (unreachable) |
| E702 | A nominated field cannot be admitted by the requested public-contract profile or its dependency independence cannot be established | `Public contract profile cannot admit {target}: {reason}.` | a nominated field observed by an unsupported `require` dependency |
| E703 | Public-contract export exceeds a stated budget or receives inconsistent validated compilation inputs | `Public contract export rejected: {reason}.` | target/version expansion exceeds the export budget |

E702 and E703 belong to explicit public-contract export. They do not change
ordinary compile admission merely because `@public` is present. Export failure
returns no successful partial contract; the command exits with status 1.

### 10.8 E8xx — command line

| ID | Condition | Message template | Example |
|---|---|---|---|
| E801 | Unknown command | `Unknown command '{text}'; expected compile, lint, templates or init.` | `abstract frobnicate` |
| E802 | Unknown flag | `Unknown flag '{text}'.` | `--skip-asset` |
| E803 | Flag without a value | `Flag '{flag}' requires a value.` | `--out` at the end |
| E804 | Unexpected positional | `Unexpected argument '{text}'.` | `templates a b` |
| E805 | Unknown format keyword | `Unknown output format '{text}'; expected JSON, YML, YAML or RAW.` | `compile pack XML` |
| E806 | Input path missing, wrong kind, absent, or a directory that is neither a project root nor a data directory | `Input not found: '{path}'.` / `'{path}' is not an .ab or .abt file.` / `No input path was given.` + `note: available .ab files here: {list}.` / `'{path}' is inside the data directory '{data}'.` + `note: name the project root or the 'data' directory.` / `'{path}' contains more than one 'data' directory.` | typo in a filename; `compile pack/data/items` |
| E807 | Incompatible inputs | `Cannot mix files and directories in one invocation.` / `Inputs belong to different projects: '{a}' and '{b}'.` | `compile data data/x.ab` |
| E808 | `--out` destination refused | `Refusing to write '{path}': it is a source file.` / `Refusing to write '{path}': it is inside the data directory.` / `Refusing to write '{path}': an '.ab' or '.abt' file inside the discovery root would be read on the next run.` | `--out data/out.json` |
| E810 | Output write failed | `Cannot write '{path}': {reason}.` | read-only destination |
| E811 | Flag repeated | `Flag '{flag}' was given twice.` | `--out a --out b` |
| E812 | Invalid flag value, including a `--max-errors` outside 1..10000 (§9.3) | `Flag '{flag}' requires {expected}; found '{text}'.` | `--max-errors x`, `--max-errors 0`, `--max-errors 10001` |
| E813 | `init` target not empty | `'{path}' is not empty.` | `init .` in a project |

---

## 11. Conformance

### 11.1 Requirements

An implementation is conforming when, for every input:

1. it accepts exactly the programs this document declares valid;
2. it reports the identifier this document prescribes for every invalid program, at the position this document prescribes;
3. it emits byte-identical output to the forms in §8;
4. it obeys the determinism guarantees of §7.6;
5. it never aborts, panics or crashes, and never exceeds the limits of §3.7;
6. it passes the golden suite (§11.2).

An implementation MAY report more than one diagnostic per run, up to the limit of `--max-errors` (§9.3). The diagnostics it reports MUST be the first ones in the order below, and when it reports only one, that one MUST be the first.

**Precedence.** When one input satisfies more than one error condition, the diagnostic reported is chosen by, in order: (1) the earliest phase of §7.1; (2) within a phase, the earliest step of §7.3; (3) within a step, the earliest source position; (4) at the same position, the lower numeric identifier. These pairs are fixed explicitly, because their two conditions can arise in the same step: E443 before E412; E211 before E412 and E413; E603 and E604 before E430 and E440; E430 before E440; E320 before E313; E807 before E804; E415 before E414; E445 before E411.

### 11.2 Golden-test format

A golden case is a directory:

```text
<case-name>/
  cmd                 one line: the arguments that follow `abstract`
  in/                 the project tree, exactly as authored
  exit                one line: the expected exit code, in decimal
  out.json            expected stdout, byte for byte  (when exit is 0)
  out.yml             (alternative to out.json)
  out.abraw           (alternative to out.json)
  err.txt             expected stderr, byte for byte  (when exit is not 0)
```

Rules:

- `cmd` contains the arguments only. Paths in it are relative to `in/`. Example: `compile . JSON`.
- The runner executes the compiler with `in/` as the working directory.
- At most one `out.*` file may be present. It MUST be present exactly when the case expects bytes on stdout, so a passing `compile` case carries one, and a passing `lint` case, a `compile --out` case and every failing case carry none.
- **Success with no stdout is a complete expectation.** A case whose `exit` is `0` and which carries no `out.*` file expects exit code 0 and an empty stdout, and the runner MUST check both. That is the normal shape of a passing `lint` case, whose `err.txt` is the single line `abstract: ok`.
- `err.txt` MUST be present exactly when the case expects bytes on stderr: every case whose `exit` is not `0`, every passing `lint` (which prints `abstract: ok`), and every `--out` case (which prints the informational line of §9.4).
- Comparison is byte for byte after two normalisations, applied to the expected and the actual stream alike: `CRLF` becomes `LF`, and the string value of `abstract.compiler` (§8.1) becomes `0.0.0`. No other normalisation is permitted: whitespace, key order and number spelling are all part of the contract.
- **Parser recovery is not part of the golden contract.** What §11.1 fixes for a failing case is the first diagnostic and every further *independent* one, in order; how many diagnostics a compiler recovers far enough to add after them is its own choice. A case whose `err.txt` names one diagnostic therefore constrains the first line and the exit code, and a suite MAY additionally compare identifiers as a prefix rather than comparing the whole stream byte for byte. A case that does pin the whole stream pins one implementation's recovery with it.
- Paths inside `err.txt` are project-relative with `/` separators (§9.8), which makes cases platform-independent.
- Informational stderr lines (for example the `--out` line) are part of `err.txt` when the case expects them; a case with `exit` `0` that produces stderr MUST also carry `err.txt`.
- A case directory MUST NOT contain anything else. Binary assets used by `image`/`file` cases live under `in/assets/`.

### 11.3 Required coverage

A conforming suite MUST contain at least one case for:

- every error identifier in §10 that is reachable (E701 excepted);
- every type in §4.4, valid and invalid;
- every construct in §5, including clones, keyed-list merge, wildcards, tuple arrays, brace patterns, interpolation, version annotations on statements and version windows on instance headers;
- both spellings of an explicit `id` declaration (`id: text`, `id: text(a..b)`), an id that violates the declared range, an id that violates the implicit `text(1..64)`, and a rejected declaration such as `id: int` (§4.11);
- every logic statement and every operator in §6, including the `version` built-in in a condition and in a `derive`;
- all three output formats for one project with nested groups, lists of groups, floats, booleans, unicode text and an empty list;
- a multi-version project exercising `@since` and `@removed` on fields, on statements and on instance headers; per-instance overlay grouping; two overlays whose ranges overlap; a non-empty `removed` list; an instance that exists only outside the base version; and an empty overlay list;
- determinism: the same project compiled from a directory root and from its data directory, and with the file arguments in two different orders, producing identical bytes;
- every command of §9.2 and every reachable `E8xx` identifier, exercised through the binary: `--out`, `--out=`, `--skip-assets`, `--max-errors`, a repeated flag, an unknown flag, an unknown format keyword, a mixed file-and-directory invocation, a destination the compiler must refuse, and a run piped into a reader that closes stdout;
- validity: the runner MUST parse `out.json` as JSON and `out.yml` as YAML in addition to comparing bytes, so that a syntactically invalid document cannot pass;
- cross-format equivalence: for one project the JSON and YAML cases MUST carry the same data, verified by the runner after coercing every mapping key to a string;
- negative logic: at least one case for each of E501–E523, and one determinism case proving that two runs of a project whose logic fails report the same first failing `require`;
- the work limit of §3.7 from both sides: a project whose logic crosses it (E523) and one that stays under it and compiles.
---

## Appendix A — Migration from 0.2.0

Every difference that can change the meaning of an existing project, or reject a project that used to compile, is listed here. Differences that only add new constructs are listed last.

### A.1 Source language

| # | Change | 0.2.0 | 1.0 |
|---|---|---|---|
| A1 | The `data:` sentinel is gone | a statement starting with `data:` silently stopped parsing the rest of the file | `data` is an ordinary field name; every statement is parsed |
| A2 | Trailing-comma continuation removed | a statement ending in `,` swallowed the next line | E442; wrap lists in `[ … ]` to span lines |
| A3 | Clone position | `&id.*` was written **before** the header | `&id.*` is written **after** the header, before the first assignment (E404) |
| A4 | Cross-template clones | allowed, and smuggled foreign fields in | E407 |
| A5 | Keyed lists in clones | a clone replaced a tagged list wholesale | tagged lists merge by tag value (§5.7) |
| A6 | `lang` → `lang_values` rename | a field named `lang` was emitted as `lang_values` | removed; fields are emitted under their declared names |
| A7 | `template` / `id` as schema fields | allowed, and silently overwrote the envelope | `template` is always E314. An `id` declaration MAY be kept when it is `id: text` or `id: text(a..b)`: the range is honoured against the instance id (§4.11, E413). Delete it and the implicit `id: text(1..64)` applies. Any other declaration of `id` — another type, a modifier, a default, a list head — is E314 |
| A8 | `template:` / `id:` assignments | allowed, and forged the envelope | E410 |
| A9 | `@tag` at root | consumed the instance id and deleted `id` from output | E311 |
| A10 | Several `@tag` per group | the extra ones silently became ordinary fields | E310 |
| A11 | Quoted numbers for `int`/`float`/`bool` | silently ignored, then a confusing error | E412 — quoting never coerces |
| A12 | Absent optional list fields | emitted `[]` | omitted, like every other absent optional field |
| A13 | Duplicate assignment to one path | last writer won, silently | E429 |
| A14 | Nested lists | flattened into a string | E441 |
| A15 | Enum wildcard matching nothing | expanded to nothing, field became `[]` | E415 |
| A16 | `#prefix*` with a non-`id` tag field | failed with an enum mismatch | expands correctly (§5.6) |
| A17 | Unknown escapes in strings | passed through (`"C:\pack"` kept `\p`) | E202 — write `/` or `\\` |
| A18 | Unknown `$variable` | left as literal text | E425 |
| A19 | `$name` boundary | substring replacement; `$named` became `<name>d` | maximal identifier run; `${name}` for explicit boundaries |
| A20 | Multi-path prefix | only one segment (`a.b.{c,d}` built a field literally named `a.b`) | the prefix may be dotted |
| A21 | Text after `#tag(...)` or between tuple rows | silently discarded | E210 |
| A22 | Tuple rows without separating commas | accepted | E210 |
| A23 | Header detection | any statement containing `::` could start an instance | only a statement whose first token is a schema name followed by `::` |
| A24 | Asset paths | `..`, absolute and UNC paths escaped the project | E424; one resolution rule, relative to `assets/` |
| A25 | `./assets/x` | resolved to `<project>/assets/x` | resolves to `<project>/assets/assets/x`; drop the `assets/` prefix |
| A26 | UTF-8 BOM | silently deleted the first schema, or broke the first header | stripped and ignored |
| A27 | `.AB` / `.ABT` files | invisible | recognised (extensions are case-insensitive) |
| A28 | A directory named `Data` | did not trigger the project-root rule | the `data` marker is case-insensitive |
| A29 | Instance ids | case-sensitive and unnormalised | normalised like every other identifier; `Atlas` and `atlas` now collide (E402) |
| A30 | `@id.42` | became an integer and defeated duplicate detection | ids are identifiers, never type-inferred (E428) |
| A31 | Reversed ranges, bad defaults | accepted at schema time, failed later at an instance | E305 / E313 at schema time |
| A32 | Non-ASCII identifiers | half-normalised, silently distinct | E206 |
| A33 | Empty project | compiled to `{"data": []}` | E103 |
| A66 | Bare tuple multi-assignment `(a, b): (1, 2)` | undocumented, accepted, and duplicate columns silently overwrote | removed; a tuple array requires a field path before `(` (§5.5) |
| A67 | Header tag values | type-inferred without the schema, so `@count.12` was always an integer | type-directed like every other value (§5.2) |
| A68 | Keyed lists | duplicate tag values were accepted inside one list | E444 |
| A69 | Values containing `,` | a comma was sometimes text, sometimes a separator | always a list separator at depth 0; quote the value (§5.5) |
| A70 | Values containing `//` | a spaced `//` truncated the value as a comment, silently | still a comment; the value must be quoted (§3.2) |

### A.2 Logic

| # | Change | 0.2.0 | 1.0 |
|---|---|---|---|
| A34 | Unknown paths in conditions | silently false, so guards stopped guarding | E503 |
| A35 | `==` on text | case- and `-`/`_`-insensitive | exact comparison (§6.8) |
| A36 | Numeric comparison of strings | `"10" > 9` compared numerically | E513 |
| A37 | Truthiness | any value could be a condition | a condition must be boolean (E513) |
| A38 | `length()` on a scalar | returned 1 | E514 for numbers/booleans; text yields its scalar count |
| A39 | Bare `length(...)` as a condition | meant `> 0` | E513 |
| A40 | `exists` on a `text` field | probed the filesystem when the value looked like a path | never touches the filesystem (§6.7) |
| A41 | Bare field names as operands (`status == "x"`) | compared the literal text `"status"` | E511/E503: paths start with `.` |
| A42 | `derive?` on a defaulted field | silently never fired | E518 |
| A43 | `derive` into an array | silently dropped, or crashed | E506 |
| A44 | Loop variables without `$` | allowed for exact-value derive only | loop variables are always `$name` |
| A45 | Loop variables in messages | only `$`-prefixed ones substituted | uniform (§6.9) |

### A.3 Output and CLI

| # | Change | 0.2.0 | 1.0 |
|---|---|---|---|
| A46 | Envelope | `{"data": [...]}` | `{"abstract", "data", "overlays"}`, where `abstract` is an object carrying `format`, `compiler` and `versions` (§8.1) |
| A47 | Instance order | lexicographic source-path order, undocumented | sorted by `(template, id)` (§2.7) |
| A48 | YAML keys | unquoted, so `1`, `yes`, `no` changed type | always quoted (§8.5) |
| A49 | YAML empty object in an array | rendered as `- ` (null) | `- {}` (§8.5); unreachable in a conforming document (§8.9) |
| A50 | RAW | arrays of objects on one line; top-level `{` unindented | fully specified (§8.6) |
| A51 | Floats | never used exponent notation, so `1e300` became a 300-digit literal | shortest round-trip, exponent outside `1e-6 … 1e21` (§8.7) |
| A52 | `--allow-unknown` | accepted undeclared fields | removed; there is no lenient mode |
| A53 | Direct form `abstract a.ab T.abt JSON` | supported, with different argument rules from `compile` | removed; use `abstract compile a.ab JSON` |
| A54 | Single-file compile | only loaded siblings of the file, so `data/templates/*.abt` was invisible | the whole project is discovered (§2.5) |
| A55 | Unknown flags / positionals / formats | silently ignored | E802 / E804 / E805 |
| A56 | `--out=file` | silently did nothing | supported (§9.3) |
| A57 | `--out` | overwrote any path, including sources, and still printed to stdout | refuses inputs (E808), suppresses stdout |
| A58 | Trailing `true` write flag | wrote a sibling file, sometimes inside the sources | removed; use `--out` |
| A59 | `--skip-assets` | did not skip absolute paths; changed `exists`, and therefore changed data | skips exactly the on-disk checks; output is unaffected (§9.5) |
| A60 | Piping into `head` | panicked with exit 101 | exits 0 quietly (§9.7) |
| A61 | `templates` | ignored duplicate-schema errors and printed a list anyway | runs P0–P3 and fails on any schema error |
| A62 | Hidden and vendor directories | walked, producing duplicate-id errors | `.`-prefixed, `node_modules`, `target`, `build`, `out` are skipped |
| A63 | Symlink cycles | walked forever | canonical-path visited set |
| A64 | Deep input | overflowed the stack and aborted the process | E209 |
| A65 | Diagnostic format | `abstract: path: message`, sometimes with `\\?\` prefixes | `path:line:col: error[Exxx]: message` (§9.8) |
| A71 | `templates` flags | `--skip-assets` / `--allow-unknown` were accepted and ignored | E802 |

### A.4 New in 1.0

- `versions min..max`, `@since(n)`, `@removed(n)`, and the `overlays` section (§4.12, §7.5).
- Instance version windows at the end of an instance header, and the `removed` list of an overlay (§5.14, §7.5).
- The built-in `version` in logic conditions and `derive` expressions (§6.6).
- The `ref(Schema)` type (§4.4.8).
- List cardinality in a field head: `tags[1..]`, `tags[2..4]` (§4.6).
- Body blocks in instances: `owner { … }` as sugar for a dotted prefix (§5.4).
- Dotted keys, nested `#tag` objects and bracketed lists as `#tag` arguments, and a bracketed row list for tuple arrays (§5.5).
- `--max-errors` (§9.3).
- Stable error identifiers and the diagnostics catalogue (§10).
- The golden-test conformance format (§11.2).

### A.5 Mechanical migration checklist

1. Delete every `template:` field declaration from `.abt` files. Keep an `id:` declaration only when it reads `id: text` or `id: text(a..b)`; rewrite any other one to that shape or delete it, and check that every id fits the range you keep (§4.11).
2. Move every `&clone` line to directly under its instance header.
3. Replace every trailing-comma line continuation with bracketed lists.
4. Quote nothing that feeds an `int`, `float` or `bool` field; unquote what is already quoted.
5. Replace `./assets/x` asset values with `x`.
6. Replace `\` in quoted paths with `/`.
7. Add a leading `.` to every bare path operand in logic, and lowercase every string compared with `==` against an enum.
8. Remove `--allow-unknown` from build scripts and fix the fields it was hiding.
9. Replace `abstract file.ab T.abt JSON true` with `abstract compile file.ab JSON --out <file>`.
10. Re-read consumers: the envelope gained the `abstract` object and `overlays`, and absent optional lists are no longer `[]`. A consumer that wants a version other than `max` applies **every** overlay whose range contains that version, replacing or adding whole objects by `id` from the overlay's `data` and deleting the ids in its `removed` (§7.5).

---

## Appendix B — Reserved words and identifiers

### B.1 Envelope keys

`template` and `id` are the envelope keys of every instance object and can never be assigned (E410). `template` can never be declared as a field either (E314). `id` may be declared at the root of a schema in exactly two spellings, `id: text` and `id: text(a..b)`, which constrain the instance id and nothing else; any other declaration of `id` is E314 (§4.11). Inside a group both names are ordinary field names, because a nested object carries no envelope.

### B.2 Document keys

`abstract`, `format`, `compiler`, `versions`, `data`, `overlays`, `removed`, `min` and `max` are keys of the compiled document (§8.1, §7.5). They are **not** reserved in the source language: a schema may declare a field named `data` or `format`, because instance objects are nested inside `data` and never collide with the envelope.

### B.3 Keywords

Keywords are recognised only in the position where they are meaningful; none of them is reserved as a field name, an enum member or an instance id.

| Position | Words |
|---|---|
| Top level of a `.abt` file | `schema`, `logic`, `versions` |
| Type position | `text`, `int`, `float`, `bool`, `enum`, `file`, `image`, `ref` |
| Modifier position | `@optional`, `@tag`, `@since`, `@removed` |
| Logic statement position | `derive`, `derive?`, `require`, `else`, `throw`, `if`, `for` |
| Logic expression position | `in`, `not`, `and`, `or`, `contains`, `exists`, `length`, `version`, `true`, `false` |

Operator spellings: `==` `!=` `<` `<=` `>` `>=` `&&` `||` `!`.

Punctuation with meaning: `:` `::` `.` `,` `=` `(` `)` `[` `]` `{` `}` `[]` `@` `#` `&` `$` `${` `$$` `*` `?` `..` `//`.

### B.4 Identifier rules, summarised

| Kind | Grammar | Normalised | Case-sensitive |
|---|---|---|---|
| Field name, enum member, tag name, path segment, tuple column, multi-path key | `identifier` | yes | no |
| Instance id, clone target, `ref` value | `identifier` | yes | no |
| Schema name | `schema_name` | no | **yes** |
| Loop variable | `$` + `identifier` | yes | no |
| File extension in `file`/`image` | letters and digits | canonicalised: lowercased, leading `.` stripped, `jpeg` → `jpg` | no |

An `identifier` is `[A-Za-z0-9_][A-Za-z0-9_-]*` that does not end with `-`. Normalisation lowercases ASCII letters and maps `-` to `_`.

---

## Appendix C — Worked example

A complete project exercising schemas, an explicit `id` declaration, groups, a nested reference, a tagged list, defaults, versions, an instance version window, a clone, interpolation, wildcards, tuple arrays and logic.

### C.1 Layout

```text
pack/
  assets/
    textures/
      frost.png            128x128 PNG
      ember.png            128x128 PNG
  data/
    templates/
      Pack.abt
    packs/
      winter_2026.ab
      spring_2026.ab
    stickers/
      frost.ab
      ember.ab
```

### C.2 `data/templates/Pack.abt`

```abstract
versions 1..2

schema Pack {
    id: text(3..24)
    title: text(1..40)
    tier: enum(free, plus) = free
    slot_count: int(1..9) @optional
}

schema Sticker {
    pack: ref(Pack)
    title: text(1..60)
    icon: image(png 128x128)
    rarity: enum(common, rare, epic) = common
    glow: bool @since(2) = false
    tint: int(0..255) @optional @removed(2)
    owner {
        team: text(1..40)
        contact: text(1..60) @optional
    }
    tags[]: enum(core_ui, core_game, promo) @optional
    copy[] {
        key: enum(en_us, es_es, es_mx) @tag
        value: text(1..80)
    }
}

logic Sticker {
    derive? .owner.contact = support@example.com

    if .rarity == "epic" {
        require .tags contains "promo"
            else throw "Epic stickers must carry the promo tag."
    }

    for $lang in [es_es, es_mx] {
        require .copy.key contains $lang
            else throw "Missing $lang copy."
    }
}
```

### C.3 `data/packs/winter_2026.ab` and `data/packs/spring_2026.ab`

```abstract
Pack :: @tier.plus
    title: Winter 2026
    slot_count: 3
```
```abstract
Pack :: @since(2)
    title: Spring 2026
```

- Each id comes from its file stem: `winter_2026` and `spring_2026`. Both are 11 Unicode scalar values, so both satisfy the declared `id: text(3..24)` (§4.11).
- `spring_2026` carries an instance window (§5.14): it exists in version 2 only. Its header has no tags at all, and `@since(2)` is an annotation rather than a tag because the identifier is followed immediately by `(` (§5.2).
- `tier` is absent from `spring_2026`, so its default `free` is filled; `slot_count` is optional and absent, so it is omitted (§8.9).

### C.4 `data/stickers/frost.ab`

```abstract
Sticker :: @id.frost, @rarity.rare
    pack: winter_2026
    title: Frost
    icon: ./textures/$id.png
    tint: 200
    owner.team: Studio A
    tags: core_*
    copy(key, value): (en_us, Frost), (es_*, Escarcha)
```

- `$id` interpolates to `frost`, so `icon` resolves to `pack/assets/textures/frost.png`.
- `core_*` expands to `core_ui, core_game`.
- `es_*` expands the tuple row into `es_es` and `es_mx`.
- `tint` is written for version 1 only, because the field does not exist in version 2; the statement needs no annotation (§5.13).

### C.5 `data/stickers/ember.ab`

```abstract
Sticker :: @id.ember, @rarity.epic
&frost.*
    title: Ember
    tags: [core_ui, promo]
    copy(key, value): (en_us, Ember), (es_*, Brasa)
```

- The clone copies `pack`, `title`, `icon` (still as the raw text `./textures/$id.png`), `rarity`, `owner`, `tags` and `copy` from frost's authored data; `id` and `template` are never cloned.
- Ember's own header and statements then override `rarity`, `title`, `tags` and `copy`.
- Interpolation runs after cloning, so `icon` becomes `./textures/ember.png`.
- In version 1 the clone also brings `tint: 200`; in version 2 that statement does not apply, so nothing is copied.

### C.6 `abstract compile pack JSON`

```json
{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 2
    }
  },
  "data": [
    {
      "template": "Pack",
      "id": "spring_2026",
      "title": "Spring 2026",
      "tier": "free"
    },
    {
      "template": "Pack",
      "id": "winter_2026",
      "title": "Winter 2026",
      "tier": "plus",
      "slot_count": 3
    },
    {
      "template": "Sticker",
      "id": "ember",
      "pack": "winter_2026",
      "title": "Ember",
      "icon": "./textures/ember.png",
      "rarity": "epic",
      "glow": false,
      "owner": {
        "team": "Studio A",
        "contact": "support@example.com"
      },
      "tags": [
        "core_ui",
        "promo"
      ],
      "copy": [
        {
          "key": "en_us",
          "value": "Ember"
        },
        {
          "key": "es_es",
          "value": "Brasa"
        },
        {
          "key": "es_mx",
          "value": "Brasa"
        }
      ]
    },
    {
      "template": "Sticker",
      "id": "frost",
      "pack": "winter_2026",
      "title": "Frost",
      "icon": "./textures/frost.png",
      "rarity": "rare",
      "glow": false,
      "owner": {
        "team": "Studio A",
        "contact": "support@example.com"
      },
      "tags": [
        "core_ui",
        "core_game"
      ],
      "copy": [
        {
          "key": "en_us",
          "value": "Frost"
        },
        {
          "key": "es_es",
          "value": "Escarcha"
        },
        {
          "key": "es_mx",
          "value": "Escarcha"
        }
      ]
    }
  ],
  "overlays": [
    {
      "versions": {
        "min": 1,
        "max": 1
      },
      "data": [
        {
          "template": "Sticker",
          "id": "ember",
          "pack": "winter_2026",
          "title": "Ember",
          "icon": "./textures/ember.png",
          "rarity": "epic",
          "tint": 200,
          "owner": {
            "team": "Studio A",
            "contact": "support@example.com"
          },
          "tags": [
            "core_ui",
            "promo"
          ],
          "copy": [
            {
              "key": "en_us",
              "value": "Ember"
            },
            {
              "key": "es_es",
              "value": "Brasa"
            },
            {
              "key": "es_mx",
              "value": "Brasa"
            }
          ]
        },
        {
          "template": "Sticker",
          "id": "frost",
          "pack": "winter_2026",
          "title": "Frost",
          "icon": "./textures/frost.png",
          "rarity": "rare",
          "tint": 200,
          "owner": {
            "team": "Studio A",
            "contact": "support@example.com"
          },
          "tags": [
            "core_ui",
            "core_game"
          ],
          "copy": [
            {
              "key": "en_us",
              "value": "Frost"
            },
            {
              "key": "es_es",
              "value": "Escarcha"
            },
            {
              "key": "es_mx",
              "value": "Escarcha"
            }
          ]
        }
      ],
      "removed": [
        "spring_2026"
      ]
    }
  ]
}
```

Reading the overlay: the project range is `1..2`, so the base is version 2 and the only version to reduce is 1. `ember` and `frost` each differ from the base throughout `1..1` (they carry `tint` and no `glow`), and `spring_2026` does not exist in version 1 while the base carries it, so it is a **remove** entry over the same range. All three entries share the range `1..1`, so §7.5 step 2 puts them in one overlay. `winter_2026` has no version-scoped field and no window, so it is identical in both versions and appears in no overlay.

A consumer running version 1 applies that one overlay: it replaces `ember` and `frost` with the objects in `data`, deletes `spring_2026`, and keeps `winter_2026` from the base. A consumer running version 2 matches no overlay and uses `data` unchanged.

### C.7 `abstract compile pack YML`

```yaml
"abstract":
  "format": 1
  "compiler": "1.0.0"
  "versions":
    "min": 1
    "max": 2
"data":
  - "template": "Pack"
    "id": "spring_2026"
    "title": "Spring 2026"
    "tier": "free"
  - "template": "Pack"
    "id": "winter_2026"
    "title": "Winter 2026"
    "tier": "plus"
    "slot_count": 3
  - "template": "Sticker"
    "id": "ember"
    "pack": "winter_2026"
    "title": "Ember"
    "icon": "./textures/ember.png"
    "rarity": "epic"
    "glow": false
    "owner":
      "team": "Studio A"
      "contact": "support@example.com"
    "tags":
      - "core_ui"
      - "promo"
    "copy":
      - "key": "en_us"
        "value": "Ember"
      - "key": "es_es"
        "value": "Brasa"
      - "key": "es_mx"
        "value": "Brasa"
  - "template": "Sticker"
    "id": "frost"
    "pack": "winter_2026"
    "title": "Frost"
    "icon": "./textures/frost.png"
    "rarity": "rare"
    "glow": false
    "owner":
      "team": "Studio A"
      "contact": "support@example.com"
    "tags":
      - "core_ui"
      - "core_game"
    "copy":
      - "key": "en_us"
        "value": "Frost"
      - "key": "es_es"
        "value": "Escarcha"
      - "key": "es_mx"
        "value": "Escarcha"
"overlays":
  - "versions":
      "min": 1
      "max": 1
    "data":
      - "template": "Sticker"
        "id": "ember"
        "pack": "winter_2026"
        "title": "Ember"
        "icon": "./textures/ember.png"
        "rarity": "epic"
        "tint": 200
        "owner":
          "team": "Studio A"
          "contact": "support@example.com"
        "tags":
          - "core_ui"
          - "promo"
        "copy":
          - "key": "en_us"
            "value": "Ember"
          - "key": "es_es"
            "value": "Brasa"
          - "key": "es_mx"
            "value": "Brasa"
      - "template": "Sticker"
        "id": "frost"
        "pack": "winter_2026"
        "title": "Frost"
        "icon": "./textures/frost.png"
        "rarity": "rare"
        "tint": 200
        "owner":
          "team": "Studio A"
          "contact": "support@example.com"
        "tags":
          - "core_ui"
          - "core_game"
        "copy":
          - "key": "en_us"
            "value": "Frost"
          - "key": "es_es"
            "value": "Escarcha"
          - "key": "es_mx"
            "value": "Escarcha"
    "removed":
      - "spring_2026"
```

### C.8 A failing compile

Changing ember's tags to `tags: [core_ui]` makes the `require` in `logic Sticker` fail:

```text
data/templates/Pack.abt:32:9: error[E515]: Sticker logic: Epic stickers must carry the promo tag.
  note: data/stickers/ember.ab:1:1: instance 'ember' declared here.
  note: compiling version 1.
```

Exit code 1; nothing is written to stdout. Version 1 is named because versions are compiled in ascending order (§7.4) and the same `require` fails in both.

---

## Appendix D — Optional tooling

`bundle`, `unbundle`, the Java runtime and the editor extension are not part of the language and are not required for conformance (§9.9). They ship with the reference implementation, and the reference implementation MUST satisfy the following. None of these items changes compilation semantics.

**D.1 `bundle` and `unbundle`**

1. `--key` together with `--plain` is a usage error. A supplied key MUST NOT be silently discarded, and no command may write an unencrypted container while a key was given.
2. No flag value may begin with `-` (§9.3): `bundle data --key --out x.abx` is a usage error, not a container sealed with the passphrase `--out`.
3. Opening a container whose encryption flag has been cleared MUST fail whether or not the caller supplied a key. The refusal MUST NOT depend on caller behaviour.
4. `unbundle` without a key on an unencrypted container is supported, and `--key` against a container that declares itself unencrypted is refused. Both MUST be documented.
5. The nonce construction MUST be documented completely, including any per-process counter and what resets it.

A `bundle` or `unbundle` usage error carries **no chapter 10 identifier**: chapter 10 catalogues the diagnostics of the language, and none of these conditions can arise in `compile`, `lint`, `templates` or `init`. The reference implementation prints such an error as `abstract: error: {message}` and exits 2. The behaviour is what these items fix; the spelling of the identifier is not, because there is none to assign.

**D.2 Java runtime**

6. The JSON reader MUST enforce a nesting-depth limit and report it as the runtime's own exception type, never as `StackOverflowError`.
7. The JSON reader MUST reject a leading `+`, leading zeros, duplicate object keys and lone surrogates.
8. Every parsing failure, including a malformed `\u` escape, MUST surface as the runtime's own exception type.
9. `fromHex(null)` and non-hexadecimal digits MUST surface as the runtime's own exception type, never as `NullPointerException` or `NumberFormatException`.
10. Key derivation MUST follow the same rule as the compiler: a bare 64-character hexadecimal string is a passphrase, and only the `hex:` prefix selects raw key bytes.
11. The runtime MUST expose the version axis (`forVersion(int)`), applying **every** overlay whose range contains the requested version, exactly as §7.5 step 3 prescribes: replace or add by `id` from the overlay's `data`, then delete the ids in its `removed`. It MUST NOT assume that at most one overlay matches a version, and MUST NOT assume that every id in an overlay is already present in the base.

**D.3 Editor extension**

12. The compiler MUST be invoked with an argument vector. No workspace-controlled string may be interpolated into a shell command line.
13. The extension MUST resolve the project as §2.3 prescribes before linting, and MUST NOT lint a single `.ab` file out of project context.
14. The syntax grammar MUST cover the 1.0 language: nested braces inside `schema`, `logic` and instance body blocks, every type keyword of §4.4, `versions`, `@since`, `@removed`, `ref` and `$(Schema)`.

**D.4 Distribution**

15. Every install command printed in the project's documentation MUST resolve, as written, from the repository root.
