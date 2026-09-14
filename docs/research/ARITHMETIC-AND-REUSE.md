# Arithmetic, reuse and variants

## Product decision

Abstract 1.2 adds explicit numeric expressions through `calc(...)` in logic.
Instances remain compact data. A string such as `2 + 3` remains text; a field
name such as `unit-price` remains an identifier. Only a glued lowercase
`calc(` in a logic expression opts into calculation. An existing literal
`calc(...)` at that position must now be quoted, as with `length(...)`.

Numeric operations belong to compilation, not to runtime actions. There is no
clock, network access, random source, dynamic evaluation or user-defined code.
Expression limits and existing loop limits bound the work accepted per object.
Checked failures are preferable to silently wrapping integers or emitting NaN.

Integer money examples use minor units (such as cents). Binary floating-point
arithmetic is not decimal money arithmetic. This release does not introduce a
decimal type or pretend that rounding repairs every representation issue.

## Reuse without another declaration language

`$(Contact)` embeds the declared Contact shape. `logic Contact` is evaluated for
each such nested object. Shared structure and its shared validation therefore
already travel together. A separate named-predicate system could help rules
that cross unrelated shapes, but current examples do not justify its added
binding and parameter rules. No second reuse mechanism is introduced here.

## Variants are different from cloning

`&source.*` copies authored data from a same-schema instance. It answers how an
instance obtains initial values, not which combinations of fields are legal.

A discriminator enum, optional fields and conditional `require` rules can
already express a small closed family: an electric powertrain must supply its
battery fields and omit its fuel fields, while gasoline requires the converse.
The learning exercise demonstrates this with the existing language. Dedicated
union syntax is deferred until actual models show that this approach becomes
repetitive or produces poor diagnostics. This is not a claim that optional
fields plus rules provide all the static narrowing of a first-class sum type.

CUE's [disjunctions of structs](https://cuelang.org/docs/tour/types/sumstruct/)
provide a useful primary reference for the alternative design: selecting among
allowed shapes is a type constraint, not a data-copy operation. Abstract keeps
its existing explicit schema/instance distinction.

## Teaching location

The canonical learning path lives with the official documentation, in
`docs/learn/`, and its executable projects live in `examples/exercises/`.
Each problem has a small starter, expected behavior, optional hints and a
separate explained solution. The public site can publish the same material;
the author's portfolio can link to selected exercises without maintaining a
second version of the language manual. No external site is published by this
change.

## Numeric implementation references

- Rust [checked integer operations](https://doc.rust-lang.org/std/primitive.i64.html)
  provide explicit overflow results.
- Rust [floating-point operations](https://doc.rust-lang.org/std/primitive.f64.html)
  document rounding and precision. Arbitrary real-exponent `powf` and other
  functions with platform-dependent precision are outside this release.

Implementation tests and measurements establish behavior on the tested host;
they are not a proof of universal business-rule coverage or a cross-platform
performance guarantee.
