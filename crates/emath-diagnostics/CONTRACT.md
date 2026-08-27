# CONTRACT — emath-diagnostics

## Purpose and layer
- Pedagogic explanations and rendered witnesses for finite-checker refusals.
- Schema `emath.diagnostic.explanation v1`.
- `tutor-check/v1` rejects synthesized narrative that is not backed by a checker receipt.

## Public types
- `Explanation`, `ExplainKind`, `RenderedWitness`, `TableExcerpt`, `DocLink`, `RenderFormat`
- `tutor_check_v1`
- `explain_law_report` / `check_and_explain` / `every_failure_has_witness` / `e_law_001_demo`

Plan explanations (`emath.plan-explanation v1`) live on `emath_plan::PlanInspection` (the planner pipeline crate), not a separate empty `emath-pipeline` crate.

## Invariants
- A `LawFalsified` explanation without a `RenderedWitness` and receipt id is rejected.
- Witness cells come from the checker table, never invented numbers.
- Authority is not raised by explaining a refusal.

## No-claim
- Explanations do not prove the law. They show the finite counterexample the checker found.
