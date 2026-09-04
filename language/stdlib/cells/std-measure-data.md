# `std.measure`; data and measurement types (cell contract)

Status: std-layer data package (Phase 12), implemented in
`crates/emath-core/src/measure.rs`. Extends
the Phase 2 `Measured<T>` family: provenance is a FIELD,
not metadata, and participates in identity.

## The measurement triple

`Measurement { value, std_uncertainty, distribution, unit, provenance,
authority }`; the quantity + uncertainty + provenance triple. The
uncertainty is data: `add_independent` propagates it in quadrature,
`weighted_mean` uses inverse-variance weights, and neither ever hides
it in a bare float.

## Provenance (closed, 21ul taxonomy)

`Exact{basis}` / `Citation{source, adjustment}` / `InstrumentRun{file,
processing}` / `Fitted{fit_id}` / `Assumed` / `Unstated`. The canonical
encoding carries the discriminant and every field; dataset identity
(FNV-1a64) hashes values as bit patterns PLUS provenance PLUS
authority; the 21ul law that provenance keys participate in identity.

## Authority lattice (observational honesty)

`Unstated < Structural < Certified`:

- **The CSV adapter ALWAYS imports at `Structural`**; observational
  data is structural, whatever provenance the caller asserts (even an
  `Exact` argument stays structural on import). The adapter has no
  path to `Certified`.
- `degrade(to)` moves explicitly DOWN the lattice; there is no promote.
- Arithmetic and `weighted_mean` take the lattice MEET: one assumed
  input contaminates the derived result downward.

## CSV adapter

`parse_csv_dataset(text, name, provenance)`: header row of `name` or
`name (unit)` cells (units resolved through the core unit table), data
cells parsed as finite f64, rows validated against the header width.
Typed refusals, never silent repair:

- ragged row: `E-MEASURE-1` (names the file line and expected width)
- non-numeric / non-finite cell: `E-MEASURE-2` (names file line,
  column, and the raw cell text)
- empty import (no header): `E-MEASURE-3`
- unknown unit: `E-MEASURE-4` (via the core unit table)

Affine units (degC, degF) refuse in arithmetic: affine points are not
addable quantities.

## `series_from_csv`; pure-text CSV time-series import

`series_from_csv(text, time_column, value_column, interpolation,
extrapolation)` projects a CSV block into an executable `Series` (the
same value type as a literal series; sample it with `series_at`). Every
argument is a string literal; the import is **pure text**; it reads the
CSV string only, with **no filesystem, network, or CSV crate** I/O.

Signature (all strings, the first is the CSV text):

```
series_from_csv(csv, time_column, value_column, interpolation, extrapolation)
```

Accepted CSV grammar (closed, deterministic):

- BOM (`\u{FEFF}`) tolerated on the header line or its own line; CRLF
  (`\r\n`) tolerated; blank lines skipped.
- RFC-minimal quoting: a double-quoted field may contain a comma; a
  literal `"` inside a quoted field is escaped as `""`. An embedded
  newline inside a quoted field is **not** supported (no-claim).

Column mapping:

- A column is requested by **bare name** (e.g. `"time"`) or by its
  **full header name with unit suffix** (e.g. `"time (s)"`); the trailing
  `(unit)` suffix is stripped for matching.
- Physical column order is irrelevant; unrequested columns are ignored
  (projected away). A request that matches **exactly one** column wins;
  ambiguity refuses (never guesses). Duplicate unrequested columns are
  irrelevant.

Policy and identity:

- Interpolation policies: `previous`, `linear`, `nearest`, `pwc`,
  `monotone_cubic`.
- Extrapolation policies: `refuse` (the default), `clamp`, `extend`.
  With `refuse`, sampling outside the support is a typed
  `SeriesOutOfSupport` fault.
- The declared policy hashes into the series **identity**: two series
  differing only in policy are different values. Determinism class:
  identical CSV text + identical args → bit-identical points and meaning.

Typed refusals (data-class), one per defect, never silent repair:

- `E-CSV-001` time column missing
- `E-CSV-002` time column ambiguous
- `E-CSV-003` value column missing
- `E-CSV-004` value column ambiguous
- `E-CSV-005` ragged row (cell count ≠ header width)
- `E-CSV-006` unclosed/malformed double-quote (header or data row)
- `E-CSV-007` no data rows (header only)
- `E-CSV-008` selected cell not a finite number
- `E-CSV-009` time axis nonincreasing (names the offending row/time)

Arg-shape or policy misuse (non-literal arg, wrong arg count, unknown
policy name) refuses as `E-SERIES-CSV`.

See `language/examples/science/wind-series-csv.emath` for the runnable
example that exercises unit-suffixed + full/bare-name mapping, column
reordering, quoted-comma and escaped-quote fields, unused columns, and
the `linear`/`refuse` and `clamp` policies.

## No-claim boundaries

- NetCDF/HDF adapters and the installable package surface are follow-up
  slices (this is the core data layer + CSV import).
- Distribution tags are recorded, not yet propagated (Normal/Uniform/
  Lognormal distinction is metadata on the triple).
- `series_from_csv` is the pure-text bridge above; the parser/sema
  `Measured` literal path (21ul/) remains separate.
- Correlated-uncertainty propagation (covariances) is out of scope;
  `add_independent` is exactly what its name says.
