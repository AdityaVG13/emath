# emath-lab-core CONTRACT

## Purpose and layer
- Laboratory core for the Phase 10 laboratory (CRATE_MAP tier: core).
- Experiment manifests, quality gates, measurement, statistical protocol, and promotion policy engine. Everything is std-only and deterministic; wall-clock timing enters only as injected raw samples.
- Brings the std-only SHA-256 primitive used as the keep-gate identity.

## Public types and semantics
- `ExperimentManifest` (schema, experiment_id, baseline, candidate, admission_policy, promotion, created_by) plus `AdmissionPolicy` and `PromotionPolicy`.
- `Observation`, `MetricDefinition`, `MetricKind`, `CandidateAdmission`, `AdmissionStatus` (observed experiment records).
- `Sampler`: deterministic splitmix64-style pseudo-random samples in `[-1, 1)`.
- Re-exported module surfaces: `qualitygate` (`GateCheckKind`, `GateVerdict`, `QualityGate`), `promotion` (`decide`, `PromotionDecision`, `PromotionReason`), `stats` (`StatisticalProtocol`, `evaluate_paired`, `OutlierPolicy`, `PairedResult`), `drift` (`DriftMonitor`, `DriftAlert`, `DriftKind`, `DriftBand`), `selector` (`Selector`, `Route`, `Telemetry`), `identity` (`EngineIdentity`, `EngineRole`), `sha256` (`digest`, `hex`), `error` (`LabError`).
- More modules: `manifest`, `measure` (`HarnessReport`, `Summary`, `DerivedMetric`), `promotion` (`EnginePolicy`, `PromotionOutcome`), `candidate` (`CandidateLoop`, `ParetoArchive`, `Candidate`, `dominates`), `adversarial`, `pilot` (`CachePilot`, `ServeResult`), `supervisor` (`Supervisor`, `TickOutcome`).
- (not exhaustive; `failure` (`FailureBundle`, `TRUE_DIVERGENCE_POINTER`) and `receipt` (`DecisionReceipt`) also re-export at root.)

## Invariants
- Correctness/evidence gates precede performance gates (no promotion before evidence).
- Promotion requires passing numeric gates (absolute/relative error tolerances) and performance gates (median speedup and p99 regression).
- Sampling is deterministic and seeded; default seed is fixed so runs are reproducible.
- Summary `cv_pct` quarantines noisy measurement cells; degenerate samples have zero cv.
- Identity comparator gate: distinct engine identities pass, identical identities are refused.
- Wall-clock timing enters only as injected raw samples; the engine never clocks internally.

## Error model
- Structured `LabError` (module `error`) for laboratory operations; drift surfaced as typed `DriftAlert` (`E-HOST-010`); `failure` bundles true divergence (with `TRUE_DIVERGENCE_POINTER`).
- Statistical protocol returns explicit outcomes (e.g. empty percentiles are an `E-HOST-006` value, not an index panic).

## Determinism class
- Deterministic: std-only with fixed seed; no wall-clock, entropy, or environment dependence.

## Cancellation behavior
- Not applicable: std-only synchronous crate with no cancellation surface.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml.

## Conformance tests
- In-crate `#[cfg(test)]` modules (no `tests/` directory on disk):
  - failure: `monitor_with_fired_alert`, `bundle_emitted_with_true_divergence_pointer`, `bundle_identity_is_deterministic_and_binds_to_identities`, `monitor_emits_bundle_only_after_true_divergence`.
  - stats: `empty_percentile_is_e_host_006_not_an_index_panic`, `single_sample_percentile_is_the_sample`, `interpolated_percentile_of_two_samples`.
  - identity: `distinct_identities_pass_the_comparator_gate`, `identical_identities_are_refused_by_the_comparator_gate`, `tokens_are_role_label_stable`.
  - measure: `cv_pct_quarantines_noisy_cells`, `degenerate_samples_have_zero_cv`.
  - promotion: `currently_promoted_non_regressed_stays_on_the_promoted_route`, `regressed_promoted_candidate_is_demoted_not_retained`, `not_promoted_candidate_between_targets_goes_canary`.
  - sha256: `nist_vectors_match`, `digest_is_deterministic`.

## No-claim boundaries
- The keep-gate SHA-256 is an identity primitive, not release cryptographic hardening.
- Wall-clock-injected samples are trusted input; the engine does not validate their provenance.
- Multi-declaration and out-of-subset candidate workloads are outside this crate's guarantee.
