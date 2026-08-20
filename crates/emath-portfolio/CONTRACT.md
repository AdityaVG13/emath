# emath-portfolio

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP; may depend on meaning-provider-api).
- Deterministic interpretation portfolios: collects, ranks, and selects interpretation candidates over world identities.
- G7 extends this crate in place: integer-metric ranking, Pareto archive, selection policies with an explicit collapse gate, disqualification ledger, and byte-identical receipt replay.

## Public types and semantics

- InterpretationPortfolio: deterministic candidate collection, sorted by a stable policy; new sorts and candidates exposes the order.
- InterpretationCandidate: world id, name, canonical answer, scoped Authority, score vector, provenance summary.
- translated_candidate(morphism, base, answer): admits a candidate into the morphism target world, records the morphism identity in provenance, and caps authority by the morphism's preservation relation.
- Authority enum: Structural < Tested < Certified < Proved (lattice rank 0..=3). `as_str` / `lattice_rank` are the wire and ranking forms. Ranking never raises a label.
- ScoreVector: multi-objective cost, complexity, evidence, utility (f64); lower cost/complexity and higher evidence/utility preferred. Genesis-era only; G7 uses integer metrics.
- PORTFOLIO_SCHEMA_VERSION (1): version constant for the portfolio document layout (durable id emath.interpretation-portfolio lives in the schema registry).
- WorldCandidate (G7 record): `world_fingerprint`, `provider_id`, `evidence_authority`, `labeled_authority`, `metrics` (`BTreeMap<String, i64>`), `artifact_hash`, optional `guard_failure`. `new` sets the label equal to evidence. `with_claimed_label` refuses if the claim is strictly above evidence.
- CandidateRecord::world_candidate: projects the genesis-era record onto WorldCandidate; ScoreVector floats become milli-unit integers.
- MetricAxis / MetricPolarity: declared ranking and Pareto axes (`max` / `min`).
- RANKING_KEY_SPEC: `authority.desc,axes.declared,fingerprint.asc,provider.asc,artifact.asc`.
- InterpretationPolicy: `Portfolio` (keep all non-dominated), `SingleBest { collapse }`, or `UserLocked { lock_id, origin_receipt_id, method }` (single-world user lock; provenance `user-locked`). `canonical()` is the wire name.
- CollapsePolicy: `RequireUnique` (exit gate) or `RankKey` (explicit collapse).
- MeaningLock (project-local `.emath/meaning.lock`, schema `emath.meaning-lock` v1): BTreeMap-ordered, byte-stable JSON. Entries keyed by `(declaration_id, hole_id)` store `world_fingerprint` (the same `WorldIr::identity` / `WorldCandidate::world_fingerprint` used by G7), `portfolio_receipt_id`, `selection_method`, `source` / `source_hash` drift witnesses, and `selected_at` (excluded from `lock_id`). `DEFAULT_PORTFOLIO_CAP` is 5 (receipted `portfolio_cap`, not a hidden constant). `commit_locked_world` skips ranking and emits a `UserLocked` receipt. `refuse_disqualified` refuses `set` when the fingerprint is on the checker/guard ledger (dominated worlds remain choosable).
- Re-exports: `MeaningLock`, `LockEntry`, `LockKey`, `LockError`, `SelectionMethod`, `commit_locked_world`, `refuse_disqualified`, `apply_portfolio_cap`, `DEFAULT_PORTFOLIO_CAP`, `PROVENANCE_USER_LOCKED`, `WHOLE_TERM_HOLE`, `LOCK_SCHEMA` / `LOCK_SCHEMA_VERSION`.
- ParetoArchive: non-dominated set in ranking-key order; dominated members recorded with the lowest-fingerprint witness.
- PortfolioReceipt: `input` (policy, axes, candidates sorted by fingerprint), `ranked`, `selected`, `archived`, `ledger`, `receipt_id` (FNV-1a64 of the canonical body). `encode()` is the durable byte form.
- ReceiptInput + `replay(input)`: re-runs selection; success is byte-identical to the original `encode()`.
- Re-exports from submodules: select, SelectionOutcome, SelectionPolicy, SelectionWeights; CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict, GuardFailure, WorldCandidate; replay_identity, PortfolioLock. (not exhaustive.)

## Ranking key

Total order, no floating comparison:

1. `evidence_authority` descending (`Proved > Certified > Tested > Structural`)
2. each declared metric axis in declaration order: Maximize → larger `i64` first; Minimize → smaller `i64` first
3. `world_fingerprint` ascending (bit-exact tie-break)
4. `provider_id` ascending
5. `artifact_hash` ascending

Missing declared metrics disqualify as `failed-guard:missing-metric` and never enter ranking or Pareto.

## Invariants

- Portfolio order is deterministic by policy: authority descending, utility descending, cost ascending, complexity ascending, then world id ascending.
- f64 comparisons in the genesis-era `InterpretationPortfolio` use total_cmp, giving a total order over finite and NaN values alike.
- G7 ranking uses only `i64` metrics and `Ord` on authority / fingerprints.
- Pareto semantics are kept: candidates are retained rather than dropped on conflicting objectives. Dominated G7 candidates are recorded on the archive and on the ledger; they are not silently dropped.
- translated_candidate never raises authority. Exact and refinement relations keep the base authority; approximation, simulation, and observational-equivalence degrade it to Structural. When obligations disagree, any degrading relation wins.
- Authority non-escalation: `labeled_authority` must be `<= evidence_authority`. `evaluate` refuses the whole run on a seeded escalation. Ranking and selection never emit a candidate labeled above its evidence.
- Exit gate: `SingleBest { collapse: RequireUnique }` with more than one non-dominated world is `PortfolioError::AmbiguousSingleBest`. There is no hidden single-world selection.
- Ledger completeness: for a successful receipt, `selected ∪ archived ∪ ledger` is a partition of the input fingerprints (`selected + archived + disqualified = input`).
- Receipt replay: `replay(&receipt.input).encode()` equals `receipt.encode()` byte-for-byte.
- Meaning lock: a matching lock commits to that world fingerprint before ranking; the run is single-world. Drifted, missing, or inadmissible locked worlds refuse (`E-LOCK-004`) with a hint to `emath meaning unset` — never a silent fallback to another world. Tampered `lock_id` refuses (`E-LOCK-003`). Malformed files refuse (`E-LOCK-001`); unknown `schema_version` refuses (`E-LOCK-002`). Locked receipts record provenance `user-locked` and copy the candidate's evidence authority; a lock never escalates authority. Locks are local-side (per-user, per-project) and are not baked into shared source; teams MAY commit `.emath/meaning.lock` to share one interpretation.

## Error model

- Genesis-era `select` emits no errors; it consumes precomputed scores and verdicts.
- G7 `evaluate` / `replay` / `WorldCandidate::with_claimed_label` return `PortfolioError`:
  - `AmbiguousSingleBest { nondominated }`: single-best exit gate.
  - `NoViableCandidate`: single-best with an empty archive.
  - `AuthorityEscalation { fingerprint, evidence, claimed }`: label above evidence.
  - `DuplicateFingerprint { fingerprint }`: two candidates share a world fingerprint.
- Meaning-lock `LockError` (every token is registered in `implementation/ERROR_CODES.md`):
  - `E-LOCK-001` `Malformed`: truncated JSON, unknown fields, missing fields, unreadable file.
  - `E-LOCK-002` `UnknownVersion`: `schema_version` other than 1.
  - `E-LOCK-003` `Tampered`: stored `lock_id` does not match the identity body (fingerprint edits without a matching id).
  - `E-LOCK-004` `Drifted`: locked world missing from current identities, source/declaration witness mismatch, or the locked candidate is no longer admissible.
  - `E-LOCK-005` `Disqualified`: `set` targeted a checker/guard-disqualified world; the diagnostic includes the ledger row.
  - `E-LOCK-006` `UnknownCandidate`: `set` named a fingerprint that is not in the current portfolio.

## Determinism class

- Deterministic: identical candidates yield identical portfolio order, enabling replay.
- G7 receipts are order-independent: input candidates are stored fingerprint-ascending; ranking ignores caller order.
- Meaning-lock encode is BTreeMap-ordered and byte-stable for the same entries and `selected_at`. `selected_at` is excluded from `lock_id`; changing the timestamp does not change identity.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- lib.rs / tests/emath-portfolio: stable-policy ordering (authority > utility > cost) and world-identity tie-breaking; exit-gate translation keeps both source and target worlds and deopts on a failed fast-path guard; approximation caps Tested to Structural while exact preserves it.
- `interpretation` unit tests: ranking determinism and fingerprint tie-break; hand-computed 3-candidate Pareto archive; single-best refusal gate; ledger completeness; receipt replay byte-identity; authority non-escalation plus seeded escalation refusal; explicit `RankKey` collapse.
- `meaning_lock` unit tests: encode round-trip byte-determinism; timestamp excluded from `lock_id`; unknown version; malformed file; tampered fingerprint; fingerprint match vs source drift; `commit_locked_world` single-world user-locked receipt; disqualified `set`; drifted locked candidate.
- Production path: `cargo xtask demo interpretation-portfolio`.

## No-claim boundaries

- Keep-pareto semantics only over the candidates supplied; E-GEN-090/091 deferred worlds are recorded as deferred entries, not prioritized by this crate.
- Selection correctness depends on precomputed, honest ScoreVector / integer-metric and authority inputs. This crate does not mint evidence or raise authority.
- Genesis still emits the genesis-era `InterpretationPortfolio` JSON bag (`keep: pareto N` is a cap on that bag). Answer selection is `evaluate` / `replay` over `InterpretationCandidate::world_candidate` (uniform `cost=1`, so domination cannot drop a kept world). `g7-portfolio-receipt.txt` is the selection artifact. Hidden single-winner collapse is `E-GEN-095`.
- Meaning-provider discovery and world construction live in other crates; this crate ranks and selects records it is given.
- A user lock does not promote a world to `tested`/`certified`/`proved`. Provenance `user-locked` is a selection source, not an authority upgrade.
