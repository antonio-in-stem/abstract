# Exercise 05: model a powertrain variant

Model one `Powertrain` shape with two allowed variants. `kind` is either
`electric` or `gasoline`. Electric powertrains must have `battery` and must
not have `fuel`; gasoline powertrains must have `fuel` and must not have
`battery`. Each nested property is optional so the logic decides which shape
is required.

Complete the starter's missing logic, then fix its deliberate
invalid gasoline record. The solution is in `solution/`.

Check yourself: an electric powertrain without a battery fails; one with both
battery and fuel fails too. Gasoline requires fuel and rejects a battery.

<details>
<summary>Hint</summary>

use two optional groups and `if`/`else`. A named variant schema is a
validated shape; it does not copy authored data. Cloning is a separate feature
that copies authored values and does not replace these rules.

</details>
