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

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- Integration test tests/parse_forest_schema.rs.
- No #[cfg(test)] module declared inside lib.rs.

## No-claim boundaries

- Example worlds cover only the reference alien signature, not arbitrary user worlds.
- Provider-neutral by construction: no provider spawning or external evaluation occurs here.
