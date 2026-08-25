# CONTRACT — emath-tuning

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Semantic and joint tuning: semantic tuning varies World IR components (carriers, symbols, signature, operator meanings, constants, constructors, laws, effects/capabilities) while protecting declared laws and held-out examples; joint tuning varies the world and implementation together.
- Promotion requires semantic admission, evidence threshold, resource envelope, fallback availability, and a deterministic receipt.
- Depends on: emath-term, emath-world-ir.

## Public types and semantics
- `SemanticVariable` / `SemanticVariableKind` - one tunable semantic variable and which World IR component it varies. Eight kinds, `Ord` by declaration order: Carrier, Symbol, Signature, Operator, Constant, Constructor, Law, Effect. Canonical names (`carrier` through `effect`) round-trip through `from_canonical`.
- `SemanticChange` - one concrete world change proposed by a candidate, with provenance. `replace` builds an operational patch whose `description` encodes prior then next, separated by `PATCH_SEPARATOR` (`U+001F`).
- `encode_patch` / `PATCH_SEPARATOR` - reversible payload encoding used by `WorldDelta::apply` / `WorldDelta::revert`.
- `WorldDelta` - base world plus deterministically sorted semantic changes. `locality()` is the sorted unique World IR component names the change set touches; `receipt()` builds a `DeltaReceipt`. `apply(&self, base: &WorldIr) -> Result<WorldIr, DeltaError>` writes each change onto a clone of `base`; `revert(&self, applied: &WorldIr) -> Result<WorldIr, DeltaError>` is the inverse, so `revert(apply(base))` restores `base`'s canonical form and `WorldId`.
- `DeltaError` - typed apply/revert refusal (`BaseMismatch`, `MissingTarget`, `PriorMismatch`, `MalformedPatch`, `NotReversible`, `IdentityUnchanged`, `DidNotRestore`). A delta whose target is absent from the world is `MissingTarget`, never a silent no-op.
- `DeltaReceipt` - binds a base world fingerprint (`u64`), the sorted applied-change canonical strings, and a deterministic receipt identity (FNV-1a64 over `canonical()`). Same input always yields the same identity. `locality()` recovers touched components from those strings.
- `ExecutionDelta` - implementation delta: lowering, precision, provider, target, schedule.
- `JointCandidate` - world delta plus execution delta with deterministic identity (`held_out_verified`, `evidence_units`, `identity`).
- `CoverageSample` / `CalibratedConfidence` / `calibrate_confidence` - recalibrates meaning confidence from construction vs held-out coverage; unused table capacity is a complexity penalty; a memorizing candidate (`held_out` below `MIN_HELD_OUT_PERMILLE`) is refused.
- `HostCampaign` - runs a campaign over candidates and their measurements; `run` returns a `CampaignReceipt`.
- `ResourceEnvelope` / `HostMetric` / `HostObjectives` - protected resource envelope, one host measurement, and protected-metric objectives.
- `CandidateMeasurement` / `PromotionChecklist` / `CandidateDecision` / `CampaignReceipt` - per-candidate measurement, promotion gates, decision, and deterministic campaign receipt.
- `frontier` module - frontier engine (aggressive generation, strict admission): `FRONTIER_SCHEMA` (`emath.frontier`) / `FRONTIER_VERSION` (1), `RewriteRule` (an algebraic rewrite hypothesis over one operator), `generate_algebraic_candidates` (deterministic: sorted by canonical form, deduplicated, budget-capped; every emitted candidate is unverified with zero evidence), and `verify_held_out` (runs the held-out challenge; evidence is awarded only on a pass, and the returned candidate has a fresh deterministic identity either way). Provenance of generated changes is `algebraic-rewrite`.
- (not exhaustive)

## Invariants
- Promotion requires all five gates: semantic admission, evidence threshold, resource envelope, fallback availability, and a deterministic receipt.
- A world that merely memorizes construction examples must not promote as a general meaning; every candidate is tested against held-out references before selection.
- The resource envelope admits fail-closed on missing cost or latency measurements.
- Selection among promoted candidates is deterministic; ties break toward the lower identity.
- When no candidate promotes, the campaign rejects and selects the fallback, and the receipt records that.
- A `DeltaReceipt` identity is a function of the base fingerprint and the sorted applied-change canonical strings only; presentation-only labels are not part of the preimage.
- Locality of a change set is the sorted unique World IR component names of its kinds (`carriers`, `symbols`, `signature`, `operators`, `constants`, `constructors`, `laws`, `effects`), so dependents re-check only affected obligations.
- `WorldDelta::apply` requires `base.identity() == base_world`, addresses all eight `SemanticVariableKind`s against existing World IR components, and a non-empty change set must produce a new `WorldId`. `revert` restores that original identity. Constructor/law/effect list entries may use the target `SymbolId` as the prior string when `PATCH_SEPARATOR` is absent; other kinds need an encoded prior to revert.
- Apply walks changes in canonical sort order; revert walks the reverse of that order. No hashing or iteration order leaks into the result.
- Frontier pipeline order is fixed: generate (unverified) → verify held-out → benchmark verified candidates only → campaign. The generator alone can never produce a promotable candidate, and an incorrect candidate is rejected at semantic admission before any benchmark exists for it.

## Error model
- `WorldDelta::apply` / `WorldDelta::revert` return `Result<WorldIr, DeltaError>`. Missing targets, base-identity mismatch, prior/next mismatch, malformed payloads, non-reversible descriptions, no-op identity, and a revert that does not restore `base_world` are typed refusals.
- `HostCampaign::run` returns a `CampaignReceipt` (not a `Result`); rejection of candidates is expressed through decisions, not errors.
- No panics.

## Determinism class
- Deterministic: candidate identity is FNV-1a64 over canonical form, promoted selection has a fixed tie-break (lower identity), campaign receipts and `DeltaReceipt` identities are FNV-1a64 over their canonical preimages, `SemanticVariableKind` / locality names are fixed strings, and world-delta apply/revert is a pure function of the world snapshot plus the sorted change list.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- No `crates/emath-tuning/tests/` directory and no inline `#[cfg(test)]` modules in `src/`. Conformance lives in the standalone `tests/emath-tuning` package:
  - `tests/campaign.rs`: `admits_fails_closed_on_missing_cost_or_latency`, `admits_at_the_bounds_with_all_three_measured`.
  - `tests/lib.rs`: receipt identity determinism and sensitivity, locality of a change set, `SemanticVariableKind` canonical-name round-trip, `memorizing_candidate_fails_held_out_challenge`, `general_candidate_survives_and_oversize_table_is_penalized`, `apply_then_revert_restores_canonical_form_and_identity`, `apply_changes_world_identity`, `apply_refuses_missing_target`.
  - `tests/frontier.rs`: `generation_is_deterministic_deduplicated_and_budget_capped`, `seeded_campaign_promotes_equivalent_and_rejects_wrong_before_benchmark` (seeded cache-policy campaign: equivalent faster policy promoted with a stable receipt; wrong policy refused at semantic admission with no benchmark row; host-worse policy refused by the envelope), `promotion_requires_a_baseline_fallback`.

## No-claim boundaries
- A slice of the planned tuning surface, not the full tuning service.
- The `held_out_verified` flag is a per-candidate claim fed into selection, not independent certification.
- Promotion policy here is structural over the declared gates; host objectives and envelopes are the caller's responsibility.
