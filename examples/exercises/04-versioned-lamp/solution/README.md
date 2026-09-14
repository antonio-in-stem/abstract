# Solution

The schema declares the project range and scopes the two changing fields:
`glow` starts at version 2 and `old_tint` is removed at version 2. The authored
instance supplies the old tint once. Compilation evaluates every version and
emits the newest object in `data`; the version 1 object appears in `overlays`.

The base and overlay are compiler output. They are not separate authored
documents or separate objects to deploy.
