# Invoice/catalog comparison fixture

This fixture expresses one invoice in Abstract 1.2 and CUE, then runs the real
Abstract 1.4.0 and CUE v0.17.1 CLIs. Both models contain the same enum choices,
nested customer/address shape, text and numeric bounds, line-item arithmetic,
and 500000-cent approval limit.

Run from the Abstract repository root on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File examples/comparison/invoice-catalog/verify.ps1
```

The script expects the repository's `target/release/abstract.exe` and resolves
`cue` from `PATH`. Override either location to test explicit release artifacts:

```powershell
./examples/comparison/invoice-catalog/verify.ps1 `
  -AbstractExe path/to/abstract.exe `
  -CueExe path/to/cue.exe
```

The files are split into one schema and independent cases. The script stages
one Abstract case at a time in a temporary project because Abstract compiles
every `.ab` source discovered under a project. CUE receives its schema and one
case explicitly on each invocation.

The valid run removes only Abstract's built-in `template` field, recursively
sorts object keys, and compares both values with
[`expected.normalized.json`](expected.normalized.json). It retains the invoice
`id`; the CUE model declares that field explicitly. Arrays retain their order.

The negative cases isolate four shared requirements:

| Case | Intended failure |
|---|---|
| `invalid-enum` | `cancelled` is outside the status vocabulary |
| `invalid-nested` | `customer.address.city` is required |
| `invalid-bound` | quantity `0` is below the inclusive lower bound |
| `invalid-rule` | computed total `600000` exceeds the approval limit |

See [`docs/comparison/ABSTRACT-VS-CUE.md`](../../../docs/comparison/ABSTRACT-VS-CUE.md)
for the evidence, commands, and interpretation.
