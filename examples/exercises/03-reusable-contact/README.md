# Exercise 03: reuse a contact

Both a `Person` and a `Vendor` need the same contact shape: an email and a
city. The email must contain `@`, no matter which object contains the contact.

The starter embeds `Contact` with `$(Contact)` in both schemas. Complete it
by writing one `logic Contact` rule. The starter intentionally
has an invalid vendor email. The solution is in `solution/`.

Check yourself: removing `@` from either person's or vendor's contact must
fail with the same rule. This toy rule checks for one character; it is not
a complete email-address validator.

<details>
<summary>Hint</summary>

named nested schemas run their own logic wherever they are embedded. Do
not copy the rule into `logic Person` and `logic Vendor`.

</details>
