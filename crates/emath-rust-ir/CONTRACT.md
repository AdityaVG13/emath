# CONTRACT.md

## Purpose and layer

Structured Rust IR: a target AST with deterministic rendering, identifier hygiene and byte-range anchors for source maps. Layer: `ir` (per CRATE_MAP.md). No string-concatenated generation outside this renderer.

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `HostBinding`, `HostMethod`, `HostTraitSpec`, `HostBindError` (module `host`): `generate_binding`, `fallback_binding`, `append_to_module`, `check_version`.
- `CrateProfile`, `ProfileProblem` (module `profiles`): `parse_profile`.
- `FileSet`, `Anchor`, `RenderResult` (module `render`): `render_module`, `render_file_set`, `render_file_set_partitioned`, `render_generics`, `coverage_gaps`.
- Module `ast`: full AST item types (`Module`, `Item`, `StructDef`, `FnDef`, `ImplDef`, `EnumDef`, `Expr`, `Stmt`, `Ty`, etc.) and helpers `escape_ident`, `snake_case`, `RUST_KEYWORDS`.

## Invariants

- All generation goes through the structured AST and its renderer; no string-concatenated Rust emission elsewhere.
- Identifier hygiene: Rust keywords and reserved names are escaped, never emitted raw.
- Byte-range anchors are produced for source maps (`Anchor`, `coverage_gaps`).
- Profile validation refuses unknown ranges (E-CODEGEN-003), unsafe code in a safe profile (E-CODEGEN-002) and public items without a source-map anchor (E-CODEGEN-004).

## Error model

`HostBindError` (stable `E-HOST-001`/`E-HOST-002`): unknown/incompatible binding refusal, typed rather than silent stubs. `ProfileProblem` carries stable codes `E-CODEGEN-002`/`E-CODEGEN-003`/`E-CODEGEN-004`. `RenderResult` reports coverage gaps as data, not panics.

## Determinism class

Deterministic. Rendering is byte-stable given the same AST; no RNG or wall-clock input.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

None on disk currently. No `tests/` directory and no inline `#[cfg(test)]` module in `lib.rs`.

## No-claim boundaries

No additional no-claim boundaries documented.
