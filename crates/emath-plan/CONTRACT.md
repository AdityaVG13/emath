# emath-plan CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Deterministic resolution planning (Phase 1 bootstrap plus Phase 6 planner machinery).
- Hosts the provider-facing planner surface: the resolution algebra, decomposition rules, representation planning, fallback graphs, provider lifting, total dispositions, inspection, and plan identity/cache.
- The canonical native plan constructor (`native_plan`) lives in `emath-ir`, which owns `ResolutionPlan` and plan-node types.
- Depends on `emath-artifact`, `emath-core`, `emath-goal`, `emath-ir`, `emath-provider-api`, `emath-runtime`.

## Public types and semantics

- `plan` / `PlannerConfig` / `PlanningOutcome` / `TieBreak`; the deterministic planner over a goal and provider registry, producing a total `PlanningOutcome`.
- `PlanningOutcome`; `Selected { plan, inspection }`, `NoEligible { reasons, disposition, inspection }`, or `Exhausted { continuation, disposition, inspection }`.
- `PlannerConfig`; bounded candidate retention (`max_candidates`), node budget (`max_nodes`), tie-break rule, and policy name that binds to plan identity.
- `ArtifactDisposition` / `disposition_for_plan` / `disposition_without_plan` / `disposition_exhausted`; total disposition machinery.
- `DecompositionRule` / `SubgoalDag` / `SubgoalNode` / `decompose` / `requirements_preserved`; decomposition and requirement preservation.
- `FallbackGraph` / `FallbackNode`, `Conversion` / `ConversionNode` / `RepresentationError` / `find_conversion_path`, `ProviderTraitSpec` / `LiftedMethod` / `emit_provider_trait` / `lift_missing`, `plan_identity` / `PlanCache` / `ProviderFingerprint` / `provider_set_fingerprint`, `PlanInspection` (not exhaustive).
- `algebra`: `Facet` (five capability facets mirroring `E-PROV-512`..`E-PROV-516`), `QState` (residual resolution question over those facets), `Step` (capability as a partial transformation; `Id`, `Serial`, `Parallel`, `Alt`, `Fallback`, `Portfolio`), `Application`, `Lifted`, and the `serial`/`parallel`/`fallback` helpers. `Step::apply` is partial; `Step::apply_total` lifts to a total application whose failure is an explicit refusal with retained reasons. Candidate selection in `plan` is expressed through `Step::Alt` over capability steps.
- `PlanInspection::explain`; deterministic human-readable plan explanation (selected candidate, tie-break order, every exclusion with stable code, checks, budget, disposition).
- `PlanInspection::to_json`; schema `emath.plan-explanation v1` (CLI `emath explain --json` / `emath planner --json`).

## Invariants

- Candidate ordering and tie-breaks are deterministic (`CostAscendingId` or `IdLexicographic`); every exclusion is retained with its reason.
- No eligible candidate yields `NoEligible` with `E-GOAL-201: no eligible plan`; budget exhaustion yields an `Exhausted` continuation or diagnostic per the goal's fallback policy.
- Planner policy name binds to plan identity.

## Error model

- No `emath_core::Diagnostics`; planning is a total function returning `PlanningOutcome` variants carrying stable reasons (`E-GOAL-201`, `E-RES-100`).
- `RepresentationError` is a dedicated error type in `representations` for conversion-path finding; it carries stable codes (`E-PROV-515` / `E-PROV-517`).

## Determinism class

- Deterministic resolution planning: ordered rules, bounded candidate retention, explicit pruning, and deterministic tie-breaks; `plan` is deterministic over its inputs.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-plan/tests/` directory on disk; all conformance lives in the standalone `tests/emath-plan` package:
  - `tests/planner.rs`: `node_budget_refuses_oversized_plan_dag`, `adding_providers_or_budget_preserves_the_artifact_class`, `capability_matrix_admits_supported_and_refuses_unsupported`.
  - `tests/planner_logic.rs`: candidate-exhaustion bounds (`more_than_max_compatible_candidates_is_exhausted`, `one_compatible_plan_in_large_registry_is_not_exhausted`), `excluded_trace_reports_real_exclusions`, exactness produce polarity and lossy-path handling (`serves_kind_requires_exact_produce`, `exact_goal_keeps_searching_past_lossy_hit`, `exact_goal_refuses_when_every_path_is_lossy`, `estimate_goal_accepts_first_lossy_path`).
  - `tests/algebra.rs`: law tests (serial associativity, identity neutrality, alternative left-bias, parallel state commutativity, lifting totality, fallback degradation marking).

## No-claim boundaries

- No external providers are installed in Phase 1; planning covers native dispositions only.
- Does not own or construct the canonical `ResolutionPlan`; that resides in `emath-ir`.
