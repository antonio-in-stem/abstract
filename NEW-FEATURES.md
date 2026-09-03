# Abstract v0.2.0: Audit Results and New Features

This release is the result of a full audit of the v0.1 compiler. It fixes
every correctness bug found, extends the template-instance model, and adds
the distribution, Java, and security layers. Existing v0.1 projects compile
unchanged (verified against the complete Wardrobe & Stickers corpus, 41
instances) with one intentional improvement: nested `$(Schema)` objects are
now emitted in schema field order.

## 1. Bugs found by the audit (all fixed)

| # | Bug | Symptom before | Behavior now |
|---|-----|----------------|--------------|
| 1 | `::` inside values parsed as instance headers | `note: "a::b"` or `window: 12::30` silently started a new broken instance | Headers require a bare template name before a top-level `::`; values keep `::` |
| 2 | `//` stripped inside unquoted values | `link: https://example.com` lost everything after `https:` | Comments start only at line start or after whitespace |
| 3 | Multiple clones dropped | `&a.*` + `&b.*` kept only `b` | Clones deep-merge in order; later clones win per field |
| 4 | Quoted strings brace-expanded | `desc: "use {x}. ok"` was mangled into fake file paths | Quoted values are always literal |
| 5 | Tuples broke on `(` inside strings | `(en_us, "Hi (there)")` misparsed | All scanners are string-aware with escape handling |
| 6 | `\\` escaping was wrong | `"a\\"` confused every scanner | Proper escape-state scanning everywhere; `\n`, `\t`, `\r`, `\"`, `\\` decode and re-encode correctly |
| 7 | Unknown fields passed silently | A typo like `rarty:` produced garbage output plus a confusing "missing field" error | Strict mode rejects unknown fields with a "Did you mean 'rarity'?" suggestion (`--allow-unknown` opts out) |
| 8 | Duplicate ids/schemas silently overwrote | Last file won, data vanished | Compile errors naming both definition sites |
| 9 | `$id` clobbered `$identity` | Prefix collisions in interpolation | Longest variable name replaced first; `${name}` and `$$` (literal `$`) added |
| 10 | Multi-line arrays required trailing commas | `[` on its own line broke the statement | Statements continue while brackets are open |
| 11 | Braces/parens inside strings corrupted depth tracking | Rare parse corruption | All depth counters skip string content |
| 12 | Tuple arity was unchecked | `(en_us, Hello, extra)` silently mapped wrong columns | Arity mismatches are errors with line numbers |
| 13 | Parse errors had no line numbers | Hard to find the broken line | Parse-stage errors report `file.ab:line:` |

## 2. Type system extensions

### float and bool are first-class

```abstract
schema Item {
    price: float(0..9999) = 0.0
    opacity: float(0..1)
    tradable: bool = true
}
```

- `9.5` compiles to a real JSON/YAML number, `true` to a real boolean.
  v0.1 emitted them as strings.
- Version-like strings stay text: `1.21.5` is not a float.
- Bare types work without constraints: `text`, `int`, `float`, `bool`.
- Header flags: `Item :: @id.x, @featured` sets `featured: true`.

### image(...): native, memory-optimal asset validation

```abstract
schema Sticker {
    icon: image(png 128x128)
    hero: image(png 1920x1080, jpg 1920x1080)
    any_size: image(png)
    wide: image(png 1024x*)
}
```

The compiler opens each image and reads only the header bytes (26 bytes for
a PNG) to verify, without ever decoding pixels:

1. the extension is allowed,
2. the file exists,
3. the real content matches the extension (a GIF renamed to `.png` fails),
4. the dimensions match the declared alternatives (`*` = any).

Formats: PNG, JPG, GIF, BMP, WebP (lossy, lossless, extended).

## 3. Template-instance extensions

### Partial clones

```abstract
&hero.stats          // copy only the stats subtree from hero

Thing :: @id.sidekick
    stats.agility: 40    // then override one field
```

`&hero.*` remains the full clone; multiple clone lines now merge in order.

### Enum wildcards in plain lists

```abstract
flags: hat_*         // expands against the enum vocabulary
```

(Previously wildcards worked only inside tagged tuple arrays.)

## 4. Logic extensions

```abstract
logic Card {
    if .kind == "big" {
        derive .size = large
    } else if .kind == "medium" {
        derive .size = mid
    } else {
        derive .size = small
    }

    require not .flags contains "banned"
        else throw "Banned cards are not allowed."

    derive? .wave = 1        // only when the instance did not set it

    for $n in [3] {
        derive .slot_count = $n              // keeps the int type
        derive .banner = "pack $id, $n slots" // string interpolation
    }
}
```

- `else` and `else if` chains.
- `not ...` and `!(...)` negation.
- `derive?` writes only when the field is missing (authored values win).
- Derive values interpolate `$loop` variables and root `$fields`; a value
  that is exactly one variable keeps its native type.
- Numeric comparisons now understand floats.

## 5. CLI

```text
abstract init <dir>                          scaffold a working project
abstract compile <path> JSON|YML|RAW         RAW is now a CLI format
abstract compile <path> JSON --out file      explicit output path
abstract lint <path>
abstract templates <path>
abstract bundle <path> --key <k> [--plain]   sealed .abx bundles
abstract unbundle <file.abx> --key <k>       inspect a bundle
--skip-assets                                compile without binary assets on disk
--allow-unknown                              legacy lenient mode
--version / --help
```

`--skip-assets` exists because converting or CI-checking data on a machine
that does not have the textures checked out is a real workflow.

## 6. Sealed bundles + Java runtime (new)

- `abstract bundle` seals compiled JSON with ChaCha20-Poly1305 (RFC 8439)
  into a `.abx` container. Zero dependencies; SHA-256 key derivation from a
  passphrase or exact `hex:` keys; the header is authenticated, so flag
  tampering and truncation fail loudly; encrypted-to-plain downgrade attacks
  are refused.
- `java/` contains a zero-dependency Java 8+ runtime:
  `AbstractBundle.loadResource(...)` -> `AbstractData` -> typed
  `AbstractObject` access (`getString("slots.1.mode")`, `getObjects`,
  fallbacks). Verified against the RFC vectors and against real bundles
  produced by the CLI (cross-language test, including UTF-8 content).
- `AbstractKeys.split/combine` implements XOR key sharing so the key never
  exists as one constant in your jar; `proguard-rules.pro` and `SECURITY.md`
  document the full hardening checklist and the honest threat model.

## 7. Distribution

- `scripts/install.ps1` (Windows: cargo build or `-Prebuilt exe`, adds PATH).
- `scripts/install.sh` (Linux/macOS into `~/.local/bin`).
- Refreshed prebuilt binary in `site/downloads/`.
- `abstract init` scaffolds a project that compiles out of the box and
  demonstrates float/bool/image/derive?.

## 8. Output changes to be aware of

1. Nested `$(Schema)` objects are emitted in schema field order (previously
   the tag field landed last). Key-based consumers (JSON/YAML parsers) are
   unaffected; byte-for-byte diff tools will see the reorder once.
2. `true`/`false` and decimal literals are now native booleans/numbers in
   JSON and YAML. If a consumer relied on them being strings, quote the
   value in the instance (`"true"`) or adjust the consumer.
3. Strict unknown-field validation may reject files that previously
   "worked" by silently carrying typos; `--allow-unknown` restores the old
   behavior while you clean up.

## 9. Test coverage

`cargo test` runs 66 tests: 49 compiler behavior tests (including one per
audit fix above), image probe tests for all five formats, and the official
RFC 8439 / FIPS 180-4 vectors for the crypto. The Java side mirrors the
vectors in JUnit plus a dependency-free `Selftest` runner, and the full
Wardrobe & Stickers corpus is the cross-language integration fixture.
