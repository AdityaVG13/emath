# CONTRACT.md

## Purpose and layer

Independent artifact checking, translation validation, negative controls and claim-language linting. Layer: `evidence/artifact` (per CRATE_MAP.md).

The checker never invokes generator internals: authority is rebuilt exclusively from the retained artifact (manifest, source map, plan, evidence bundle, file content, provider locks) and content identity.

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `ArtifactInput`, `ArtifactCheckConfig`, `ArtifactCheckIssue`, `ArtifactCheckReport`, `ProviderLockRecord` (module `artifact_check`): `check_artifact`, `check_artifact_dir`, `artifact_input_from_dir`.
- `ClaimLinter`, `LintIssue` (module `claimlint`): `lint_claims`.
- `NegativeControl`, `NegativeControlKind`, `ControlRun` (module `negative`): `run_standard_battery`, `run_negative_controls`, `seed_incomplete`, `seed_stale`, `seed_tampered`, `seed_unsupported`, `seed_wrong_goal`, `seed_wrong_derivative` (Phase 3 planted-value stand-in; refuses via translation `E-EVID-301`, not a differentiate producer).
- `EquivalenceWitness`, `TranslationRelation`, `TranslationSample` (module `translation`): `validate_translation`, `check_witness`.
- `CheckerError`: shared failure with a stable `code` and `message`.

## Invariants

- The checker never calls generator internals; it reads only the retained artifact and recomputes content identity.
- Artifact identity must recompute (E-EVID-102); content identity must match (E-EVID-101).
- Evidence/goal scope must match the artifact (E-EVID-103); stale certificates refuse (E-EVID-104). Manifest `evidence_level` must not exceed the strongest Pass claim level (also `E-EVID-103`).
- Symlink and non-UTF-8 paths are refused (E-EVID-113, E-EVID-114).
- Claim language must not be stronger than available evidence (E-EVID-201).
- Translation mismatch has no independent verification basis unless a witness verifies (E-EVID-301, E-EVID-302).

## Error model

`CheckerError { code: &'static str, message: String }`. Stable codes `E-EVID-101`..`E-EVID-114` (artifact battery), `E-EVID-201` (claimlint), `E-EVID-301`/`E-EVID-302` (translation). Identity uses deterministic FNV-1a64 (`identity_of`).

## Determinism class

Deterministic. Content identity is FNV-1a64 over retained text; checks are pure functions of artifact bytes and config.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

Integration coverage in `tests/emath-checker`: translation witness recheck,
`E-EVID-301` / `E-EVID-302` refusals, and
`seeded_wrong_derivative_is_refused_with_e_evid_301` (Phase 3 planted
wrong derivative row via `seed_wrong_derivative`).

## No-claim boundaries

- Content identity is the bootstrap FNV-1a hash, not a release cryptographic identity.
- The checker refuses tampered, stale and wrong-goal certificates; it claims no certification power beyond the documented E-EVID checks.
