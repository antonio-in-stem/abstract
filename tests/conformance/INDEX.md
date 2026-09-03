# Abstract 1.0 conformance corpus - index

Total cases: 282

## Counts by area

| area | cases |
|---|---|
| adversarial-1 | 27 |
| adversarial-2 | 11 |
| cli-project | 29 |
| docs-tests-examples | 32 |
| instances-merge | 25 |
| language-design | 24 |
| lexing-parsing | 31 |
| logic-engine | 28 |
| media-bundle-crypto | 14 |
| output-canon | 27 |
| revision-7 | 4 |
| schema-types | 30 |

## Counts by severity

| severity | cases |
|---|---|
| critical | 9 |
| design | 19 |
| major | 96 |
| minor | 158 |

## Counts by expected_kind

| expected_kind | cases |
|---|---|
| document | 41 |
| error | 128 |
| ok | 2 |
| undecided | 111 |

## Counts by status

| status | cases |
|---|---|
| manual | 9 |
| ready | 273 |

## All cases

| area | id | severity | expected_kind | title |
|---|---|---|---|---|
| adversarial-1 | ADV-01 | critical | undecided | A wide `versions` range aborts the process with an allocation failure instead of a diagnostic |
| adversarial-1 | ADV-02 | major | error | 62 nested parentheses in a logic condition overflow the stack in the default (debug) build |
| adversarial-1 | ADV-03 | major | error | A clone chain of 80 is accepted when the ids sort along the chain: the limit is enforced against resolver recursion, not chain length |
| adversarial-1 | ADV-03b | major | error | The same 80-long clone chain written in the other direction is E209 - the pair shows the diagnostic depends on id spelling |
| adversarial-1 | ADV-04 | minor | error | A value at exactly the documented bracket nesting depth of 64 is rejected as E209 instead of E441 |
| adversarial-1 | ADV-05 | minor | document | A schema with exactly 64 nested groups is rejected as E209, one level below the documented limit |
| adversarial-1 | ADV-06 | minor | document | A logic block with exactly 64 nested `if` blocks is rejected as E209, one level below the documented limit |
| adversarial-1 | ADV-07 | minor | error | A ']' closing a '(' is not reported as a bracket-type mismatch (E204) |
| adversarial-1 | ADV-08 | minor | error | A byte-order mark inside a bare value is accepted and reaches the compiled document |
| adversarial-1 | ADV-09 | minor | error | Non-ASCII whitespace is ordinary text inside a bare value; a control character in the same position is E210 |
| adversarial-1 | ADV-10 | minor | error | A bare boolean literal is accepted as a logic condition, where SPEC 6.6 makes every bare literal E513 |
| adversarial-1 | ADV-11 | minor | error | Whitespace between a field name and its list head is silently accepted |
| adversarial-1 | ADV-12 | minor | error | A malformed image size token is reported as E206 with a message that contradicts itself |
| adversarial-1 | ADV-13 | minor | document | The integer literal -0 assigned to a float field renders as -0.0, which is not structurally equal to 0.0 |
| adversarial-1 | ADV-14 | major | error | An unterminated `${` in an unquoted value is E203, not the E427 the catalogue gives as its own example |
| adversarial-1 | ADV-15 | minor | error | A leading ',' in an instance header tag list is silently discarded |
| adversarial-1 | ADV-16 | minor | error | A trailing ',' in a multi-path key list is silently discarded |
| adversarial-1 | ADV-17 | minor | error | Whitespace between a field name and a tuple-array column list is accepted, although SPEC 3.1 names that '(' as a single lexeme |
| adversarial-1 | ADV-18 | major | error | A bare flag argument of a #tag object on a non-bool field is silently coerced to the text 'true |
| adversarial-1 | ADV-19 | minor | error | A trailing comma after an unbracketed tuple row is E210 naming the line terminator, not E442 |
| adversarial-1 | ADV-20 | minor | error | Trailing text after a closed quoted value is reported at the wrong position when the trailing text contains a quote |
| adversarial-1 | ADV-21 | minor | ok | An instance 62 levels deep compiles: the depth limit counts the instance object as level 1 and the envelope not at all |
| adversarial-1 | ADV-21B | minor | error | An instance 65 levels deep is E209 at the statement that reaches the limit, with a file, line and column |
| adversarial-1 | ADV-22 | minor | error | A body block written on one line reports E203 and E205, whose conditions did not occur, instead of the E210 every sibling spelling reports |
| adversarial-1 | ADV-23 | minor | error | An identifier that begins with '-' is E210 at the '-', never E206 |
| adversarial-1 | ADV-24 | minor | error | A bare value that begins with '//' is E210 wherever the '//' stands, the ':' included |
| adversarial-1 | ADV-25 | minor | error | A brace pattern with nine groups is E435; eight is the bound |
| adversarial-2 | ADV2-01 | major | error | A body-statement annotation above the project range reports E430 instead of E603 |
| adversarial-2 | ADV2-02 | major | error | A body-statement `@removed(n)` above the project range is silently accepted |
| adversarial-2 | ADV2-03 | minor | error | A body-statement `@since(0)` below the project minimum reports E430 instead of E603 |
| adversarial-2 | ADV2-04 | minor | document | A clone chain of exactly the documented depth 64 is rejected as E209 |
| adversarial-2 | ADV2-05 | minor | error | `&other.id` reports E408 "does not exist on instance" before the E410 the spec prescribes |
| adversarial-2 | ADV2-06 | minor | error | `schema` at statement position in a .ab file reports E316 "Invalid field name" instead of E210 |
| adversarial-2 | ADV2-07 | minor | error | A `versions` declaration in a .ab file reports E403 instead of E210 |
| adversarial-2 | ADV2-08 | major | document | U+007F in a text value is escaped, so the YAML document a conforming reader gets is parseable |
| adversarial-2 | ADV2-09 | minor | error | A header tag whose applicability set is empty is E440, the same diagnostic the equivalent body statement gets |
| adversarial-2 | ADV2-10 | minor | error | Interpolating a loop variable bound to a group object is E522, not a silent drop |
| adversarial-2 | ADV2-11 | minor | error | A reversed window on a body statement is E604, the diagnostic an instance header already gets for it |
| cli-project | cli-01 | minor | document | Single-file compile cannot find templates outside the file's own directory - the layout `abstract init` generates fails |
| cli-project | cli-02 | major | document | Direct/subdirectory compile resolves file()/image() against the .ab file's own folder, so any project with assets fails in direct mode |
| cli-project | cli-03 | major | error | file()/image() values are not confined to the project: `../..` traversal and absolute paths are validated on disk and emitted verbatim |
| cli-project | cli-04 | minor | error | Passing a file together with a directory that contains it yields a bogus 'Duplicate instance id ... already declared in <itself>' |
| cli-project | cli-05 | major | error | Mixing a .ab file and a directory silently drops every instance in the directory (exit 0) |
| cli-project | cli-06 | major | document | `--out=<file>` (equals form) is silently ignored: nothing is written, exit 0 |
| cli-project | cli-07 | major | error | Unrecognized options and unrecognized positional tokens are silently ignored; a mistyped format falls back to JSON |
| cli-project | cli-08 | major | error | `bundle --key <pass> --plain` silently writes an UNENCRYPTED bundle |
| cli-project | cli-09 | major | document | A UTF-8 BOM silently deletes a schema from a .abt, and produces a self-contradictory error in a .ab |
| cli-project | cli-10 | major | document | `.AB` / `.ABT` files are silently invisible to the compiler on Windows |
| cli-project | cli-11 | minor | error | `--skip-assets` does not skip absolute-path file()/image() values, contradicting the documentation |
| cli-project | cli-12 | design | error | `--out` overwrites any existing file, including project sources, with no guard or confirmation |
| cli-project | cli-13 | minor | error | A leading `/` or `\` in an asset value silently discards the assets/ folder and resolves at the drive root |
| cli-project | cli-14 | minor | document | Requesting a file with no instances silently emits the entire directory instead |
| cli-project | cli-15 | minor | document | Hidden/vendor directories are walked with no ignore rules, breaking builds with confusing duplicate errors |
| cli-project | cli-16 | minor | document | Directory junctions/symlinks are followed with no cycle detection or containment: a loop yields a 775-char nonsense error, and a junction pulls in sources from outside the project |
| cli-project | cli-17 | minor | error | Missing input paths produce a bare localized OS error for compile/lint/templates, while only the direct form gets the friendly message |
| cli-project | cli-18 | minor | error | An empty or wrong-but-existing directory compiles to `{"data": []}` and lints as ok |
| cli-project | cli-19 | minor | error | A non-Abstract file passed to compile silently compiles its containing directory |
| cli-project | cli-20 | minor | error | Extra positional paths are silently dropped and never validated |
| cli-project | cli-21 | minor | document | The `data` project-root marker is an exact lowercase string match, so `Data`/`DATA` silently resolves assets to the wrong folder |
| cli-project | cli-22 | minor | error | An `assets/` folder placed inside `data/` is unreachable and the error never explains the parent-of-data rule |
| cli-project | cli-23 | minor | document | Windows verbatim `\\?\` prefix leaks into every asset error message |
| cli-project | cli-24 | minor | error | Direct form: the trailing `true` must sit immediately after the format token or it is parsed as an input path |
| cli-project | cli-25 | minor | document | A directory named json/yml/yaml/raw/true/false cannot be used as an input path |
| cli-project | cli-26 | minor | error | Compiled output is printed to stdout even when --out is given, and a failed write leaves a full success payload on stdout with exit 1 |
| cli-project | cli-27 | minor | ok | `templates` reports success on projects that cannot compile and silently ignores --skip-assets/--allow-unknown |
| cli-project | cli-28 | major | document | No tests exist for the CLI layer: argument parsing, init, --out, direct-form ordering and project-root discovery are entirely uncovered |
| cli-project | cli-29 | minor | error | `--max-errors 0` is E812: the flag takes an integer from 1 to 10000 and there is no "no limit" spelling |
| docs-tests-examples | DTE-01 | critical | error | A line starting with `data:` silently truncates the rest of the .ab file with no diagnostic |
| docs-tests-examples | DTE-02 | major | document | The primary `logic` example in every doc surface does not compile: `derive` to a field the schema does not declare is rejected |
| docs-tests-examples | DTE-03 | major | undecided | `--skip-assets` silently changes compiled DATA, not just validation, because `exists` becomes unconditionally true |
| docs-tests-examples | DTE-04 | major | undecided | `exists` decides filesystem-check vs truthiness from the VALUE's shape, not the field's declared type — contradicting both language docs |
| docs-tests-examples | DTE-05 | major | error | `abstract bundle --key <secret> --plain` silently discards the key and writes an UNENCRYPTED bundle |
| docs-tests-examples | DTE-06 | major | undecided | VS Code extension builds a shell command string and runs it with cp.exec — command injection from a workspace path or workspace setting |
| docs-tests-examples | DTE-07 | major | error | Brace file patterns with more than one brace group silently produce corrupt paths; braces in a plain text value are silently stripped |
| docs-tests-examples | DTE-08 | minor | error | Nested bracket list literals are silently misparsed into strings instead of nested arrays or an error |
| docs-tests-examples | DTE-09 | minor | undecided | The `lang` → `lang_values` output rename collides with a real `lang_values` field and silently destroys one of them |
| docs-tests-examples | DTE-10 | major | error | An unknown/typo'd path in a logic condition silently evaluates false, so `require` guards stop enforcing without any error |
| docs-tests-examples | DTE-11 | minor | undecided | RAW output indentation is wrong: every instance's opening brace is emitted at column 0, contradicting the documented RAW contract |
| docs-tests-examples | DTE-12 | minor | undecided | The YAML emitter never quotes or escapes mapping keys, so `--allow-unknown` can produce output that is not valid YAML |
| docs-tests-examples | DTE-13 | minor | undecided | Direct/single-file compile loads sibling context only from the file's own directory, so it fails on the layout `abstract init` itself generates |
| docs-tests-examples | DTE-14 | major | undecided | The `template` output field can be overwritten by an instance assignment or by a schema field, breaking the documented output contract |
| docs-tests-examples | DTE-15 | minor | undecided | VS Code extension lints a single .ab file by default, producing false 'Unknown template' errors on every project `abstract init` creates |
| docs-tests-examples | DTE-16 | minor | error | Asset paths are resolved with no containment check: `../` and absolute paths escape the project and are validated/probed on disk |
| docs-tests-examples | DTE-17 | major | undecided | Quoting a value does NOT keep it as text for int/float/bool fields — contradicting the compiler's own error message and NEW-FEATURES.md's migration advice |
| docs-tests-examples | DTE-18 | minor | undecided | Three doc surfaces claim PNG validation reads 26 header bytes; the probe accepts a 24-byte file |
| docs-tests-examples | DTE-19 | minor | undecided | A truncated PNG with a valid signature is reported as 'unrecognized image signature' |
| docs-tests-examples | DTE-20 | minor | undecided | The shipped `example/` project is byte-identical to v0.1 and exercises none of the 0.2 features docs tell collaborators to demo with it |
| docs-tests-examples | DTE-21 | minor | undecided | NEW-FEATURES.md contradicts itself about v0.1 compatibility |
| docs-tests-examples | DTE-22 | minor | undecided | A misspelled `$variable` is silently left as literal text instead of being reported |
| docs-tests-examples | DTE-23 | minor | error | Multi-path braces only support a single-segment prefix: `a.b.{c, d}` builds a field literally named `a.b` |
| docs-tests-examples | DTE-24 | minor | undecided | A project directory named raw/json/yml/yaml/true/false cannot be passed as an input path |
| docs-tests-examples | DTE-25 | minor | undecided | `--plain` and the JSON output-format default are absent from all four markdown docs and README |
| docs-tests-examples | DTE-26 | minor | error | `length()` on a non-array returns 1, so `length(.text_field) == n` is a silently always-false guard |
| docs-tests-examples | DTE-27 | minor | undecided | README's prebuilt-install command uses a path that does not resolve from the repository root |
| docs-tests-examples | DTE-28 | minor | undecided | Logic `==` on text compares case-insensitively and treats `-` and `_` as equal, while ids are compared three different ways |
| docs-tests-examples | DTE-29 | minor | undecided | An empty or wrongly-pointed project directory compiles, lints and bundles as a success |
| docs-tests-examples | DTE-30 | minor | undecided | VS Code grammar and extension are 0.1-era: schema/logic scopes end at the first nested `}`, float/bool/image/not are unhighlighted, completions omit every 0.2 keyword |
| docs-tests-examples | DTE-31 | minor | undecided | No test in the repository executes the CLI binary; RAW output shape and ~12 other behaviours are untested |
| docs-tests-examples | DTE-32 | minor | error | Undocumented single-tuple multi-assignment `(a, b): (1, 2)` accepts duplicate column names and silently keeps the last value |
| instances-merge | IM-01 | critical | undecided | Any statement starting with `data:` silently truncates the rest of the .ab file |
| instances-merge | IM-02 | major | error | Two schema fields `lang` and `lang_values` silently collapse into one; one field's value is lost |
| instances-merge | IM-03 | major | undecided | Compiled output can contain duplicate `id` values; the documented uniqueness guarantee is not enforced on the emitted id |
| instances-merge | IM-04 | major | undecided | `@id` accepts non-string scalars (int/float/bool), which bypasses duplicate-id detection entirely and emits a non-string id |
| instances-merge | IM-05 | major | undecided | Compiler panics ('path target must be an object') on a dotted assignment whose prefix already holds a scalar |
| instances-merge | IM-06 | major | undecided | Stack overflow (uncatchable process abort) on a deep clone chain or a deep dotted path |
| instances-merge | IM-07 | major | undecided | Clone resolution is exponential: a 98-line file takes 38 seconds to compile |
| instances-merge | IM-08 | major | error | A `&clone.*` line written after the instance header is silently discarded (or silently applied to the NEXT instance) |
| instances-merge | IM-09 | major | error | A trailing comma silently swallows the following line into the previous statement |
| instances-merge | IM-10 | major | error | An unterminated `[` at end of file becomes a one-element array containing the raw text |
| instances-merge | IM-11 | major | undecided | An enum wildcard that matches nothing silently expands to an empty list instead of erroring |
| instances-merge | IM-12 | major | undecided | Tuple-array wildcards do not normalize the prefix, so `ES_*` / `es-*` silently expand to nothing while the same prefix works in a plain enum list |
| instances-merge | IM-13 | major | undecided | `file()` values escape the assets sandbox via `../` and absolute paths; the traversal can be assembled from a cloned field plus interpolation |
| instances-merge | IM-14 | minor | undecided | Non-ASCII identifiers are not normalized: schema fields and enum values become unreachable mixed-case names |
| instances-merge | IM-15 | major | undecided | A UTF-8 BOM breaks .ab parsing and silently prevents a .abt schema from being registered |
| instances-merge | IM-16 | minor | undecided | `@id.` (trailing dot) silently produces an empty instance id |
| instances-merge | IM-17 | major | undecided | A partial clone of a string leaves un-interpolated `$var` text in the compiled output |
| instances-merge | IM-18 | major | undecided | Interpolation has no variable-name boundary: a mistyped `$named` silently becomes the value of `$name` plus the leftover characters |
| instances-merge | IM-19 | design | undecided | A tagged group list accepts duplicate tag values, so a keyed list can carry conflicting entries for the same key |
| instances-merge | IM-20 | design | undecided | A clone replaces list fields wholesale, including tagged (keyed) lists — there is no way to override one entry |
| instances-merge | IM-21 | design | document | Partial clones cannot see schema defaults or logic-derived values, and fail with a misleading 'not found on the source instance' |
| instances-merge | IM-22 | minor | undecided | `#prefix*` expands inside brackets but hard-errors when written bare, despite single-value-to-list coercion being documented as equivalent |
| instances-merge | IM-23 | design | error | A clone across templates silently smuggles the source template's fields into the target under --allow-unknown, and the rejection message lists 'template, id' as declared schema fields |
| instances-merge | IM-24 | minor | undecided | Instance output order is lexicographic full-path order and is not documented anywhere |
| instances-merge | IM-25 | design | undecided | An instance id containing a dot cannot be used as a partial-clone target |
| language-design | L01 | major | error | A typo in a `logic <Name>` block silently disables every rule in it (exit 0, no diagnostic) |
| language-design | L02 | major | error | Any unrecognized top-level block in a .abt is silently ignored — a typo'd `schema` keyword produces zero diagnostics and `lint` reports ok |
| language-design | L03 | critical | document | `data:` is an undeclared reserved word that silently truncates the rest of an .ab file — for a round-trip that does not work |
| language-design | L04 | major | undecided | `&clone` lines bind forward to the *next* instance header and are silently discarded when there is none |
| language-design | L05 | design | undecided | Version overlays are unexpressible today: there is no way to REMOVE a key, and clone-based variants produce separate top-level ids with no version metadata — concrete syntax proposal with two alternatives |
| language-design | L06 | design | undecided | No schema versioning or migration path: `--allow-unknown` is the only tool and it is project-wide, silent, and injects unvalidated fields into the output — concrete syntax proposal with two alternatives |
| language-design | L07 | major | undecided | Compiled output carries no schema/version/provenance metadata, and `template`/`id` are reserved keys that an instance can silently overwrite |
| language-design | L08 | major | undecided | Schemas nest with brace groups but instances cannot: `.abt` and `.ab` use non-transferable nesting syntax, and nested list groups are unreachable from instance syntax |
| language-design | L09 | minor | undecided | `@field.value` in a header and `field.value:` in a body mean opposite things — the same dotted notation is a value separator in one place and a path separator in the other |
| language-design | L10 | major | undecided | Duplicate field assignments in one instance body silently last-win, while duplicate schema fields, schema names and instance ids are all hard errors |
| language-design | L11 | major | error | `$var` interpolation runs before schema defaults are filled and only sees root scalars, so `$defaulted_field` silently stays literal in the output; an undefined `$var` is never reported |
| language-design | L12 | design | undecided | No compiler-validated cross-instance references: `$(Schema)` is a shape reference, not a link, so every relationship between instances is untyped free text |
| language-design | L13 | minor | undecided | `{...}` means fan-out on both sides of the colon with opposite semantics, and on the right its activation depends on whether the value contains a space |
| language-design | L14 | minor | undecided | Quoting rules are value-dependent and surprising: a comma makes prose into a list, and a two-part version number like `1.3` is a float, not text |
| language-design | L15 | minor | undecided | A domain-specific rename (`lang` -> `lang_values`) is hardcoded in the general-purpose compiler and leaks into error messages as a field name that cannot be spelled in a schema |
| language-design | L16 | design | undecided | No imports, namespaces or constants: an .ab file cannot declare which schema it belongs to, and cross-project schema reuse only works by naming every file on the command line |
| language-design | L17 | major | undecided | The documented direct-compile form `abstract object.ab Template.abt JSON` fails with a self-contradictory duplicate-schema error whenever the template sits under the object's own directory — the layout the docs recommend |
| language-design | L18 | major | error | No null, no absent/empty distinction, and no list cardinality: `field:` silently yields "", `null` is the literal string "null", and a required list group accepts an empty array |
| language-design | L19 | minor | undecided | Identifier normalization follows three different rules in one file, and case folding is ASCII-only so unicode field names half-normalize |
| language-design | L20 | minor | undecided | Unicode schema names are accepted by .abt and listed by `abstract templates`, but no .ab file can ever reference them — and the failure message is about something else entirely |
| language-design | L21 | major | undecided | Diagnostics lose position and blame the wrong file: logic errors carry no line or array index, and a logic-derived unknown field is reported against the .ab |
| language-design | L22 | critical | error | Tuple-array syntax is detected by the presence of any parenthesis on the left, and a tuple list missing its parentheses silently compiles to an empty array |
| language-design | L23 | minor | undecided | Comments are stripped at tokenization and can never reach the output, so the compiled artifact loses all authored intent |
| language-design | L24 | minor | undecided | Unknown-field diagnostics list `template` and repeat `id` as declared fields, and asset errors print Windows `\\?\` UNC paths |
| lexing-parsing | LP-001 | major | undecided | A trailing comma at the end of a statement silently swallows the following statements — and can delete an entire instance |
| lexing-parsing | LP-002 | major | error | A `&clone` line written after its instance header is silently ignored, dropping all inherited fields |
| lexing-parsing | LP-003 | critical | undecided | A field named `data` silently truncates the rest of the instance file and drops itself |
| lexing-parsing | LP-004 | major | error | An unbalanced bracket silently absorbs the entire rest of the file into one string value |
| lexing-parsing | LP-005 | major | error | Panic (process abort) parsing a `#tag` value whose `(` comes after its last `)` |
| lexing-parsing | LP-006 | major | undecided | Panic (process abort) expanding a brace pattern whose `}` precedes its `{` |
| lexing-parsing | LP-007 | major | undecided | Panic (process abort) on a multi-path left side whose `}` precedes its `.{` |
| lexing-parsing | LP-008 | major | error | Stack overflow (unrecoverable abort) on deeply nested `#tag(...)` values or long dotted paths |
| lexing-parsing | LP-009 | major | error | The `template` field of the emitted object can be forged from the instance body |
| lexing-parsing | LP-010 | major | undecided | Assigning `id` in the body desynchronizes the registry id from the emitted id, producing duplicate ids in the output |
| lexing-parsing | LP-011 | major | document | `${a}_${b}` in a file path is silently corrupted by brace-pattern expansion |
| lexing-parsing | LP-012 | minor | error | A brace pattern with more than one `{...}` group silently produces corrupt paths instead of a cross product or an error |
| lexing-parsing | LP-013 | major | error | Text with braces that is not a file path is silently mangled by pattern expansion |
| lexing-parsing | LP-014 | major | undecided | `$` interpolation result depends on the declaration order of fields, so two logically identical instances compile differently |
| lexing-parsing | LP-015 | major | undecided | `$` interpolation has no word boundary, so literal text that merely starts with a field name is rewritten |
| lexing-parsing | LP-016 | major | document | A numeric `@id` makes the instance register under its file stem, producing a bogus "Duplicate instance id" error that names an id nobody wrote |
| lexing-parsing | LP-017 | major | undecided | A protocol-relative URL or any value containing ` //` is silently truncated as a comment |
| lexing-parsing | LP-018 | minor | error | Arbitrary text between tuples in a tuple array is silently discarded |
| lexing-parsing | LP-019 | major | error | `file(...)` values escape the project assets directory via `..` or an absolute path, and the error message leaks the resolved absolute path |
| lexing-parsing | LP-020 | minor | error | A stray `::` in the header tail is silently absorbed into the previous tag's value instead of erroring |
| lexing-parsing | LP-021 | minor | document | A literal U+0001 in a value is silently rewritten to `$` by the `$$` escape sentinel |
| lexing-parsing | LP-022 | minor | error | Nested lists are silently flattened into strings rather than parsed or rejected |
| lexing-parsing | LP-023 | minor | error | A header tag value beginning with `./` silently loses its leading dot |
| lexing-parsing | LP-024 | minor | undecided | Clone target ids are case-sensitive while every other identifier is lowercased |
| lexing-parsing | LP-025 | design | undecided | The hardcoded `lang` -> `lang_values` rename is applied to every field at every nesting depth, not just tuple arrays |
| lexing-parsing | LP-026 | minor | error | `\n`, `\t` and `\r` inside quoted Windows paths are silently turned into control characters while every other escape passes through |
| lexing-parsing | LP-027 | minor | document | A UTF-8 BOM makes the first instance header unrecognizable and produces a misleading error |
| lexing-parsing | LP-028 | minor | undecided | A schema name containing a hyphen is accepted by the schema parser but can never be instantiated, and the error blames the wrong thing |
| lexing-parsing | LP-029 | minor | error | Field names differing only in case or in `-` vs `_` silently overwrite each other |
| lexing-parsing | LP-030 | minor | undecided | Interpolation cost is the product of field count and string count, making large instances superlinearly slow |
| lexing-parsing | LP-031 | minor | undecided | Diagnostics list `id` twice in "Declared fields", and a duplicate tuple column reports a misleading "missing field" error |
| logic-engine | LOGIC-01 | critical | error | derive through a non-object path panics the compiler (exit 101); the 2-segment form silently drops the write |
| logic-engine | LOGIC-02 | major | undecided | A logic block whose schema name does not match exactly is silently ignored — every rule in it disappears |
| logic-engine | LOGIC-03 | critical | error | A stray '}' silently truncates a logic block; every rule after it is discarded with no error and lint reports ok |
| logic-engine | LOGIC-04 | critical | error | Any condition the evaluator does not recognise falls back to string truthiness and therefore evaluates TRUE — require guards silently pass |
| logic-engine | LOGIC-05 | major | undecided | Compiled output is non-deterministic: throw messages and derived values differ run-to-run because loop variables are substituted in HashMap iteration order |
| logic-engine | LOGIC-06 | major | undecided | 'exists' means 'truthy', so an authored false / 0 / "" is reported as missing and the standard fill-in idiom silently overwrites it |
| logic-engine | LOGIC-07 | major | undecided | A for-loop variable hijacks an identically named segment of a root-anchored '.path', silently reading a different field |
| logic-engine | LOGIC-08 | major | error | 'exists' probes the filesystem with no containment: absolute paths and '..' escape assets/ and the project, leaking file existence into compiled output |
| logic-engine | LOGIC-09 | major | undecided | derive can overwrite .id and .template, producing duplicate ids and a template name that was never validated |
| logic-engine | LOGIC-10 | major | error | A quoted string literal is silently replaced by a bound variable of the same name — quoting does not protect a literal |
| logic-engine | LOGIC-11 | major | error | A for-loop over a missing or misspelled path iterates once, binding the literal path text as the item |
| logic-engine | LOGIC-12 | major | error | '==' and '!=' against a path projected across a list of groups are existential (ANY), so 'no item is X' guards silently pass |
| logic-engine | LOGIC-13 | minor | undecided | $variables are not substituted in a derive TARGET path, contradicting the documented dynamic-path feature; a literal '$slot' field is created instead |
| logic-engine | LOGIC-14 | major | error | derive values cannot read another field, index a path, or call length() — such right-hand sides are written into the data as literal text |
| logic-engine | LOGIC-15 | major | error | A logic block attached to a schema used as a nested $(Schema) type never runs |
| logic-engine | LOGIC-16 | major | error | '$field' interpolation has no word boundary, so any text where a field name is a prefix of the following word is silently corrupted |
| logic-engine | LOGIC-17 | major | error | 'contains' on a text field is exact equality, not substring — content-policy guards silently never fire |
| logic-engine | LOGIC-18 | minor | document | derive cannot satisfy a required field — the pre-logic validation pass rejects the instance first, and the docs' own example does not compile |
| logic-engine | LOGIC-19 | minor | error | A loop variable spelled with capitals resolves in dynamic paths but is not interpolated into derive values or throw messages |
| logic-engine | LOGIC-20 | minor | undecided | The ' else throw ' split is not quote-aware, so a condition containing that phrase in a string misparses into a wrong condition and a garbage message |
| logic-engine | LOGIC-21 | minor | error | A derive inside a for-loop overwrites its target every iteration; there is no way to accumulate a list |
| logic-engine | LOGIC-22 | minor | error | length() counts elements, so on any scalar it is always 1 — length-based guards on text silently always fail |
| logic-engine | LOGIC-23 | minor | undecided | '==' is case-insensitive for scalars but case-sensitive for arrays (structural fallback) |
| logic-engine | LOGIC-24 | minor | undecided | Interpolation is re-entrant: a '$' that came from instance DATA is expanded again, defeating the $$ escape |
| logic-engine | LOGIC-25 | minor | undecided | Deeply nested if blocks overflow the stack and abort with no diagnostic (~1000 levels) |
| logic-engine | LOGIC-26 | minor | undecided | A single-line 'logic X { ... }' is unsupported and reports the misleading error 'logic block was not closed' |
| logic-engine | LOGIC-27 | minor | undecided | The unknown-field diagnostic for a bad derive target lists 'id' twice and carries no .abt line number |
| logic-engine | LOGIC-28 | minor | undecided | No negative tests for the logic engine: every logic test in the suite is a happy path |
| media-bundle-crypto | mbc-01 | minor | error | file()/image() values escape the asset root via `..` and absolute paths (filesystem read + existence oracle from authored data) |
| media-bundle-crypto | mbc-02 | design | undecided | Documented 'downgrade resistance' is only enforced when the caller passes a key; opening a flag-flipped bundle with no key silently returns raw ciphertext |
| media-bundle-crypto | mbc-03 | minor | undecided | Json.java has no nesting-depth limit: deep JSON throws uncaught StackOverflowError, violating the AbstractDataException contract |
| media-bundle-crypto | mbc-04 | design | undecided | Valid 25-29 byte VP8L WebP rejected as 'unrecognized image signature' due to the uniform 30-byte WebP gate |
| media-bundle-crypto | mbc-05 | minor | undecided | docs claim '--skip-assets disables all on-disk checks' but absolute paths are still opened/checked (contradicts the same file) |
| media-bundle-crypto | mbc-06 | minor | undecided | `--key` swallows a following flag token as its value: `bundle data --key --out x.abx` silently seals with passphrase '--out' |
| media-bundle-crypto | mbc-07 | minor | undecided | JPEG marker walk hard-capped at 256 iterations rejects valid JPEGs whose SOF sits beyond segment 256 |
| media-bundle-crypto | mbc-08 | minor | undecided | AbstractKeys.fromHex accepts a bare 64-hex string while the Rust CLI treats the same string as a passphrase (silent cross-tool key mismatch) |
| media-bundle-crypto | mbc-09 | minor | undecided | AbstractKeys.fromHex leaks java.lang.NumberFormatException on non-hex digits instead of AbstractDataException |
| media-bundle-crypto | mbc-10 | minor | undecided | Truncated file with a valid format magic is reported as 'unrecognized image signature', indistinguishable from a non-image |
| media-bundle-crypto | mbc-11 | design | undecided | VP8X (extended WebP) parses canvas dimensions with no signature/sanity check at all |
| media-bundle-crypto | mbc-12 | design | error | BMP width/height run through unsigned_abs, silently accepting malformed negative dimensions |
| media-bundle-crypto | mbc-13 | minor | undecided | Docs state PNG probing reads '26 bytes'; actual buffer is a fixed 32 bytes and PNG needs only 24 (26 is BMP's gate) |
| media-bundle-crypto | mbc-14 | minor | undecided | Json.java is materially more permissive than strict JSON (leading +/zeros, duplicate keys last-wins, lone surrogate -> '?') |
| output-canon | OC-01 | major | undecided | YAML output writes mapping keys unquoted: numeric and boolean-looking schema field names change type, and yes/on (no/off) collide and silently drop fields |
| output-canon | OC-02 | major | undecided | Stack overflow (hard crash, no diagnostic) from a deeply nested dotted assignment path |
| output-canon | OC-03 | major | error | Panic 'path target must be an object' (exit 101) when a 3+ segment path crosses an existing scalar |
| output-canon | OC-04 | major | error | Two-segment assignment onto an existing scalar is silently discarded - the value never appears in output and no error is raised |
| output-canon | OC-05 | major | error | A plain `template:` / `id:` assignment silently rewrites the output envelope keys, producing a phantom template name and duplicate ids that defeat the duplicate-instance-id check |
| output-canon | OC-06 | minor | undecided | A schema declaring both `lang` and `lang_values` silently collapses them into one output key, losing one field's value |
| output-canon | OC-07 | minor | document | YAML renders an empty object inside an array as `- ` (null), diverging from JSON's `{}` |
| output-canon | OC-08 | minor | document | `abstract compile ... \| head` panics on broken pipe (exit 101) and the --out file is never written |
| output-canon | OC-09 | minor | error | `--out` silently overwrites a source .ab/.abt file, destroying it |
| output-canon | OC-10 | minor | undecided | --allow-unknown field names are emitted as unquoted YAML/RAW keys, producing invalid YAML or silently vanished fields |
| output-canon | OC-11 | minor | undecided | RAW output's indentation is internally inconsistent and contradicts the documented example: the opening brace sits at column 0 |
| output-canon | OC-12 | design | undecided | RAW renders every array of objects on one unbroken line, defeating the format's stated purpose (diffs and reviews) |
| output-canon | OC-13 | minor | undecided | RAW emits a blank line for an empty project (`data: [\n\n]`), where JSON and YAML emit `[]` |
| output-canon | OC-14 | minor | undecided | `--out` with no value silently disables the write; `--out YML` swallows the format token and writes JSON to a file named 'YML' |
| output-canon | OC-15 | minor | document | `abstract compile <dir>` fails with 'missing input path' when the directory is named json, yml, yaml, raw, true or false |
| output-canon | OC-16 | minor | error | The emitted `id` is neither guaranteed to be a string nor unique: an int-typed id coerces, so @id.007 and @id.7 both emit id: 7 |
| output-canon | OC-17 | design | undecided | Optional list fields materialize as [] while optional scalars, refs and groups are omitted entirely - undocumented and inconsistent |
| output-canon | OC-18 | minor | document | The lang -> lang_values rename is not applied to tuple-array columns or #tag(...) arguments, producing a self-contradictory error |
| output-canon | OC-19 | minor | undecided | UTF-8 BOM: a BOM in a .abt silently creates a schema whose name starts with U+FEFF, and the error blames the instance file |
| output-canon | OC-20 | minor | error | A zero-column tuple array `field(): ()` is accepted and silently produces an array containing one empty object |
| output-canon | OC-21 | minor | undecided | A schema field written as `caps[]: { ... }` silently becomes a field literally named 'caps[]:' that no instance can ever satisfy |
| output-canon | OC-22 | minor | error | file() values accept ../ traversal outside the project and absolute machine paths, and the raw string is published into the compiled data |
| output-canon | OC-23 | minor | document | Directory symlink/junction cycles are traversed without cycle detection and surface as a bogus 'Duplicate schema' error |
| output-canon | OC-24 | design | error | Numeric literal typing flips at the i64 boundary: 9223372036854775807 is rejected by a text field, 9223372036854775808 is accepted |
| output-canon | OC-25 | design | document | Huge and tiny floats expand to 300+ digit decimal literals, and JSON output size is quadratic in nesting depth |
| output-canon | OC-26 | minor | undecided | docs/raw-data.md's JSON example and CLI section do not match the emitter (inline arrays, missing RAW/.abraw, wrong write condition) |
| output-canon | OC-27 | minor | undecided | Output-format tests are substring assertions only: no parser validation, no JSON/YAML equivalence, no key-safety, empty-project or empty-object coverage |
| revision-7 | R7-01 | major | error | Seven `for` blocks nested over a literal list of ten cross the logic work limit of SPEC 3.7 and are E523 |
| revision-7 | R7-02 | minor | document | Five of the same `for` blocks demand 111110 iterations, stay under the work limit and compile |
| revision-7 | R7-03 | minor | document | A project whose sources declare schemas and logic but no instance compiles to an empty `data` array |
| revision-7 | R7-04 | minor | document | Blank lines and comment lines may sit between a closing brace and its `else`, and between a `require` and its `else throw` |
| schema-types | abs-skip | minor | error | --skip-assets does not skip on-disk checks for absolute paths, contradicting the CLI help and both docs |
| schema-types | bad-default | minor | error | `abstract lint` reports 'ok' for a schema whose own defaults violate its own types — schema defaults are only validated when an instance happens to omit the field |
| schema-types | bom | major | document | A UTF-8 BOM makes an entire .abt file invisible: schemas silently vanish and the error blames the instance |
| schema-types | deep-groups-3000 | minor | error | Deeply nested schema groups blow the stack: the process aborts with 'has overflowed its stack' instead of reporting an error |
| schema-types | deep-ref | minor | error | Deeply nested $(Ref) instance data blows the stack in the validator: process abort, no diagnostic |
| schema-types | dup-list-marker | minor | error | `name[][]` silently creates a field literally named `name[]` that no instance can address |
| schema-types | dup-nonascii | minor | error | Identifier normalisation is ASCII-only: `CAFÉ` and `café` are two distinct fields (while `NAME`/`name` collide), and a half-lowercased key leaks into the output |
| schema-types | dup-schema-same-file | minor | error | The duplicate-schema error carries no line numbers and names the same file twice when both definitions are in one file |
| schema-types | emoji-len | minor | error | text(...) length counts Unicode scalar values, so a single emoji can count as 2+ 'characters' |
| schema-types | enum-empty | minor | error | enum(...)/file(...)/image(...) silently drop empty and duplicate entries instead of flagging the typo |
| schema-types | enum-modifier-strip | minor | error | @optional/@tag stripping is not parenthesis-aware: a token inside enum(...) is deleted from the type and silently turns the field optional |
| schema-types | file-traversal | major | error | file()/image() paths are never confined to the project: `../../` escapes the project root, is existence-checked, and is copied verbatim into compiled output |
| schema-types | id-dup-msg | minor | error | Unknown-field errors list a duplicated `id` and advertise output names that never appear in the .abt |
| schema-types | image-cases | minor | document | A truncated image is reported as 'unrecognized image signature' even when the signature is present and correct |
| schema-types | jpeg-ff-dos | minor | error | probe_image's JPEG padding-skip loop is unbounded and reads one byte at a time: a 64 MB file of 0xFF costs 107 seconds |
| schema-types | junk-abt | minor | error | Unrecognised content in a .abt is silently discarded — misspelled `schema`, fields outside a block, one-line schemas and brace-less schemas all pass without a word |
| schema-types | lang-collision2 | minor | document | The undocumented `lang` -> `lang_values` output rename lets two distinct schema fields alias one output key; one field's value is silently dropped |
| schema-types | list-default-comma | minor | document | A comma-separated default on a list field produces a self-contradictory error: "received 'a, b', expected one of: a, b" |
| schema-types | multi-tag | minor | error | A group may declare several @tag fields; only the first is functional and the rest silently become ordinary required fields |
| schema-types | name-space | minor | error | A schema name containing spaces is accepted and listed by `abstract templates`, but no instance can ever reference it |
| schema-types | numbered-yaml | major | document | Numbered keys (the documented `slots { 1: ... 2: ... }` idiom) become integer keys in YAML output but string keys in JSON output |
| schema-types | proj-root | major | document | The documented direct form `abstract obj.ab Template.abt JSON` resolves assets against the .ab file's own directory, so file()/image() checks spuriously fail |
| schema-types | rev-range | minor | error | Unsatisfiable type constraints are accepted with no schema-authoring error: int(10..5), text(-5..-1), image(png 0x0) |
| schema-types | root-tag-id | major | error | @tag on a root-level schema field consumes the instance's id and then DELETES `id` from the compiled output |
| schema-types | schema-case | minor | document | Schema names and $(Ref) targets are case-sensitive while every other identifier is case-insensitive; `Thing` and `thing` coexist as different schemas |
| schema-types | star-member | major | error | Enum members ending in '*' are unreachable, and the two-pass validation re-expands them, duplicating list entries |
| schema-types | template-field | major | error | A schema field named `template` silently overwrites the compiled record's own `template` discriminator, destroying the schema identity in the output |
| schema-types | unc-path | major | error | file()/image() values are handed straight to the OS: a UNC path makes `abstract compile` open an outbound SMB connection (NTLM credential-leak vector) and --skip-assets does not stop it |
| schema-types | wildcard-empty2 | major | error | An enum-list wildcard that matches nothing silently expands to an empty array instead of erroring — a one-character typo silently deletes data |
| schema-types | yaml-key | major | error | YML output is not valid YAML when a schema field name starts with a YAML indicator character — the compiler emits it with exit 0 |
