# emath-portfolio

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP; may depend on meaning-provider-api).
- Deterministic interpretation portfolios: collects, ranks, and selects interpretation candidates over world identities.

## Public types and semantics

- InterpretationPortfolio: deterministic candidate collection, sorted by a stable policy; new sorts and candidates exposes the order.
- InterpretationCandidate: world id, name, canonical answer, scoped Authority, score vector, provenance summary.
- translated_candidate(morphism, base, answer): admits a candidate into the morphism target world, records the morphism identity in provenance, and caps authority by the morphism's preservation relation.
- Authority enum: Structural, Tested, Certified, Proved.
- ScoreVector: multi-objective cost, complexity, evidence, utility (f64); lower cost/complexity and higher evidence/utility preferred.
- PORTFOLIO_SCHEMA_VERSION (1): version constant for the portfolio document layout (durable id emath.interpretation-portfolio lives in the schema registry).
- Re-exports from submodules: select, SelectionOutcome, SelectionPolicy, SelectionWeights; CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict; replay_identity, PortfolioLock. (not exhaustive.)

## Invariants

- Portfolio order is deterministic by policy: authority descending, utility descending, cost ascending, complexity ascending, then world id ascending.
- f64 comparisons use total_cmp, giving a total order over finite and NaN values alike.
- Pareto semantics are kept: candidates are retained rather than dropped on conflicting objectives.
- translated_candidate never raises authority. Exact and refinement relations keep the base authority; approximation, simulation, and observational-equivalence degrade it to Structural. When obligations disagree, any degrading relation wins.

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

- lib.rs unit tests: stable-policy ordering (authority > utility > cost) and world-identity tie-breaking; exit-gate translation keeps both source and target worlds and deopts on a failed fast-path guard; approximation caps Tested to Structural while exact preserves it.

## No-claim boundaries

- Keep-pareto semantics only over the candidates supplied; E-GEN-090/091 deferred worlds are recorded as deferred entries, not prioritized by this crate.
- Selection correctness depends on precomputed, honest ScoreVector and authority inputs.
