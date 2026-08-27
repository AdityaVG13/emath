# CONTRACT.md - emath-adapter-rumoca

## Purpose and layer

Tier 3 (goals and providers) Modelica-subset import adapter. Phase 1
implements the structural lane with in-tree native stand-ins: structural/
equation IR, compiler-phase census, dynamic-model subset contract,
emath-to-DAE lowering, DAE-plan and simulation providers, semantic mapping
table, subset import, MSL conformance ladder, versioned provider seam, and
diagnostic mapping. No upstream Rumoca engine is consumed in Phase 1.

## Public types and semantics

- `StructuralModel`, `Component`, `ComponentKind`, `VariableDecl`,
  `VariableKind`, `Equation`, `EqExpr`, `Event`, `InitialCondition`,
  `Connection`, `Dimensions`, `Unit`, `UnitError`, `ModelIssue`: neutral
  structural/equation IR.
- `PhaseRecord`, `PhaseKind`, `Stability`: compiler-phase census.
- `DaePlan`, `DerivativeDef`, `EqProvenance`, `LowerError`: emath-to-DAE
  lowering.
- `SimulationArtifact`, `DerivativeMetadata`, `build_simulation_artifact`:
  bounded flattening into byte-deterministic safe Rust source plus a runnable
  in-process component over the same DAE plan.
- `SimPoint`, `SimulationConfig`, `SimulationResult`, `SimError`:
  DAE-plan and simulation providers (`provide_dae_plan`, `simulate`).
- `ConstructMapping`, `MappingClass`: semantic mapping table.
- `ConformanceReport`, `FeatureResult`, `FeatureStatus`, `Tier`: MSL
  conformance ladder.
- `MappedDiagnostic`, `ProviderDiagnostic`: diagnostic mapping.
- `ForeignModelDeclaration`, `ImportError`, `SubsetFeature`, `SubsetIssue`,
  `AdapterSeam`, `ProviderVersion`, `SeamError` (not exhaustive).

## Invariants

- No upstream Rumoca type appears; Rumoca is referenced only by provider
  identity string.
- Import refuses constructs outside the documented Modelica subset
  (`E-KIND-310..312`), never approximates.
- Lowering refuses unsupported semantics instead of approximating
  (`E-PROV-220..223`).
- Simulation provider refuses plans outside supported states
  (`E-PROV-230..238`).
- The runnable artifact profile accepts exactly two scalar states and no
  algebraic outputs. Unsupported Rust identifiers, non-causal derivative
  references, and wider state/output shapes refuse under `E-PROV-239`.
- Census records per-phase stability; the engine is not consumed per phase.

## Error model

Stable codes: `E-PROV-210` (structural), `E-PROV-220..223` (lower),
`E-PROV-230..238` (provider/simulation), `E-PROV-240/241` (import),
`E-PROV-239` (artifact profile), `E-PROV-401/402` (seam),
`E-KIND-310..312` (subset). Provider diagnostics
map into emath diagnostics via `MappedDiagnostic`/`ProviderDiagnostic`.

## Determinism class

The native structural/equation IR, census, subset contract, lowering, map
table, and per-config simulation are deterministic. Provider posture is
declared per phase in the census.

## Cancellation behavior

Not applicable. Std-only synchronous adapter, no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

No `tests/` directory on disk in the crate and no `#[cfg(test)]` module in
`src/`. Workspace integration suites live in `tests/emath-adapter-rumoca`:
`tests/provider.rs` (simulation/plan refusal families `E-PROV-230..239`,
deterministic mass-spring artifact, physical differential check, and
hostile-budget refusal) and `tests/census.rs` (phase and stability).

## No-claim boundaries

Phase 1 consumes no upstream Rumoca engine; the structural lane is a native
stand-in. Import covers only the documented Modelica subset
(`E-KIND-310..312`); the MSL conformance ladder assesses the subset, not
full MSL conformance.
