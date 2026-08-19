# emath-genesis

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP).
- Minimal Semantic Genesis evaluator and built-in example worlds.
- Hosts the G1 world-side stage, moved from emath-syntax (world-side fence): the forest module builds a bounded parse forest over a genesis body expression, infers the world signature, and constructs emath-term and emath-world-ir values directly.
- emath-syntax keeps the G0 emath custom section parser and re-exports this module for the CLI.

## Public types and semantics

- FirstOrderWorld trait: generic first-order world over a value type and an error type; constant resolves nullary symbols, apply applies an operator to evaluated arguments.
- EvalError enum: MissingVariable, UnknownSymbol, Arity (symbol, expected, actual); implements std::error::Error.
- FreeTermWorld: free symbolic world whose values remain Terms (structural, no hardcoded meaning).
- BooleanAlienWorld: Boolean interpretation of the reference alien signature (z true, join xor, neg not, times and).
- ModularAlienWorld: modular-17 interpretation (z = 3, join add, neg square, times multiply, rem_euclid(17)).
- Environment<V> type alias: BTreeMap<VariableId, V> free-variable valuation.
- evaluate fn: total recursive evaluator over Term, propagating world errors via W::Error: From<EvalError>.
- free_symbolic_world and reference_alien_term fns: produce provider-neutral WorldIr and the reference term/signature. (not exhaustive; see the forest module.)
- vm module (schema emath.vm, version 1): explicit-stack semantic VM. VmBudget (step ceiling; seed_default 4096), VmStep/VmTrace (deterministic per-frame trace with canonical text encoding), run/resume (metered execution returning VmOutcome::Complete { value, steps, trace } or VmOutcome::Suspended(VmContinuation) when the budget is exhausted; continuations resume losslessly with a fresh budget).
- csa module (schema emath.csa, version 1): canonical seeded algebra, the totality baseline (ADR-003). OnePointWorld (degenerate one-point algebra, total). SeededCsaWorld (seeded FNV-1a mixing over u64: total, bit-exact reproducible per seed; distinct seeds are the built-in negative control). CSA_MEANING_CLAIM labels every CSA artifact as never author-intended meaning.
- specialization module (SG-17): SpecializationCache keyed by WorldId, with hit/miss/challenge counters. challenge(bound, presented) reuses a cached specialization only when WorldId is unchanged; an identity change is SpecializationChallenge::IdentityChanged (never a silent reuse). Process-local BTreeMap; no on-disk format.

## Invariants

- Evaluating a well-formed term under a matching symbolic world never panics; all failure is typed via EvalError.
- Seed worlds bind exactly the reference alien signature; unknown symbols and wrong arities are errors, not fallthroughs.
- free_symbolic_world emits a provider-neutral free WorldIr with derived structural constructor semantics.
- Terms are built from emath_term structures in stable iteration order.

## Error model

- EvalError: MissingVariable, UnknownSymbol, Arity; converted into W::Error via From<EvalError>.
- Genesis syntax codes (E-SYN-201..211) are emitted by emath-syntax, not by this crate.

## Determinism class

- Deterministic: WorldIr construction iterates a BTreeMap signature in sorted key order.
- Outputs are byte-comparable where serialized.
- VM traces are bit-identical across runs for the same term/world/valuation; VM results agree with the recursive evaluator (tested).
- CSA values are bit-exact across runs, hosts and tool versions (pure integer mixing, no floats).
- SpecializationCache is deterministic: identical insert/get/challenge sequences yield identical counters and BTreeMap key order.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- Integration test tests/parse_forest_schema.rs.
- forest.rs unit tests (G1 gates): trailing-operator postfix hypothesis with fixity/arity inference; deterministic fixity-hypothesis priority (infix > prefix > postfix > constant); ranking/receipt determinism across rebuilds (byte-identical canonical JSON, stable parse_id, reference body unique); bounded ambiguity-explosion falsifier (application-argument queue capped).
- lib.rs unit tests: free-world universal round-trip and mutation detection.
- vm.rs unit tests: agreement with the recursive evaluator, deterministic traces, budget suspension + lossless resume, typed unknown-symbol and missing-variable errors.
- csa.rs unit tests: totality + reproducibility, seeded negative control, argument-order sensitivity, one-point totality, meaning-claim labeling.
- specialization.rs unit tests: hit on identical WorldId; miss plus challenge refusal when identity changed; deterministic counters and key order across independent rebuilds.

## No-claim boundaries

- Example worlds cover only the reference alien signature, not arbitrary user worlds.
- Provider-neutral by construction: no provider spawning or external evaluation occurs here.
- SpecializationCache does not compile, load, or execute specialized code; it only records and challenges WorldId-bound artifacts. WORLD_IR_VERSION / WORLD_ABI_VERSION bumps change identity or ABI, so an old cache entry cannot satisfy a challenge against a new WorldId (rollback is refuse-and-recompile; there is no on-disk cache to migrate).
