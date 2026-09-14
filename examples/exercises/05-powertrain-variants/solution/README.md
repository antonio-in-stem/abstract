# Solution

`Powertrain` is a reusable constrained variant. Its two groups are optional
at the schema level, then `logic Powertrain` requires exactly the group that
matches `kind`. The vehicle instances embed the same named schema, so the
powertrain logic runs inside both vehicles.

The gasoline record supplies `fuel.tank_litres: 50`; the electric record
supplies only its battery. This is a small variant pattern built from existing
schemas and logic, rather than new variant syntax.
