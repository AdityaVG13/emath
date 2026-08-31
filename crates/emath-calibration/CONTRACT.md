# CONTRACT — emath-calibration

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Semantic calibration: behavioral examples constrain candidate worlds.
- Also hosts the generic fit-goal runtime (04 §5.3, emath-r3-fit-goal-4xjh): parameters, observable, residual/optimizer methods, weights, data rows, provenance, and identifiability.
- Delivers deterministic example partitions, finite-carrier operator-table fitting, a held-out challenge, semantic drift, and forward-only world versioning.
- Depends on: emath-term, emath-world-ir, emath-ir.

## Public types and semantics
- `PartitionedExamples` - examples keyed by content identity, split into construction / validation / held-out / adversarial partitions.
- `CalibrationExample` / `ExampleKind` - one example and its partition kind.
- `FittedTable` - finite-carrier operator table fitted over construction examples; supports `from_cells`, `get`, `cells`, `canonical`.
- `FitGoal` / `FitRow` / `ResidualWeights` - generic fit-goal data (04 §5.3): parameters, observable, model path, prediction label, residual method, optimizer method, initial seeds, weights, and declared data rows. `FitGoal::from_payload` traces an elaborated fit payload (`emath_ir::goal::GoalPayload`) into the runtime goal; malformed or unknown spellings are typed `FitPayloadError` refusals (never silent defaults). Data rows pair one coordinate row with the observable row (`data: t = [...], data: <observable> = [...]`); uniform row weight 1.0, per-parameter weighting via `weights`. No domain model is bound here; the PK model is a runnable `.emath` fixture (`language/examples/science/pk-two-compartment-fit.emath`).
- `FitModel` / `IdentifiabilityProvider` - model and structural-identifiability provider seams; `fit` / `weighted_residuals` / `jacobian_residuals` execute them generically. The provider receives the model, data, and fitted parameters.
- `NumericRankOracle` - honest executable structural-identifiability provider: local column rank of the residual Jacobian at the fitted point (Jacobi eigenvalues of `J^T J`, relative tolerance), covariance-based confidence intervals (normal-95 approximation), tight directions only when the interval does not straddle zero; rank deficiency and underdetermined data refuse authority.
- `materialize_measured` / `FitMeasuredError` - materializes fitted values as `emath_ir::provenance::Measured<f64>` with linked `Provenance::Fitted { fit_id }` (16-hex content hash). With a verdict each declared direction must appear (typed refusal otherwise); std_uncertainty is `(hi - lo) / 3.92`. Without a verdict std_uncertainty is 0.0 as the explicit *unclaimed* marker — never a claim of zero error.
- `FitOutcome` / `AuthorityEscalation` / `UnresolvedReason` / `ProvenanceHash` - honest dispositions: fitted provenance (with the per-direction confidence verdict when granted), refusal naming an unidentifiable direction, or a typed unresolved disposition when no structural-identifiability provider exists (`SymbolicOracleUnavailable`). Fitting never silently claims authority. `FitOutcome::Fitted.confidence` is `None` when no identifiability was claimed.
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
- `tests/emath-calibration/tests/fit_goal_runtime.rs` (standalone package built from `tests/emath-calibration`): payload tracing, executable-program fit, typed refusals, weighting, escalation, rank oracle, materialization, determinism.

## No-claim boundaries
- A slice of the planned calibration surface, not the full calibration service.
- Fitted tables are fitted approximations over finite examples, not certified semantics.
- Held-out challenge is self-contained to the crate's partitions, not an independent audit.
- The `NumericRankOracle` is a LOCAL numeric rank oracle: it certifies identifiability at the fitted point for the supplied data, not global structural identifiability of the model form.
- `materialize_measured` without a confidence verdict reports std_uncertainty 0.0 as an explicit unclaimed marker; the fit itself never certifies a zero error.
