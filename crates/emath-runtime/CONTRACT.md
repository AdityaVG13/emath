# CONTRACT.md

## Purpose and layer

Runtime outcome model: budgets, cancellation, evidence handles, continuations and explicit `Outcome::Unresolved`. Layer: `core` (per CRATE_MAP.md).

## Public types and semantics

- `Budget { evaluations, iterations, work_units, memory_bytes, output_bytes }`: resource budget with `Default` constants.
- `EvidenceHandle { schema, identity }`: content-addressed reference to the evidence backing an outcome.
- `ContinuationHandle { schema, identity, provider_id }`: reference to a deferred provider continuation.
- `UnresolvedReason`: `MissingProvider`, `UnsupportedSemanticSubset`, `BudgetExhausted`, `InconclusiveEvidence`, `TargetUnavailable`, `PermissionDenied`; each has a stable `as_str`.
- `Outcome<T, E>`: provider execution outcome; `Resolved { value, evidence }`, `Unresolved { reason, partial, continuation, evidence }`, `Failed(E)`; `is_resolved()`.
- `Cancellation` trait with `is_cancelled()`, and `NeverCancel` marker that always reports false.

## Invariants

- Only `Outcome::Resolved` carries admitted value authority.
- `Outcome::Unresolved` is an explicit, typed disposition carrying reason, optional partial value, continuation and evidence; it is never conflated with success or failure.
- Out-of-budget/inconclusive/target-unavailable states surface through `UnresolvedReason`, not silent truncation.

## Error model

No dedicated error type. Failures are carried in `Outcome::Failed(E)` where `E` is the caller's error type; no stable codes emitted by this crate.

## Determinism class

Deterministic. Outcome variants and budget defaults are pure data; no RNG or wall-clock input to the outcome model itself.

## Cancellation behavior

This crate is where cancellation surface exists. `Cancellation::is_cancelled()` is the cooperative query seams providers consult; `NeverCancel` is the explicit "never cancels" marker. This is a query-only surface; the crate does not force aborts.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

None on disk currently. No `tests/` directory and no inline `#[cfg(test)]` module in `lib.rs`.

## No-claim boundaries

- `Budget::default()` values are defaults, not limits enforced by this crate.
- Cancellation is cooperative and query-only (`is_cancelled`); this crate provides no forced-abort mechanism.
