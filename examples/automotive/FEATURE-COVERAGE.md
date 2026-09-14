# Abstract 1.1 feature coverage

This matrix treats [`docs/SPEC.md`](../../docs/SPEC.md) as normative. The
automotive project is the readable positive integration corpus; focused
compiler and conformance tests cover rejection rules, limits and syntax that
cannot appear in a successful project. `cargo test` runs both layers.

| SPEC | Feature | Positive source or golden | Rejection / focused test |
| --- | --- | --- | --- |
| 2.1–2.6 | `.abt`/`.ab`, UTF-8, project `data/` and `assets/`, discovery, case rules | this project layout | `tests/conformance` areas `cli-project`, `lexing-parsing` |
| 2.7 | deterministic instance order | [`expected.json`](expected.json) | `tests/compiler_tests.rs` output-order tests |
| 3.1–3.6 | tokens, quoted/bare values, comments, identifiers, logical continuation | all templates; multiline tuple/list in [`aurora.ab`](data/vehicles/aurora.ab) | `tests/conformance/cases/lexing-parsing` |
| 3.7 | nesting and work limits | ordinary nested groups and loops | adversarial conformance cases `ADV-02`–`ADV-06`, `ADV3-11`–`ADV3-16` |
| 4.1–4.3 | template structure, schemas, fields and modifiers | every file under [`data/templates`](data/templates) | schema parser unit tests and `schema-types` conformance area |
| 4.3 `@public` | author-nominated public fields | Brand/Owner/VehicleModel names | `tests/public_modifier.rs`, `tests/public_contract.rs`, `tests/public_inventory_cli.rs` |
| 4.4.1 | `text`, exact and ranged lengths | VINs, labels, names | `negative-cases/range`; compiler Unicode range tests |
| 4.4.2 | `int`, disjoint/exact ranges | years, pressure, `wheel_count: int(4)` | [`negative-cases/range`](negative-cases/range) (`E413`) |
| 4.4.3 | `float` | wheel widths, battery and performance values | compiler type/range tests |
| 4.4.4 | `bool` | defaults, header flag `@active`, bare tag flags | compiler type tests |
| 4.4.5 | `enum` | markets, trims, materials, feature codes | [`negative-cases/enum`](negative-cases/enum) (`E414`) |
| 4.4.6 | `file` and extensions | `documents`, `specification` | missing/extension conformance tests |
| 4.4.7 | `image`, format and exact dimensions | `hero_image`, PNG gallery, JPEG preview | `schema-types/image-cases`, media conformance tests |
| 4.4.8 | `ref(Schema)` | car→model/owner and model→brand | [`negative-cases/ref`](negative-cases/ref) (`E431`) |
| 4.4.9 | `$(Schema)` nested objects | powertrain, performance, wheels, tires, reference seat | nested-schema compiler tests |
| 4.5 | integer/float ranges | component and performance schemas | range conformance cases |
| 4.6 | list cardinality | exactly four wheels/tires, two-to-five seats | cardinality (`E445`) compiler tests |
| 4.7 | groups and list groups | `contact`, `features`, `wheel_checks`, `labels`, seats | group-depth and presence tests |
| 4.8 | `@tag` keyed lists | seats and features with `#tag` values | duplicate/misplaced tag conformance cases |
| 4.9 | numbered keys | `labels.1`, `labels.2` | `docs/examples/numbered-keys` and YAML tests |
| 4.10 | `@optional`, defaults, required presence | contact, ventilation, connectivity, many defaults | presence/default conformance cases |
| 4.11 | explicit and implicit `id`; reserved `template` | every root schema declares constrained `id` | `schema-types/template-field`, envelope assignment tests |
| 4.12 | `versions`, field `@since`/`@removed` | versions 1..3, connectivity and legacy fields | versions unit and conformance tests |
| 5.1–5.3 | headers, value/bare header tags, explicit identity | `@id`, `@trim.value`, bare `@active` | header/id conformance cases |
| 5.4 | paths, multi-paths, blocks | powertrain blocks, `wheel_checks.{…}`, dotted tag args | path/duplicate-assignment tests |
| 5.5 | scalar/list values, `#tag`, tuple arrays | component tuples, seats/features tag objects | malformed list/tag/tuple conformance cases |
| 5.6 | enum wildcards | `accent_colors: accent_*` | wildcard-empty conformance cases |
| 5.7 | full and partial clones, merge | `&aurora_demo.*`, `&aurora_demo`, `&aurora_continental.performance` | clone cycle/depth/order tests |
| 5.8 | brace file patterns | gallery PNGs and document TXT files | brace-pattern conformance cases |
| 5.9 | asset paths and containment | all files under [`assets`](assets) | [`negative-cases/missing-asset`](negative-cases/missing-asset), traversal tests |
| 5.10 | type-directed values | same tuple spellings become text/int/float/enum by target type | compiler coercion/type tests |
| 5.11 | `$name`, `${name}`, `$$` interpolation | label and display-label derivation | interpolation unit/conformance tests |
| 5.12 | strict declared paths | every assignment is schema-backed | unknown-field/suggestion conformance tests |
| 5.13 | statement version windows | `software_channel` has disjoint version assignments | version-statement tests |
| 5.14 | instance version windows | continental and sport-preview cars | instance-window tests |
| 6.1–6.4 | logic binding, `derive`, `derive?`, `require`, branches | Car and VehicleModel logic blocks | logic binding/derive conformance cases |
| 6.5 | root, nested, indexed, projected and dynamic paths | indexed first wheel, projected feature codes, `$feature.code`, `.$corner` | logic path conformance cases |
| 6.6 | precedence, comparisons, boolean conditions | numeric/equality/`contains`, `not`, `and`, branch conditions | precedence and arity unit tests |
| 6.7 | `exists` | steering-position guards | absence/presence tests |
| 6.8 | exact strings and numeric ordering | market equality, steering relation and performance min/max checks | `negative-cases/steering`, `negative-cases/performance`, comparison conformance tests |
| 6.9 | lexical loop variables | `$feature`, `$corner`, `$mode` | semantic binding test and loop scope tests |
| 6.10 | `length()` and `version` | counts, VIN length, schema version | function-kind tests |
| 6.11–6.13 | evaluation order, determinism and derive write rules | defaults → logic → required counts in all versions | logic-engine and output-canon conformance areas |
| 7.1–7.6 | compilation phases, versions, base reduction and overlays | all three golden outputs | versions/output/compiler integration tests |
| 8.1–8.9 | envelope, key order, JSON/YAML/RAW, number/string rendering, omission | [`expected.json`](expected.json), [`expected.yml`](expected.yml), [`expected.raw`](expected.raw) | `automotive_project_matches_json_yaml_and_raw_goldens`; output-canon corpus |
| 9.1–9.8 | CLI commands/flags/exits/diagnostics | README commands | `tests/cli_tests.rs`; negative automotive test |
| 9.9, Appendix D | optional analysis, bundle and runtime tools | outside the language corpus | analysis/public tests, media-bundle conformance, `java/` tests |
| 10 | diagnostic catalogue | six readable negative projects | all error goldens under `tests/conformance` |
| 11 | conformance rules and golden shape | automotive byte goldens | 301-case conformance runner and `tests/conformance/INDEX.md` |

The 21 ignored legacy tests are retained historical 1.0 expectations. They are
marked obsolete rather than counted as unfinished 1.1 behavior; the active
conformance corpus and the automotive integration tests are the current gates.
