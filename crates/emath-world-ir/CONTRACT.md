# emath-world-ir

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP).
- Provider-neutral World IR and meaning-hole structures.
- Canonical carrier of admitted worlds for the genesis pipeline; consumed by codegen, portfolio, holes, and calibration.

## Public types and semantics

- WorldIr struct: version, name, signature, carriers, symbols, operators, constructors, laws, holes, capabilities; identity() and canonical() methods.
- WorldId newtype (wrapping u64): content identity placeholder for an admitted world.
- CarrierDef: name and canonical type expression.
- SymbolDef: id, display glyph, fixity, optional precedence, type scheme.
- OperatorDef: symbol being interpreted, semantics, meaning origin.
- MeaningHole / MeaningHoleId / MeaningHoleKind / MeaningHoleState: explicit unresolved semantic requirements with category and lifecycle state.
- OperatorSemantics / MeaningOrigin / Fixity enums: executable semantics, origin, and surface fixity.
- fnv1a64 fn and builtin, translation modules. (not exhaustive.)

## Invariants

- A WorldId binds semantic content, not incidental labels: canonical() excludes the display name and symbol display glyphs from identity.
- canonical() sorts carriers, symbols, operators, laws, capabilities, constructors, and holes so the identity is independent of insertion order.
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

- None listed: no tests/ directory and no #[cfg(test)] module declared in lib.rs.

## No-claim boundaries

- Content identity is the bootstrap FNV-1a64, not a release cryptographic identity; production replaces it with the canonical cryptographic identity service.
- Worlds beyond the documented G0-G3 slice are recorded as typed deferred entries, never silently ignored.
