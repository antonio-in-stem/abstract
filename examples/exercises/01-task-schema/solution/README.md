# Solution

Priority is `1..3`, so the starter's `0` is outside the schema contract. The
solution uses `priority: 2` and omits `state`; the compiler fills the declared
default `todo`.

Run `abstract compile examples/exercises/01-task-schema/solution JSON` and
compare it with `expected.json`.
