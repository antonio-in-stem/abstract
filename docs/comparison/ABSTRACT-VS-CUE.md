# Abstract 1.4.0 and CUE v0.17.1: executable comparison

This is a narrow, reproducible comparison of two data-language toolchains. It
does not rank either language in general. The fixture models one small invoice
catalog with the same enum vocabulary, nested required fields, inclusive
numeric and text bounds, calculated line totals, calculated invoice total, and
a maximum-total business rule.

## Reproduce it

From the repository root on Windows:

```powershell
./examples/comparison/invoice-catalog/verify.ps1
```

The checked run used:

- `target/release/abstract.exe`, reporting `abstract 1.4.0`, SHA-256
  `11526f4960ef156011eebd8dc4406775f0e60580c251c7a939c2a0e3bbc949b9`;
- the official `cue_v0.17.1_windows_amd64.zip`, whose GitHub release metadata
  reports SHA-256
  `9f15378dc52b9a1bb6fa1755adc0410c9f17f330a621b784a5b31fdbc91c6d7a`;
- the extracted `cue.exe`, reporting CUE language and CLI version `v0.17.1`,
  built with Go 1.26.5 for Windows amd64, SHA-256
  `a2452301a1abeaec8b46d4ce13b5ad5875851d447bcaeda43c556418b9b57e08`.

The comparison was tested with that compiler binary and the fixture shipped in
this release. The CUE executable is a test dependency, not part of an Abstract
release.

To obtain and verify the same official CUE artifact on Windows:

```powershell
$cueZip = 'cue_v0.17.1_windows_amd64.zip'
Invoke-WebRequest `
  -Uri 'https://github.com/cue-lang/cue/releases/download/v0.17.1/cue_v0.17.1_windows_amd64.zip' `
  -OutFile $cueZip
$expected = '9f15378dc52b9a1bb6fa1755adc0410c9f17f330a621b784a5b31fdbc91c6d7a'
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $cueZip).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "CUE archive SHA-256 mismatch: $actual" }
Expand-Archive -LiteralPath $cueZip -DestinationPath tools/cue-v0.17.1
```

Place `cue.exe` on `PATH`, or pass its location explicitly:

```powershell
./examples/comparison/invoice-catalog/verify.ps1 `
  -CueExe tools/cue-v0.17.1/cue.exe
```

The verifier runs these effective commands, using a temporary Abstract project
that contains the shared schema and exactly one case:

```powershell
abstract compile <temporary-valid-project> JSON
abstract lint <temporary-invalid-project>
cue export cue/schema.cue cue/cases/valid.cue -e invoice
cue vet -c cue/schema.cue cue/cases/<invalid>.cue
```

## Equivalent source

Abstract separates the schema and logic from its compact author data:

```abstract
schema Line {
    sku: text(1..20)
    kind: enum(service, goods)
    quantity: int(1..99)
    unit_price_cents: int(1..100000)
    total_cents: int(1..9900000)
}

logic Line {
    derive .total_cents = calc(.quantity * .unit_price_cents)
}

logic Invoice {
    derive .total_cents = calc(sum(.lines.total_cents))
    require .total_cents <= 500000
        else throw "Invoice total exceeds the 500000-cent approval limit."
}
```

Its instance uses built-in identity, dotted nested paths, and a tuple list:

```abstract
Invoice :: @id.inv_1001, @status.issued, @currency.mxn
    number: INV-1001
    customer.name: Acme Studio
    customer.address.city: Mexico City
    customer.address.country: mx
    lines(sku, kind, quantity, unit_price_cents): (CONSULT, service, 2, 15000), (LICENSE, goods, 3, 2500)
```

CUE expresses the same constraints through definitions and unification:

```cue
#Line: {
	sku!:              #Text1To20
	kind!:             "service" | "goods"
	quantity!:         int & >=1 & <=99
	unit_price_cents!: int & >=1 & <=100000
	total_cents:       quantity * unit_price_cents
}

#Invoice: {
	lines!: [...#Line] & list.MinItems(1) & list.MaxItems(8)
	total_cents: list.Sum([for line in lines {
		line.total_cents
	}])
	total_cents: <=500000
}
```

The complete executable sources are under
[`examples/comparison/invoice-catalog`](../../examples/comparison/invoice-catalog/README.md).
The snippets above omit unchanged fields for readability.

## Measured result

The valid case was accepted by both CLIs. After removing Abstract's built-in
`template` discriminator and recursively sorting object keys, both produced the
same JSON value as
[`expected.normalized.json`](../../examples/comparison/invoice-catalog/expected.normalized.json).
Normalization retains `id`, all domain fields, derived fields, values, array
order, and number types. It removes no CUE fields.

Both tools rejected all four negative cases with nonzero exit status:

| Requirement | Abstract 1.4.0 | CUE v0.17.1 |
|---|---|---|
| status enum | E414, enum mismatch | empty disjunction at `invoice.status` |
| nested city required | E411, missing required field | required field not present |
| quantity at least 1 | E413, range mismatch | value out of bound `>=1` |
| total at most 500000 | E515 with the authored rule message | value out of bound `<=500000` |

The checked verifier output was:

```text
PASS valid: Abstract and CUE equal expected.normalized.json
PASS invalid-enum: both tools reject the intended constraint
PASS invalid-nested: both tools reject the intended constraint
PASS invalid-bound: both tools reject the intended constraint
PASS invalid-rule: both tools reject the intended constraint
PASS all comparison checks (Abstract 1.4.0, CUE v0.17.1)
```

## What the example shows

Abstract gives authors a distinct, compact instance syntax and emits its own
canonical document envelope. Named nested schemas run their logic before the
parent, so the invoice rule can sum derived line totals directly. Its authored
`require ... else throw` supplies a domain-specific failure message. Beyond
this fixture, Abstract has first-class instance identity, cloning, asset/image
checks, and versioned overlays in the compiler.

CUE uses one constraint model for schemas and values. Definitions, bounds,
disjunctions, comprehensions, and standard-library functions compose by
unification, independent of file order. The same CLI can combine CUE with
JSON, YAML and other supported inputs, export several data encodings, and work
with CUE modules. These are broader composition and interoperability facilities
than this Abstract fixture attempts to reproduce.

The syntactic size is not a controlled usability measurement. Abstract's tuple
form is denser for repeated records; the CUE instance makes every object field
visible. Conversely, CUE's arithmetic is part of its general expression and
constraint system, while Abstract deliberately confines arithmetic to
`calc(...)` in logic. Which is preferable depends on whether compact governed
authoring or general constraint composition is the primary job.

## Sources and limits

Primary CUE sources consulted on 2026-09-14:

- [CUE v0.17.1 official GitHub release](https://github.com/cue-lang/cue/releases/tag/v0.17.1)
- [CUE language specification](https://cuelang.org/docs/reference/spec/)
- [The logic of CUE](https://cuelang.org/docs/concept/the-logic-of-cue/)
- [Types are values](https://cuelang.org/docs/tour/basics/types-are-values/)
- [`cue export` reference](https://cuelang.org/docs/reference/command/cue-help-export/)
- [Validating YAML using CUE](https://cuelang.org/docs/howto/validate-yaml-using-cue/)
- [String rune-length constraints](https://cuelang.org/docs/howto/constrain-the-length-of-a-string/)
- [List cardinality constraints](https://cuelang.org/docs/howto/use-list-maxitems-list-minitems-to-constrain-list-length/)
- [Constraining a numeric-list sum](https://cuelang.org/docs/howto/constrain-the-sum-of-a-list-of-numbers/)

Abstract behavior is grounded in this repository's normative
[`SPEC.md`](../SPEC.md), [`ARITHMETIC.md`](../ARITHMETIC.md), and the executed
1.4.0 compiler.

This test covers one Windows amd64 release of each implementation and five
small fixtures. It does not measure performance, editor quality, ecosystem
size, learning curve, large-project behavior, language completeness, or
security. Equal normalized JSON demonstrates equivalence for this fixture only.
The CUE archive digest comes from GitHub's official release-asset metadata; it
was checked after download. No external human usability study or independent
audit was performed.
