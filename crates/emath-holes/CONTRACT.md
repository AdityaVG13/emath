# CONTRACT — emath-holes

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Meaning holes and finite synthesis: an underconstrained construct is a meaning hole.
- Delivers a deterministic hole graph (stable ids, kinds, states, dependencies, budget bookkeeping) and finite synthesis of operator tables validated by emath-law-check.
- Depends on: emath-term, emath-world-ir, emath-calibration, emath-law-check.

## Public types and semantics
- `MeaningHole` / `MeaningHoleKind` / `HoleState` - one hole, its kind, and its state.
- `HoleGraph` - deterministic hole graph with stable ids, dependencies (`dependencies`, `dependents`), and immutable `with_updated` continuations.
- `SynthesisLaw` - a law a synthesized table must satisfy; `as_law` converts to an emath-law-check `Law`.
- `SynthesisError` - typed refusal of a synthesis run (`EmptyCarrier`, `CarrierTooLarge`, `EmptyLaws`).
- `SynthesisRun` - synthesized tables (deterministic enumeration order), count examined, and honest `exhaustive` flag.
- `SolveReceipt` - deterministic receipt of one solver continuation.
- `Continuation` - a new immutable problem state (next graph) plus a receipt.
- `solve_op_hole` / `synthesize_tables` / `check_laws` - entry points; `MAX_CARRIER_SIZE` caps the enumeration. `check_laws` surfaces minimized counterexamples from emath-law-check for an existing table.
- (not exhaustive)

## Invariants
- Solving a hole produces a new immutable problem state (the next graph) plus a receipt; the authoritative graph is never mutated.
- Failed proposals never mutate the graph.
- Law validation uses the independent finite-law checker, so only tables satisfying every declared law are synthesized.
- An empty law set is refused (typed `EmptyLaws`), never reported as `Contradictory`.
- Carrier is capped by `MAX_CARRIER_SIZE`; exhaustion status is reported honestly (untotal space yields `exhaustive == false`, not a silent promise).

## Error model
- Typed `SynthesisError` with stable ident requests: `EmptyCarrier`, `CarrierTooLarge`, `EmptyLaws` (documented as `E-RES-111` / `E-RES-110` refusals).
- No panics on the refusal paths.

## Determinism class
- Deterministic: finite-carrier enumeration over `carrier^(n^2)`, stable hole ids, and deterministic continuation receipts.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface. Enumeration is bounded by `MAX_CARRIER_SIZE`; budget cuts surface via the `exhaustive` flag rather than async cancellation.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- Workspace integration suite `tests/emath-holes` (`tests/synth.rs`): `n3_commutative_synthesis_is_exhaustive_over_the_full_table_space`, `budget_cut_reports_not_exhaustive`, `empty_laws_are_refused_not_contradictory`, `impossible_identity_laws_are_rejected_exhaustively` (seeded two-identity set, n=2, `tables.is_empty()` and `exhaustive == true`), `carrier8_table_space_is_honestly_not_exhaustive`, `noncommutative_table_is_rejected_with_minimized_counterexample`.
- Production path: `cargo xtask demo holes-synthesis` synthesizes commutative-monoid tables on a 2-element carrier and exhaustively rejects `impossible_identity_laws`.
- No `tests/` directory on disk in the crate and no `#[cfg(test)]` module in `src/`.

## No-claim boundaries
- A slice of the planned finite-synthesis surface, not the full production synthesis service.
- Synthesis is limited to small carriers (`MAX_CARRIER_SIZE`); larger spaces are not exhaustively verified.
- Synthesized tables satisfy declared laws over the finite carrier; this is not certified semantics.
