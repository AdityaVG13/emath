# emath-world-ir

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP).
- Provider-neutral World IR and meaning-hole structures.
- Canonical carrier of admitted worlds for the genesis pipeline; consumed by codegen, portfolio, holes, and calibration.

## Public types and semantics

- WORLD_IR_SCHEMA ("emath.world-ir") and WORLD_IR_VERSION (1): schema identity constants; bump the version on any layout or canonical-form change. Provider references are string ids only; provider-native types never appear in the schema.
- WorldIr struct: version, name, signature, carriers, symbols, operators, constructors, laws, effects, holes, capabilities; identity() and canonical() methods. The seven worlds-contract components are carriers, symbols, signature, meanings (operators), constructors, laws, and effects; effects are declared names (C10: never ambient; empty list means pure).
- WorldId newtype (wrapping u64): content identity placeholder for an admitted world.
- CarrierDef: name and canonical type expression.
- SymbolDef: id, display glyph, fixity, optional precedence, type scheme.
- OperatorDef: symbol being interpreted, semantics, meaning origin.
- MeaningHole / MeaningHoleId / MeaningHoleKind / MeaningHoleState: explicit unresolved semantic requirements with category and lifecycle state.
- OperatorSemantics / MeaningOrigin / Fixity enums: executable semantics, origin, and surface fixity.
- builtin module: WorldClass (eight classes: free-term, finite-table, commutative-monoid, boolean-lattice, integer-ring, cyclic-group, matrix, graph; WorldClass::ALL is the stable roster) and builtin_worlds(). The matrix world deliberately omits a multiplication-commutativity law (wrong law a checker must refute); the graph world is the idempotent union algebra.
- translation module: WorldMorphism, StrictFastPortfolio, DeoptReason, FastPathGuard. Dispatch is region- and authority-aware: select_world routes by the guarded input region; select_world_with_authority additionally deoptimizes (DeoptReason::AuthorityDegraded) when the caller requires full authority and a used symbol's obligation relation does not transport it (PreservationRelation::transports_authority: exact and refinement transport; approximation, simulation, and observational equivalence degrade, matching the portfolio's conservative authority cap).
- fnv1a64 fn. (not exhaustive.)

## Invariants

- A WorldId binds semantic content, not incidental labels: canonical() excludes the display name and symbol display glyphs from identity.
- canonical() sorts carriers, symbols, operators, laws, effects, capabilities, constructors, and holes so the identity is independent of insertion order.
- Every semantic component participates in identity: the mutation matrix test pins one row per component (semantic mutation changes WorldId; presentation-only mutation does not).
- Holes are typed (category plus state) and never silently dropped from a world.

## Error model

- No errors emitted; the crate provides pure value structures and hashing.
- fnv1a64 is infallible over any byte slice.

## Determinism class

- Deterministic: canonical() yields a stable, byte-comparable seed form; identity() is its FNV-1a64 hash.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- lib.rs mutation_matrix_tests: World IR mutation matrix (12 semantic rows must change identity; name and symbol display must not) and input-order independence of canonical()/identity().
- builtin.rs builtin_world_tests: at least five world classes with deterministic, pairwise-distinct identities in stable roster order; provider rebuild determinism.
- translation.rs dispatch_tests: input-region partitioning routes fast inside the guard and deoptimizes with a canonical receipt outside it; authority-aware dispatch serves best-effort requests through weak morphisms but deoptimizes authoritative requests (`authority:op:simulation` receipt) while exact morphisms keep the fast path.
- Lane: cargo test -p emath-world-ir --lib.

## No-claim boundaries

- Content identity is the bootstrap FNV-1a64, not a release cryptographic identity; production replaces it with the canonical cryptographic identity service.
- Worlds beyond the documented G0-G3 slice are recorded as typed deferred entries, never silently ignored.
