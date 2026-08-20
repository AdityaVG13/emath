# CONTRACT.md - emath-adapter-dew

## Purpose and layer

Tier 3 (goals and providers) Dew scalar backend adapter. Std-only native
lanes for scalar mapping, backend selection, Rust/token rendering, JIT
capability with fallback, accelerator inventory, source map, and boundary/
drift oracle. Reuses Dew expression/code-generation machinery through a
versioned seam without consuming an upstream engine.

## Public types and semantics

- `DewExpr`, `Shape`, `Layout`, `CmpOp`, `LinearOp`, `MappingIssue`:
  exact scalar mapping and explicit linear-algebra mapping with shape/layout
  conversion; `map_expression`, `map_linear` refuse unsupported nodes.
- `Backend`, `DewCapability`, `NoClaimBoundary`, `OptimizationEvidence`:
  machine capability descriptor, no-claim boundary, and optimization-
  evidence classification; `provide_capability`, `select_backend`.
- `AcceleratorTarget`, `BackendSelection`, `DeviceTransferPlan`, `JitCapability`,
  `JitTarget`, `RustFragment`, `TokenStream`: backends, JIT with fallback,
  and the accelerator inventory (WGSL/GLSL/CUDA/HIP/OpenCL).
- `SourceMapEntry`: SIR -> Dew -> generated symbol/span source map with
  deterministic anchors (`build_source_map`).
- `DifferentialFinding`, `MutantDrift`, `ScanCase`, `ScanProfile`,
  `EvalValue`: reference-boundary scan and semantic-drift detection
  (`run_boundary_cases`, `detect_drift`, `evaluate_scalar`,
  `detect_seeded_wrong_result`). `evaluate_scalar` is the Dew-adapter
  evaluation path (bit-exact IEEE-754 binary64 for arithmetic, same
  class as native exec-ir). `detect_seeded_wrong_result` is a Phase 3
  planted-value control, not a differentiate producer.
- `AdapterSeam`, `PatchLedger`, `PatchOutcome`, `ProviderVersion`, `SeamError`:
  versioned adapter-facing API with a patch ledger.
- (not exhaustive).

## Invariants

- No upstream type appears; Dew is referenced only by provider identity
  string.
- Unsupported emath nodes are refused before Dew execution, never
  approximated.
- The adapter seam is versioned with a patch ledger; version drift is a
  typed refusal.
- Source map anchors and rendering are deterministic.

## Error model

Stable codes: `E-PROV-001` (seam version drift), `E-PROV-002` (uncategorized
patches), `E-PROV-030` (unsupported/refused before Dew execution),
`E-PROV-031` (target outside capability inventory), `E-PROV-033` (shape
mismatch).

## Determinism class

dexpr mapping, source map, rendering, backend selection, and the oracle
boundary scan and drift detection are documented deterministic. The JIT
capability provides a deterministic fallback path.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`. The `JitCapability` is a
declared capability/fallback, not a use of unsafe.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

No `tests/` directory on disk in this crate. Integration coverage lives in
`tests/emath-adapter-dew`: capability inventory, backend recusals,
oracle boundary/drift, `native_and_dew_agree_on_scalar_corpus` (Phase 2
native exec-ir ↔ Dew `evaluate_scalar` bit-exact agreement over the
`tests/valid` scalar corpus plus adapter fixtures), and
`seeded_wrong_derivative_result_is_refused` (Phase 3 planted-value
control via `detect_seeded_wrong_result`).

## No-claim boundaries

Phase 1 consumes no upstream Dew engine. The Phase 2 differential lane
compares the in-tree native exec-ir interpreter against the Dew adapter
evaluator, not an upstream Dew runtime. The accelerator inventory is a
capability classification, not a runtime; JIT selection is advisory
with fallback. `detect_seeded_wrong_result` is not a derivative engine.
