# CONTRACT.md

## Purpose and layer

Evidence IR, assumption ledger, evidence-level policy, certificate registry, content-addressed store, revalidation, optional proof providers and the certify-the-certifier corpus. Layer: `ir` (per CRATE_MAP.md).

Authority is explicit: every claim names its kind, producer and checker roles, freshness window and falsifiers. Assumptions are classified M/N/S/E/H; the E0-E5 policy maps requirements to admissible producer/checker combinations; the registry holds versioned checker contracts; the store addresses records by content and keeps revocation/supersession append-only; revalidation sweeps stale evidence; a fixed unsound-certifier corpus is refused.

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `EvidenceRecord`, `EvidenceKind`, `Freshness`, `Falsifier`/`FalsifierKind`, `ProducerRole`, `CheckerRole`, `Independence` (module `ir`): the claim model with explicit producer/checker/falsifier/freshness.
- `Assumption`, `AssumptionLedger`, `PremiseClass` (module `ledger`): classified assumption ledger with `premise_class_token`.
- `EvidencePolicy`, `EvidenceEntry` (module `policy`): `requirement_for`, `satisfied_by`, `admissible_combos`.
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

Four lib tests, one per EMATH-08-001..004 module:

- `ir::tests::incomplete_computation_cannot_become_resolved_evidence` — a complete Pass record is resolved evidence; incomplete or non-Pass records are refused with `E-EVID-404`; the canonical token is stable and changes when freshness changes.
- `ledger::tests::reclassifying_an_assumption_is_refused` — M/N/S/E/H assumptions record in id order; re-registering the same id under a different class is `E-EVID-405`; identical re-registration is a no-op.
- `policy::tests::bars_get_stronger_with_level` — E0 is satisfied by any producer with no checker; E1–E5 are exact bars (measurement never meets E5; E5 is formal proof plus an independent checker); lower-level evidence does not satisfy a higher requirement.
- `registry::tests::certificate_registry_lookup_and_refusal` — unknown kind/version is `E-EVID-401`, duplicate versioned contract is `E-EVID-402`, empty `admits` is `E-EVID-403`; a registered contract admits only its declared classes.

## No-claim boundaries

- Content identity is the bootstrap FNV-1a identity, not a release cryptographic identity.
- Proof providers are an optional seam: unavailability is a refuse path (E-EVID-506), not a silent fallback to resolved evidence.
