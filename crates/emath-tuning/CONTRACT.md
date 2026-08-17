# CONTRACT — emath-tuning

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Semantic and joint tuning: semantic tuning varies carriers, operators, constants, laws, or valuations while protecting declared laws and held-out examples; joint tuning varies the world and implementation together.
- Promotion requires semantic admission, evidence threshold, resource envelope, fallback availability, and a deterministic receipt.
- Depends on: emath-term, emath-world-ir.

## Public types and semantics
- `SemanticVariable` / `SemanticVariableKind` - one tunable semantic variable and which part of a world it varies (Carrier, Operator, Constant, Law, Valuation).
- `SemanticChange` - one concrete world change proposed by a candidate, with provenance.
- `WorldDelta` - base world plus deterministically sorted semantic changes.
- `ExecutionDelta` - implementation delta: lowering, precision, provider, target, schedule.
- `JointCandidate` - world delta plus execution delta with deterministic identity (`held_out_verified`, `evidence_units`, `identity`).
- `HostCampaign` - runs a campaign over candidates and their measurements; `run` returns a `CampaignReceipt`.
- `ResourceEnvelope` / `HostMetric` / `HostObjectives` - protected resource envelope, one host measurement, and protected-metric objectives.
- `CandidateMeasurement` / `PromotionChecklist` / `CandidateDecision` / `CampaignReceipt` - per-candidate measurement, promotion gates, decision, and deterministic campaign receipt.
- (not exhaustive)

## Invariants
- Promotion requires all five gates: semantic admission, evidence threshold, resource envelope, fallback availability, and a deterministic receipt.
- A world that merely memorizes construction examples must not promote as a general meaning; every candidate is tested against held-out references before selection.
- The resource envelope admits fail-closed on missing cost or latency measurements.
- Selection among promoted candidates is deterministic; ties break toward the lower identity.
- When no candidate promotes, the campaign rejects and selects the fallback, and the receipt records that.

## Error model
- No error type: `run` returns a `CampaignReceipt` (not a `Result`); rejection of candidates is expressed through decisions, not errors.
- No panics.

## Determinism class
- Deterministic: candidate identity is FNV-1a64 over canonical form, promoted selection has a fixed tie-break (lower identity), and campaign receipts carry deterministic canonical forms.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- Inline `#[cfg(test)]` in `src/campaign.rs`: `admits_fails_closed_on_missing_cost_or_latency`, `admits_at_the_bounds_with_all_three_measured`.
- No `tests/` directory on disk.

## No-claim boundaries
- A slice of the planned tuning surface, not the full tuning service.
- The `held_out_verified` flag is a per-candidate claim fed into selection, not independent certification.
- Promotion policy here is structural over the declared gates; host objectives and envelopes are the caller's responsibility.
