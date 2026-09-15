# Conformance corpus

The executable corpus lives under `tests/conformance/cases/<area>/<id>/`. Run
all active cases with:

```sh
cargo test --test conformance
```

Each case contains a `case.toml` and a small project tree. A successful compile
can pin `expected.json`, `expected.yml` or `expected.abraw`. A rejection case
lists ordered diagnostic identifiers in `expected-error.txt`. Cases that only
need to compile use `expect = "ok"`.

The runner invokes the compiler binary from each case directory. It normalizes
line endings and the compiler version before comparing golden output. Cases
that use `--out` run against a temporary copy, so the checked-in corpus is not
modified.

Cases with `status = "pending"` are listed but skipped by default. Set
`ABSTRACT_CONFORMANCE_RUN_PENDING=1` to include them while implementing a
pending requirement.

The area directories group coverage for parsing, schema types, instances,
logic, output, command-line behavior, bundles, documentation examples and
resource limits. The case files are the maintained record of inputs and
expected behavior; Git history retains earlier investigation reports.
