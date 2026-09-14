# Product catalogue

A collection of products with shared fields, variants, capabilities, release
policy and catalogue metadata. The project lives here independently of the
other examples.

From the repository root:

```sh
abstract lint examples/product-catalog
abstract compile examples/product-catalog JSON
```

Read the definitions in `data/templates/`, then the concrete records in
`data/catalog/` and `data/meta/`. Change a value and compile again to see how the
schema constrains the result.
