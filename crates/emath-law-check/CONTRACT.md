# CONTRACT — emath-law-check

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Independent world checking (WorldChecker): law obligations over finite worlds, minimized counterexamples, scoped authority, deterministic answer receipts.
- Treats provider output (a `FittedTable` from emath-calibration) as untrusted candidate data and validates claimed law obligations against it.
- Depends on: emath-term, emath-world-ir, emath-calibration, emath-portfolio.

## Public types and semantics
- `Law` - a claimed law obligation: `Commutative`, `Associative`, `Idempotent`, `Identity`, or `Custom`.
- `WorldObligation` - one claimed obligation with a contract identity (`id`, `law`).
- `FiniteLawChecker` - the independent finite-world checker (`check`) plus convenience `check_world`.
- `WorldCheckReport` - candidate identity, verdicts, overall pass, scoped authority, deterministic receipt.
- `LawVerdict` / `MinimizedCounterexample` - per-obligation verdict and, on failure, the minimized (lexicographically smallest) violating carrier tuple.
- `ScopedAuthority` - authority the check endorses, at most `Tested`, limited to the obligation ids actually run.
- `CheckerReceipt` - deterministic FNV-1a64 content identity over candidate and verdicts.
- `CheckerError` - typed reason a check could not run.
- (not exhaustive)

## Invariants
- A check endorses only the obligations it ran, at most `Tested`; no hidden escalation to `Certified`/`Proved`.
- Empty obligation sets, empty tables, custom laws, and untotal tables are refused as typed errors, never vacuously passed.
- Explicit laws are refused (not passed) if the candidate table has no row for the operator.
- Enumeration is deterministic over the sorted carrier; the first violation found is the lexicographically smallest.
- Receipt identity is deterministic: same candidate, obligations, and verdicts replay to the same id.

## Error model
- Typed `CheckerError`: `NoObligations`, `EmptyTable`, `UnknownOperator`, `UnsupportedLaw`, `Untotal`.
- `check` / `check_world` return `Result<WorldCheckReport, CheckerError>`; no panics.

## Determinism class
- Deterministic: carrier sorted, enumeration ordered, counterexample minimized (lexicographically smallest), receipt id derived from candidate and verdicts.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- Inline `#[cfg(test)]` in `src/lib.rs`: `empty_table_is_not_a_vacuous_pass`, `untotal_table_is_refused_not_passed`.
- Integration property grids in `tests/emath-law-check`:
  `finite_max_is_commutative_over_seeded_carrier`,
  `finite_max_is_associative_over_seeded_carrier`,
  `finite_max_has_bottom_identity_over_seeded_carrier`
  (Laws `Commutative` / `Associative` / `Identity` over a total 3-element
  `max` table).

## No-claim boundaries
- Law admission is structural over finite tables, not certified authority.
- `Custom` laws are not checked here and cannot pass; they require an external oracle.
- Counterexamples are minimized over the finite carrier only; enumeration cost grows with carrier size.
