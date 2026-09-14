# Automotive language tour

This is a complete Abstract project built around a fictional 2028 vehicle
catalogue. It connects brands, owners, models, cars, wheels, tires, seats,
powertrains and performance records, then compiles the same sources into a base
document and version overlays.

Everything is synthetic. `Northstar`, `Velora`, `Norwyn`, `Selvara`, the owner
names, VIN-like strings, registration marks and engineering figures do not
identify or measure a real person, company or vehicle. The studio image is
editorial demonstration art; its monochrome appearance is not evidence of the
configured `exterior_color` value.

## Compile it

From the repository root:

```sh
abstract lint examples/automotive
abstract compile examples/automotive JSON
abstract compile examples/automotive YML
abstract compile examples/automotive RAW
```

The checked-in results are [`expected.json`](expected.json),
[`expected.yml`](expected.yml), and [`expected.raw`](expected.raw). The test
`automotive_project_matches_json_yaml_and_raw_goldens` compiles all three and
compares the bytes.

Start with [`data/templates/40_cars.abt`](data/templates/40_cars.abt), then open
[`data/vehicles/aurora.ab`](data/vehicles/aurora.ab). Together they show typed
fields, reusable nested schemas, inline groups, keyed lists, tuple rows, enum
and file wildcards, interpolation, clone merging, version windows, validation
rules and loops. [`FEATURE-COVERAGE.md`](FEATURE-COVERAGE.md) maps the complete
language specification to sources and regression tests.

The asset checks are real. `aurora-estate.png` is 1536×1024 and is assigned to
an exact `image(png 1536x1024)` field. `engineering-preview.jpg` is a JPEG
fixture accepted by the bounded header probe. Text documents are checked by
`file(txt)`. No compile requires `--skip-assets`.

## See the version model

The project declares versions 1 through 3. Version 3 is the base document.
Earlier differences appear in `overlays`:

- version 1 removes the continental car and keeps legacy fields;
- version 2 adds the continental and sport-preview cars and uses the `beta`
  software channel;
- version 3 removes the preview car and legacy values and uses `stable`.

This is one project and one compiled document. The overlays are deterministic
version projections inside that document, not separately deployed files.

## Try useful failures

The six projects under [`negative-cases`](negative-cases) intentionally fail
and record the expected diagnostic code:

```sh
abstract compile examples/automotive/negative-cases/range JSON
abstract compile examples/automotive/negative-cases/enum JSON
abstract compile examples/automotive/negative-cases/ref JSON
abstract compile examples/automotive/negative-cases/missing-asset JSON
abstract compile examples/automotive/negative-cases/steering JSON
abstract compile examples/automotive/negative-cases/performance JSON
```

They demonstrate a value outside a numeric range (`E413`), an unknown enum
member (`E414`), a missing referenced instance (`E431`) and a missing asset
(`E421`). The last two trigger authored logic (`E515`) for a market/steering
mismatch and reversed minimum/maximum performance speeds. For hands-on editing,
make the same mistakes in the main project:

- change a wheel `pressure_kpa` to `401`;
- change `steering_position` to `center`;
- change `owner: alex_demo` to an undeclared id;
- change `hero` to a file that is not present;
- pair `market: uk_ireland` with `steering_position: left`;
- put `minimum_valid_speed_kmh` above `top_speed_kmh`.

Restore the value and lint again. The compiler reports source locations and
never emits a partial successful document.
