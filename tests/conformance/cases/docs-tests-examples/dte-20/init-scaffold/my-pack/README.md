# Abstract Project

- `pack.ab` describes the pack itself.
- `data/templates/` holds the schemas (`.abt`).
- `data/items/` holds instances (`.ab`).
- `assets/` holds binary files referenced by `file(...)` and `image(...)` fields.

Compile everything:

```
abstract compile . JSON
```

Validate without output:

```
abstract lint .
```
