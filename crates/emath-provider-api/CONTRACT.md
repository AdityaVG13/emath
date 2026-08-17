# CONTRACT.md - emath-provider-api

## Purpose and layer

Tier 3 (goals and providers) adapter seam. Core dependency for
`emath-adapter-*`, `emath-registry`, and provider crates. Defines the
provider descriptor, capability, provider/adapter/checker trait seam plus
registry, filter, descriptor and constellation surfaces.

## Public types and semantics

- `ProviderDescriptor`: schema, id, version, implementation content id,
  goal kinds, semantic subsets, targets, maximum evidence level,
  determinism flag, permissions, checker bindings.
- `CapabilityReport` with `CapabilityReason` and `CostEstimate`: whether a
  goal is supported, reasons, and estimated cost.
- `ProviderResult`: untrusted provider transport (schema, goal identity,
  payload, optional certificate, evidence claims) until a checker admits it.
- `Provider` trait: `descriptor`, `supports`, `execute` (budgeted, cancelable).
- `Adapter<Source, Target>` trait: `encode`/`decode` over a declared relation.
- `ResultChecker` trait: `check` admits a `ProviderResult` into `Admitted`.
- Re-exports from `constellation`, `descriptor`, `filter`, `registry`
  modules add constellation, lock, filter and registry types (not exhaustive).

## Invariants

- Provider output is untrusted transport until a `ResultChecker` admits it
  (Constitution 8).
- `encode: E -> Result<P, AdapterRefusal>` and
  `decode: P -> Result<E', AdapterRefusal>` with a declared relation
  `R(E, E')` (Constitution 7).
- An adapter must refuse unsupported semantics before provider execution,
  never silently approximate.
- Phase 1 ships no concrete providers; this API is the frozen adapter seam.

## Error model

`ProviderError` carries a stable string `code` plus `message`. Sub-modules
emit typed errors under `E-PROV-501..524`: descriptor self-validation
(`E-PROV-501..503`), registry (`E-PROV-510/511`), filter
(`E-PROV-512..516`), ledger/constellation (`E-PROV-521..523/524`).

## Determinism class

Structural types are deterministic (derive `PartialEq, Eq`, no interior
state). Execution determinism is a declared per-provider property
(`ProviderDescriptor.deterministic`), not enforced or proven by this crate.

## Cancellation behavior

`Provider::execute` takes `&dyn Cancellation` (from `emath-runtime`) and
returns `Outcome`; the crate itself is std-only and synchronous with no own
cancellation surface. Cancellation is passed through to the provider.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

No `tests/` directory on disk. Inline `mod tests` exists in `src/filter.rs`
(`#[test]` on goal filtering verdicts, including `E-PROV-515` refusal).

## No-claim boundaries

Phase 1 ships no concrete providers, so no provider semantics are executed
by this crate. Whether a provider's output is correct is out of scope for
the trait seam and must be established by the provider's `ResultChecker`.
