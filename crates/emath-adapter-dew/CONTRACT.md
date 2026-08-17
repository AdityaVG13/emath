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
- `DifferentialFinding`, `MutantDrift`, `ScanCase`, `ScanProfile`: reference-
  boundary scan and semantic-drift detection (`run_boundary_cases`,
  `detect_drift`).
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

No `tests/` directory on disk. Inline `mod tests` exists in `src/capability.rs`
(`E-PROV-031` backend inventory), `src/backends.rs` (`E-PROV-030` recusals and
Rust/token rendering), and `src/oracle.rs` (boundary and drift detection).

## No-claim boundaries

Phase 1 has no cross-engine differential lane (no upstream engine is
consumed). The accelerator inventory is a capability classification, not a
runtime; JIT selection is advisory with fallback.
