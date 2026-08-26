# emath-term CONTRACT.md

## Purpose and layer

- Tier 1 (language) per `implementation/CRATE_MAP.md`.
- Provider-neutral first-order term representation with a canonical round-trip.
- Std only; declares no dependencies.

## Public types and semantics

- `Term` — a finite first-order term: `Variable(VariableId)`, `Constant(SymbolId)`, or `Apply { operator, arguments }`.
- `SymbolId`, `VariableId` — stable string identities, ordered and hashable.
- `Signature` — maps symbols to arities; `insert` (rejecting conflicts), `arity`, `iter` (canonical order), `validate`.
- `TermError` — structural validation error (`UnknownSymbol`, `ArityMismatch`, `ConflictingArity`).
- `CanonicalError` — parse error from `Term::parse_canonical` (`Malformed`, `Trailing`).
- `Term::canonical` and `Term::parse_canonical` — deterministic structural form independent of glyph fixity, with byte-exact round-trip.
- `TERM_IR_SCHEMA` (`emath.term-ir`) / `TERM_IR_VERSION` (1) — version constants for the canonical text encoding; consumers refuse versions they do not know. The `free-term.json` artifact discloses `schema_version` from this constant.

## Invariants

- Every symbol used in a term must have a declared arity; applications must match that arity.
- A symbol may be declared with only one, consistent arity.
- `canonical` shapes are `var(...)`, `const(...)`, `apply(operator,arg,...)` with `\\ \( \) \,` escaping, round-tripping byte-exactly.
- `parse_canonical` rejects trailing non-whitespace content after the term.
- `parse_canonical` refuses unknown escapes and unescaped `(` (and unescaped `,` in `var`/`const` names), so nested-looking forms such as `apply(const(ζ)` are not flattened into an operator name.

## Error model

- Defines its own error types `TermError` and `CanonicalError`; neither carries stable codes. No `emath_core::Diagnostics`.

## Determinism class

- `canonical` is deterministic and independent of glyph fixity; `Signature::iter` is in canonical symbol order (backed by `BTreeMap`); the round-trip is byte-exact.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-term/tests/` directory on disk; the conformance suite lives in the workspace member `tests/emath-term` (`term_public_api.rs`, `term_oracle_differential.rs`).

## No-claim boundaries

- No additional no-claim boundaries documented.
