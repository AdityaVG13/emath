# CONTRACT.md - emath-meaning-provider-api

## Purpose and layer

Tier 6 (semantic genesis substrate) stable contracts for meaning proposal
and world checking. Depended on by `emath-portfolio`, `emath-law-check`,
`emath-tuning`, and `emath-agent-protocol`. Depends on `emath-term` and
`emath-world-ir`.

## Public types and semantics

- `MeaningProblem`: provider-neutral meaning problem (signature, root term,
  open semantic holes, hard constraints, behavioral examples).
- `Budget`: bounded provider request (max proposals, max work units,
  deterministic seed).
- `WorldCandidate`: proposed world with no automatic authority (world,
  provider id, claimed obligations, proposal receipt).
- `WorldObligation`, `ObligationVerdict`, `WorldCheckReport`: obligation
  challenge surface and per-obligation verdicts with a checker receipt.
- `ProviderError`, `CheckerError`: distinct role errors.
- `MeaningProvider` trait: `propose` produces bounded proposals.
- `WorldChecker` trait: `check` challenges candidate worlds under a bounded
  request.
- (not exhaustive).

## Invariants

- Proposed worlds carry no automatic authority; validity requires a
  `WorldChecker` report.
- Proposals and checks are bounded by `Budget` (`max_proposals`,
  `max_work_units`, deterministic `seed`).
- `MeaningProvider` and `WorldChecker` are independent roles; the checker
  challenges candidates rather than trusting the proposing provider.
- Distinction between `ProviderError` and `CheckerError` is by role.

## Error model

`ProviderError` and `CheckerError` each carry a stable string `code` and a
human-readable `detail`. Two separate error types by role.

## Determinism class

`Budget` carries a deterministic seed to make bounded search reproducible.
Meaning/World IR derivation is intended deterministic under a fixed budget
seed; the API exposes the seed surface to that end.

## Cancellation behavior

Not applicable. Trait methods are synchronous with no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

None present. No `tests/` directory on disk and no inline `#[cfg(test)]`
module in `src/lib.rs`.

## No-claim boundaries

A `WorldCandidate`'s `proposal_receipt` is provider-local provenance and is
not an admission by a checker; authority is only established by a
`WorldChecker` report.
