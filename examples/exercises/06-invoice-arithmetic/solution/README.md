# Solution

`logic Line` derives each line's cents total from quantity and unit price.
`logic Invoice` then derives the invoice total from the nested line totals.
The two lines are `300` and `120` cents, so the invoice total is `420` cents.

This exercise uses the arithmetic contract's `calc` boundary and keeps the
currency representation integral.
