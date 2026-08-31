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

- No `crates/emath-world-ir/tests/` directory and no inline `#[cfg(test)]` modules in `src/`. Conformance lives in the standalone `tests/emath-world-ir` package:
  - `tests/lib.rs`: World IR mutation matrix (`semantic_mutations_change_identity_and_presentation_does_not` — 12 semantic rows must change identity; name and symbol display must not) and input-order independence of canonical()/identity() (`canonical_form_is_input_order_independent`).
  - `tests/builtin.rs`: at least five world classes with deterministic, pairwise-distinct identities in stable roster order; provider rebuild determinism.
  - `tests/translation.rs`: input-region partitioning routes fast inside the guard and deoptimizes with a canonical receipt outside it; authority-aware dispatch serves best-effort requests through weak morphisms but deoptimizes authoritative requests while exact morphisms keep the fast path.

## No-claim boundaries

- Content identity is the bootstrap FNV-1a64, not a release cryptographic identity; production replaces it with the canonical cryptographic identity service.
- Worlds beyond the documented G0-G3 slice are recorded as typed deferred entries, never silently ignored.

## Absorbed module: `world_codegen_rust` (was `emath-world-codegen-rust`)

# emath-world-codegen-rust

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP).
- Deterministic parametric Rust world artifact generation (Semantic Genesis G3).
- Emits a self-contained, zero-dependency generated crate evaluating a fixed first-order term under free-symbolic, Boolean, and modular-17 worlds, plus a negative-control world whose join/times semantics are swapped.
- Output is the golden examples/generated/semantic-genesis-worlds.

## Public types and semantics

- WORLD_ABI_VERSION (1): version of the generated world ABI surface (generic World trait + declaration-specific SpecializedWorld trait + dispatcher). Every generated crate embeds the constant it was built against; the compile-lane manifest.json discloses it as world_abi_version.
- WorldSpec: a world whose implementation is generated; stable label (free_symbolic, boolean_algebra, modular_numeric) and a declared operator semantics map.
- CodegenRefusal: typed refusal with stable code (E-GEN-094) and a human-readable message.
- GeneratedPackage: crate name plus a relative-path to file-content map; write_to materializes all files under a directory.
- generate fn: emits the parametric crate for a term/signature/worlds, or refuses.
- SeedContract struct: seed-identity contract consumed by the xtask oracle. (not exhaustive; the rest of lib.rs is the embedded generated-crate template.)

## Invariants

- Label-based emission hardcodes a fixed per-label operator interpretation; default_operator_semantics must stay in lockstep with the apply implementations inside LIB_TEMPLATE.
- The generated crate carries a declaration-specific ABI (SpecializedWorld) derived from the source signature: one sym_<index> method per declared symbol at its exact arity (canonical signature order), a blanket delegation from World so the two surfaces cannot diverge, and an evaluate_specialized dispatcher whose unknown-symbol and wrong-arity paths are typed refusals.
- A declared operator meaning codegen cannot honor is refused (E-GEN-094, SURF-0008) rather than silently dropped.
- free_symbolic interprets operators structurally and requires an empty declared map.
- The negative-control swapped world is a real semantic mutation, pinned so no-op mutant delegation is killed.
- The emitted Cargo.toml template uses edition = "2024".

## Error model

- generate returns Err(CodegenRefusal) with code E-GEN-094 when declared semantics differ from the fixed per-label interpretation.
- write_to returns std::io::Result for filesystem failure.

## Determinism class

- Deterministic and byte-comparable: the file map is a BTreeMap, and rendering is parametric only over the given term/signature/labels.
- The generated crate is a golden; regeneration is byte-compared.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- Two inline `#[cfg(test)]` modules in `lib.rs` (exercising the embedded generated-crate template): `contract_tests` (`swapped_world_is_not_a_noop_mutation`, `swap_mutation_is_killed_on_other_operator_paths`, `dual_run_is_deterministic`) and `specialized_abi_tests` (`specialized_abi_agrees_with_generic_evaluation`, unknown-symbol and wrong-arity refusals). `SWAP_SEED_CONTRACT` (`SeedContract`) pins the double-run seed identity.
- `tests/world_codegen.rs` in the `tests/emath-world-ir` package drives the generated crate end to end. The committed golden `examples/generated/semantic-genesis-worlds` is the byte-compared fixture.

## Rollback and migration

- The ABI surface is versioned by WORLD_ABI_VERSION. Any change to the emitted trait shapes, method naming, or dispatch semantics bumps the constant; consumers that pin a version refuse crates generated against another.
- Rollback is regeneration: generated crates carry no hand edits (header says do not edit), so rolling back the generator (git revert of this crate) and re-running `emath compile --parametric` reproduces the previous ABI byte-exactly; the committed golden examples/generated/semantic-genesis-worlds is the fixture proving it.
- Migration between ABI versions is re-generation plus consumer recompile; there is no in-place migration of generated source, by design.

## No-claim boundaries

- Only the label-based G3 subset is generatable; all other worlds are typed refusals, never ignored.
- The generated crate name is fixed (semantic-genesis-worlds); arbitrary crate naming is not supported.
