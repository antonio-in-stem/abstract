# Exercise 06: total an invoice

An invoice has lines. Each line has an integer quantity, an integer unit price
in cents, and a derived line total in cents. The invoice total is the sum of
its line totals. Keep the arithmetic exact by using cents, not floating point.

Add derive rules to the starter's `Line` and `Invoice` schemas. The starter
intentionally leaves the derived values unfinished. The
solution is in `solution/`.

Check yourself: the supplied lines must total `300` and `120` cents, and the
invoice must total `420`. Change the first quantity to `3`: the invoice must
become `570`, without editing a stored total.

<details>
<summary>Hint</summary>

`calc(...)` marks arithmetic expressions. A nested `$(Line)` value runs
`logic Line`, so the invoice can sum `.lines.total_cents` after each line has
its own total.

</details>
