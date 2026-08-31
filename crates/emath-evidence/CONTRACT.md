# CONTRACT.md

## Purpose and layer

Evidence IR, assumption ledger, evidence-level policy, certificate registry, content-addressed store, revalidation, optional proof providers and the certify-the-certifier corpus. Layer: `ir` (per CRATE_MAP.md).

Authority is explicit: every claim names its kind, producer and checker roles, freshness window and falsifiers. Assumptions are classified M/N/S/E/H; the E0-E5 policy maps requirements to admissible producer/checker combinations; the registry holds versioned checker contracts; the store addresses records by content and keeps revocation/supersession append-only; revalidation sweeps stale evidence; a fixed unsound-certifier corpus is refused.

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `EvidenceRecord`, `EvidenceKind`, `Freshness`, `Falsifier`/`FalsifierKind`, `ProducerRole`, `CheckerRole`, `Independence` (module `ir`): the claim model with explicit producer/checker/falsifier/freshness.
- `Assumption`, `AssumptionLedger`, `PremiseClass` (module `ledger`): classified assumption ledger with `premise_class_token`.
- `EvidencePolicy`, `EvidenceEntry` (module `policy`): `requirement_for`, `satisfied_by`, `admissible_combos`. Default classes include the checker battery (`correctness`, `equivalence`, `performance`, `safety`) and the native build claim classes (`static-semantics`, `codegen`).
- `CertificateRegistry`, `CheckerContract`, `CertificateKind` (module `registry`): `register_contract`, `lookup_contract`, `admits_claim_class`.
- `EvidenceStore` (module `store`): content-addressed store with `store_address`.
- `RevalidationConfig`, `RevalidationReport`, `RevalidationTrigger` (module `revalidation`): `require_promotable`, `revalidation_sweep`.
- `ProofVerdict`, `ProofVerdictKind`, `ProofProvider` (module `proof`): optional proof seam with `verify_proof_optional`.
- `EvidenceError`: shared failure with a stable `code` and `message`.
- `UnsoundFixture`, `CERTIFY_THE_CERTIFIER`, `reject_unsound_certifier_output` (module `certify`).

## Invariants

- Every claim names its producer, checker, freshness window and falsifiers; no anonymous authority.
- The store is content-addressed: address derives from content identity.
- Revocation and supersession are append-only; duplicate markers and double supersession are refused (E-EVID-502, E-EVID-504).
- An incomplete computation cannot become resolved evidence (E-EVID-404).
- A fixed unsound-certifier corpus is rejected (E-EVID-507).
- Stale records are refused for promotion (E-EVID-505).

## Error model

`EvidenceError { code: &'static str, message: String }`. Stable codes `E-EVID-401`..`E-EVID-507`: `401` unknown certificate kind, `402` duplicate checker contract, `403` contract does not admit the claim class, `404` incomplete computation becomes resolved, `405` assumption re-registered under a different class, `501` unknown record id, `502` duplicate revocation marker, `503` content-identity mismatch, `504` double supersession, `505` stale record refused, `506` proof provider unavailable, `507` unsound certifier output.

## Determinism class

Deterministic. Store addressing and revalidation are deterministic functions of content and configuration; no wall-clock or RNG input to identity.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

Workspace integration suite `tests/emath-evidence`, one file per EMATH-08-001..004 module:

- `tests/ir.rs` `incomplete_computation_cannot_become_resolved_evidence` — a complete Pass record is resolved evidence; incomplete or non-Pass records are refused with `E-EVID-404`; the canonical token is stable and changes when freshness changes.
- `tests/ledger.rs` `reclassifying_an_assumption_is_refused` — M/N/S/E/H assumptions record in id order; re-registering the same id under a different class is `E-EVID-405`; identical re-registration is a no-op.
- `tests/policy.rs` `bars_get_stronger_with_level` — E0 is satisfied by any producer with no checker; E1–E5 are exact bars (measurement never meets E5; E5 is formal proof plus an independent checker); lower-level evidence does not satisfy a higher requirement.
- `tests/registry.rs` `certificate_registry_lookup_and_refusal` — unknown kind/version is `E-EVID-401`, duplicate versioned contract is `E-EVID-402`, empty `admits` is `E-EVID-403`; a registered contract admits only its declared classes.

## No-claim boundaries

- Content identity is the bootstrap FNV-1a identity, not a release cryptographic identity.
- Proof providers are an optional seam: unavailability is a refuse path (E-EVID-506), not a silent fallback to resolved evidence.

## Absorbed module: `checker` (was `emath-checker`)

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

Integration coverage in `tests/emath-evidence` (`tests/negative.rs`, `tests/translation.rs`, from the former `tests/emath-checker` package): translation witness recheck,
`E-EVID-301` / `E-EVID-302` refusals, and
`seeded_wrong_derivative_is_refused_with_e_evid_301` (Phase 3 planted
wrong derivative row via `seed_wrong_derivative`).

## No-claim boundaries

- Content identity is the bootstrap FNV-1a hash, not a release cryptographic identity.
- The checker refuses tampered, stale and wrong-goal certificates; it claims no certification power beyond the documented E-EVID checks.
