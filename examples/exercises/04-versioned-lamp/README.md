# Exercise 04: evolve a lamp

One lamp exists in project versions 1 and 2. Its name always exists. Version 1
stores an old `tint`; version 2 adds a `glow` flag. Write one schema and one
instance so the compiled document has the current base plus the earlier
version in `overlays`.

Start with `starter/data/templates/Lamp.abt` and
`starter/data/lamps/lamps.ab`. Add the version declarations and field windows,
then compile the solution to see the exact envelope. The solution is in
`solution/`.

Check yourself: version 2 has `glow: false` and no `old_tint`; version 1 has
`old_tint: 180` and no `glow`. The lamp keeps the same id in both versions.

<details>
<summary>Hint</summary>

`versions 1..2` defines the project range. `@since(2)` means a field is
present from version 2; `@removed(2)` means it is present before version 2.
The envelope's `data` is the base (the newest version); earlier differences
are whole-object entries in `overlays`.

</details>
