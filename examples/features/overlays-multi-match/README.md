# overlays-multi-match

The one project in this directory where a **single version is covered by more
than one overlay**, and where an id reaches a consumer through an overlay
having never appeared in `data`. `versions-overlays` and `instance-windows`
each produce overlays with disjoint ranges, so neither exercises the two
assumptions SPEC Appendix D.2 item 11 forbids a runtime to make.

## What each source contributes

| Source | Window | Entry (SPEC §7.5 step 1) |
|---|---|---|
| `data/items/torch.ab` | every version | one **replace** over `1..2`: its version 1 and version 2 objects are identical, and both differ from the base |
| `data/items/ember.ab` | every version | two **replace** entries, `1..1` and `2..2`: `tier` exists only in version 1 and `legacy_tint` only in versions 1 and 2 |
| `data/items/beacon.ab` | `@since(3)` | one **remove** over `1..2`: the base carries it and no earlier version does |
| `data/items/lantern.ab` | `@removed(3)` | one **replace** over `1..2`: it is in no base document, so an overlay is the only place it appears |

Grouping those five entries by range (step 2) gives three overlays, `1..1`,
`1..2` and `2..2`. Version 1 is therefore covered by two of them and version 2
by two others, while version 3 is covered by none and uses `data` unchanged.

Resolving each version by step 3 gives:

| Version | Instances, in `(template, id)` order |
|---|---|
| 1 | `ember` (with `tier`), `lantern`, `torch` |
| 2 | `ember` (without `tier`), `lantern`, `torch` |
| 3 | `beacon`, `ember`, `torch` |

A consumer that stops at the first matching overlay gets versions 1 and 2
wrong; one that replaces only ids it already holds loses `lantern`.

`assets/textures/item.png` is a 16×16 RGBA PNG, the size `image(png 16x16)`
declares. Assets are not versioned (SPEC §4.12), so `icon` holds the same value
in every version.

The Java runtime's `AbstractData.forVersion(int)` is tested against this
document, in `java/src/test` and `java/src/selftest`.
