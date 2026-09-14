# Exercise 02: ship a package

Describe a package that has a name, a shipping state (`queued`, `packed` or
`sent`), and a weight from 1 through 25. If the author omits the state, it must
be `queued`.

Read the contract in `starter/data/templates/Package.abt`, then fix the two
invalid values in `starter/data/packages/boxes.ab` without weakening the
schema. The solution is in `solution/`.

Check yourself: weights `1` and `25` work, `30` fails, `waiting` fails, and
an omitted state becomes `queued`.

<details>
<summary>Hint</summary>

an enum lists every accepted spelling. An integer range is inclusive.

</details>
