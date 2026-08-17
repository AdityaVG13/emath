# CONTRACT — emath-calibration

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Semantic calibration: behavioral examples constrain candidate worlds.
- Delivers deterministic example partitions, finite-carrier operator-table fitting, a held-out challenge, semantic drift, and forward-only world versioning.
- Depends on: emath-term, emath-world-ir.

## Public types and semantics
- `PartitionedExamples` - examples keyed by content identity, split into construction / validation / held-out / adversarial partitions.
- `CalibrationExample` / `ExampleKind` - one example and its partition kind.
- `FittedTable` - finite-carrier operator table fitted over construction examples; supports `from_cells`, `get`, `cells`, `canonical`.
- `CalibrationRecord` - fitted table, held-out outcome, per-partition example records, deterministic version.
- `HeldOutChallenge` / `HeldOutResult` - held-out challenge and its outcome.
- `FitFailure` - typed failure of fitting.
- `SemanticDrift` - measured semantic difference between two fitted tables.
- `WorldVersion` - deterministic stamped world version (seed `VERSION_SEED`).
- (not exhaustive)

## Invariants
- No candidate is credited for a held-out challenge if it saw the challenged examples during construction.
- Versioning is forward-only: a world invalidated by future examples becomes a new version, never a silent redefinition.
- Partitions are deterministic, keyed by content identity.

## Error model
- Fitting returns typed `FitFailure` on failure; `calibrate` propagates it via `Result<CalibrationRecord, FitFailure>`.
- Partitioning and versioning emit no errors; no panics.

## Determinism class
- Deterministic: partitions keyed by content identity, fitting order, record ordering, and version stamps are deterministic; example and record forms are canonical.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- None on disk: no `tests/` directory and no `#[cfg(test)]` module in `src/`.

## No-claim boundaries
- A slice of the planned calibration surface, not the full calibration service.
- Fitted tables are fitted approximations over finite examples, not certified semantics.
- Held-out challenge is self-contained to the crate's partitions, not an independent audit.
