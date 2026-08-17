# emath-portfolio

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP; may depend on meaning-provider-api).
- Deterministic interpretation portfolios: collects, ranks, and selects interpretation candidates over world identities.

## Public types and semantics

- InterpretationPortfolio: deterministic candidate collection, sorted by a stable policy; new sorts and candidates exposes the order.
- InterpretationCandidate: world id, name, canonical answer, scoped Authority, score vector, provenance summary.
- Authority enum: Structural, Tested, Certified, Proved.
- ScoreVector: multi-objective cost, complexity, evidence, utility (f64); lower cost/complexity and higher evidence/utility preferred.
- Re-exports from submodules: select, SelectionOutcome, SelectionPolicy, SelectionWeights; CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict; replay_identity, PortfolioLock. (not exhaustive.)

## Invariants

- Portfolio order is deterministic by policy: authority descending, utility descending, cost ascending, complexity ascending, then world id ascending.
- f64 comparisons use total_cmp, giving a total order over finite and NaN values alike.
- Pareto semantics are kept: candidates are retained rather than dropped on conflicting objectives.

## Error model

- No errors emitted at this layer; selection consumes precomputed scores and verdicts.

## Determinism class

- Deterministic: identical candidates yield identical portfolio order, enabling replay.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- None listed: no tests/ directory and no #[cfg(test)] module declared in lib.rs.

## No-claim boundaries

- Keep-pareto semantics only over the candidates supplied; E-GEN-090/091 deferred worlds are recorded as deferred entries, not prioritized by this crate.
- Selection correctness depends on precomputed, honest ScoreVector and authority inputs.
