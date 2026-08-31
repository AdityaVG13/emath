# emath-goal CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Hosts the goal schema (full goal kinds plus a custom-goal envelope) and the capability surface for the intent-compiler lane.
- The goal-elaboration kernel moved down to `emath-ir::goal` (`RequestSpec`, `build_goal`) and into `emath-sema` (`elaborate_requests`, its only consumer).
- Depends on `emath-core` (hashing, ids) and `emath-ir` (goal, policy, and profile types).

## Public types and semantics

- `GoalKindSpec`; goal kind incl. the `Custom { schema, fields }` envelope; stable `name()`.
- `GoalSchema`; full schema: kind, inputs/outputs, accuracy, evidence, budget, target, determinism, fallback, produce; with `validate`, `canonical`, `determinism_token`, `identity`, `from_goal`.
- `BudgetConstraint`; optional compile/runtime work limits plus a unit.
- `GoalSchemaProblem`; schema validation problem with a stable code and message.
- Token helpers: `exactness_token`, `budget_token`, `target_token`, `fallback_token`, `custom_token` (not exhaustive).

## Invariants

- A schema validates itself; every validation problem carries a stable code (`E-GOAL-011` to `E-GOAL-013`).
- Outputs must be non-empty; duplicate output names are rejected; a budget limit without a work unit is rejected.
- `canonical` produces the stable `goal:...` encoding, and `identity` derives an FNV-1a64 `ContentId` from it.
- `custom_token` sorts custom-envelope fields deterministically.

## Error model

- Defines its own error type `GoalSchemaProblem` with stable codes (`E-GOAL-011`, `E-GOAL-012`, `E-GOAL-013`); `validate` returns a `Vec<GoalSchemaProblem>`. No `emath_core::Diagnostics`.

## Determinism class

- Canonical encoding and identity are deterministic and byte-comparable; `custom_token` ordering is sorted.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-goal/tests/` directory and no `#[cfg(test)]` module in the crate on disk.

## No-claim boundaries

- Does not elaborate requests; elaboration lives in `emath-sema` (`elaborate_requests`), using the `emath-ir` goal kernel.
- Schema self-validation covers the enumerated `E-GOAL-011` to `E-GOAL-013` conditions only.
