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

- #[cfg(test)] modules in lib.rs: contract_tests, seed_contract_tests, unused_worldir_tests, specialized_abi_generation_tests (declaration-driven ABI shape + version embedding).
- Generated-crate tests (emitted into the golden): contract_tests (swap negative control) and specialized_abi_tests (specialized-vs-generic differential, unknown-symbol and wrong-arity refusals).
- No integration tests/ directory.

## Rollback and migration

- The ABI surface is versioned by WORLD_ABI_VERSION. Any change to the emitted trait shapes, method naming, or dispatch semantics bumps the constant; consumers that pin a version refuse crates generated against another.
- Rollback is regeneration: generated crates carry no hand edits (header says do not edit), so rolling back the generator (git revert of this crate) and re-running `emath compile --parametric` reproduces the previous ABI byte-exactly; the committed golden examples/generated/semantic-genesis-worlds is the fixture proving it.
- Migration between ABI versions is re-generation plus consumer recompile; there is no in-place migration of generated source, by design.

## No-claim boundaries

- Only the label-based G3 subset is generatable; all other worlds are typed refusals, never ignored.
- The generated crate name is fixed (semantic-genesis-worlds); arbitrary crate naming is not supported.
