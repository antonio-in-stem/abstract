# Arithmetic in Abstract 1.2

This document is normative and extends chapter 6 of [SPEC.md](SPEC.md).
Arithmetic is explicit: `calc(...)` is accepted as a complete `derive` or
`derive?` right-hand side, and as an operand in `require` and `if` conditions.

```abstract
logic Line {
    derive .total_cents = calc(.quantity * .unit_price_cents)
    require calc(.total_cents / 100) <= 500 else throw "Line exceeds limit."
}
```

Only the adjacent lowercase spelling `calc(` selects this grammar. An ordinary
instance value such as `note: 2 + 3` remains text. An existing unquoted literal
`calc(...)` in one of these logic positions MUST be quoted when migrating from
1.1. `versions 1..2` continues to select data versions, not language versions.

## Expressions

Numeric literals, scalar numeric paths, numeric loop variables, `version` and
function calls are operands. Parentheses group numeric expressions. Unary `+`
and `-` bind tightest, then `*`, `/`, `%`, then binary `+` and `-`. Binary
operators at the same level associate left to right. A calculation is numeric,
not a string concatenation or a boolean expression.

Identifiers retain their hyphens: `.unit-price` is one field. Write subtraction
with spaces, as `.price - .discount`. Use spaced operators consistently for
readability.

The result retains its numeric type and MUST be assignable to the destination
field. It is not converted to authored text and reparsed. Numeric paths must
resolve to one value except when consumed by an aggregate function; absent
scalar or incorrectly shaped inputs are errors, not silently substituted zeroes.
An absent optional numeric list or an empty projection supplies an empty
collection to an aggregate; it does not create a field in the output.

## Numeric behavior

- `int` uses signed 64-bit integers. Integer `+`, `-`, `*`, unary negation and
  integer functions check overflow. They MUST NOT wrap or saturate.
- `float` uses finite binary64 values. Mixing integers into a floating-point
  operation requires an exact integer-to-binary64 conversion; otherwise the
  calculation fails rather than silently rounding the integer.
- `/` performs floating-point division, including for two integers. Use
  `div(a, b)` for an integer quotient truncated toward zero.
- `%` is integer remainder, with the dividend's sign. Zero divisors and the
  overflowing signed-minimum divided by minus one are errors.
- Any operation producing a non-finite result fails. Binary64 is not a decimal
  money type: represent exact minor currency units as integers.
- Evaluation is ordered and uses no random source, clock, environment or I/O.

## Functions inside `calc`

| Function | Meaning |
|---|---|
| `abs(x)` | Absolute value; integer overflow is an error. |
| `min(a, b, ...)`, `max(a, b, ...)` | Minimum/maximum of at least two scalar numeric arguments, or one nonempty numeric list/projection. |
| `clamp(x, low, high)` | Restrict a value to inclusive ordered bounds. Reversed bounds fail. |
| `div(a, b)` | Checked integer division, truncated toward zero. |
| `round(x)` | Integer rounding; halfway values round away from zero. |
| `floor(x)`, `ceil(x)` | Checked conversion to the integer below/above. |
| `sqrt(x)` | Floating-point square root; negative arguments fail. |
| `pow(base, exponent)` | Integer exponent only. Integer bases require nonnegative exponents and retain checked integer arithmetic. Use a float base for negative exponents. |
| `sum(path)` | Sum a declared numeric list or numeric projection. |
| `avg(path)` | Floating-point average of a nonempty numeric list or projection. |
| `length(path)` | Existing list/text/projection length semantics; returns an integer. |

`round`, `floor` and `ceil` leave integer arguments unchanged. Their floating
results must fit signed 64-bit range. `sum` of an empty integer list is `0`;
an empty float list yields `0.0`. `avg` of an empty collection is an error.
Numeric functions accept no implicit text, boolean or object conversions.
`min` and `max` also reject empty collections. Aggregates visit values in source
order. `avg` performs the checked sum first, then divides by the count; an
overflowing intermediate sum fails even if the mathematical mean would fit.

To total products, derive the product in a named element schema and then sum
its projection in the containing schema. Nested named-schema logic already
runs before its parent's logic; no list-comprehension syntax is necessary:

```abstract
logic Invoice {
    derive .total_cents = calc(sum(.lines.total_cents))
}
```

## Validation and tooling

The compiler checks function names, arities, declared operand types and field
references before evaluating instances. Division by zero, overflow, invalid
domains and empty averages fail explicitly during evaluation when their values
become known. Existing validation still checks derived field ranges.

Arithmetic field reads participate in semantic references and rename, and in
public-contract dependency checks. Hiding a field read inside `calc` does not
make a dependent field independently exportable.

An expression has at most 256 AST nodes and at most 64 nested unary/group/call
levels. The absolute exponent passed to `pow` must not exceed 1,000,000.
Arithmetic nodes, traversed aggregate elements and exponentiation steps share
the existing 1,000,000-unit per-instance/version logic-work budget with loops.
Nested named-schema evaluation shares that budget as well.

Malformed calculations and invalid static types/arity report `E524`:
`Invalid arithmetic expression: {reason}.` Evaluation failures report `E525`:
`Arithmetic evaluation failed: {reason}.` Existing lexical bracket and
general resource-limit errors may occur earlier. These identifiers extend
the chapter 10 logic diagnostic catalogue.

The accompanying implementation validation record states regression results
and measured performance.
