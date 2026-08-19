# CONTRACT.md

## Purpose and layer

Rust backend: EMIR to deterministic Rust via the rust-ir AST. Layer: `rust-ir` (per CRATE_MAP.md). Phase 1 generates one crate per admission: a struct per declaration, a constructor with enforced invariants, an evaluation method per `evaluate <target>` goal, and `#[test]` functions for the `tests:` section. Everything is std-only, `#![forbid(unsafe_code)]`, and byte-deterministic.

## Public types and semantics

- `BackendInput { package, crate_name, version }`: input to `generate()`.
- `BackendOutput { files, anchors, assumptions, module, receipts }`: relative path to file content (including `Cargo.toml` and `src/lib.rs`), source-map anchors, surfaced domain obligations, the rendered module for `CrateProfile::validate`, and one `ConstructionReceipt` per generated constructor (the obligation matrix the emitted code discharges).
- `BackendAnchor`: byte-range anchor into generated `src/lib.rs`.
- `BackendError`: typed backend failure (variant list below).

## Invariants

- Generated crates are std-only, `#![forbid(unsafe_code)]`, `#![allow(dead_code)]`, and byte-deterministic.
- Generated manifest emits `edition = "2024"`, sanitized crate name/version; keywords and reserved identifiers are escaped (`type` to `type_`) and never emitted raw.
- Constructors are controlled entry points: every `require` precondition and `ensure`/`invariant` postcondition is checked in generated code before a value escapes.
- Goals and tests attach by declared ids, never by span geometry.
- Phase 1 subset: one constructor and one evaluate goal per declaration, strict-f64 types only.

## Error model

`BackendError` enum: `NoEvaluateGoal`, `UnknownTarget`, `MissingInput`, `MissingGiven`, `UnsupportedType`, `MultipleConstructors`, `Lowering`. All implement `Display`/`Error`. Profile validation surfaces E-CODEGEN-002/`E-CODEGEN-004` on the exact rendered module.

## Determinism class

Deterministic and byte-comparable. Same `BackendInput` produces identical generated crate bytes repo-wide; `value_expr` materializes ops deterministically via `__e<i>` temporaries.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None in the backend itself (`#![forbid(unsafe_code)]`; workspace lint forbids unsafe_code). Generated crates also carry `#![forbid(unsafe_code)]`.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

Inline `#[cfg(test)] mod tests` in `lib.rs`:
- `keyword_declaration_name_is_escaped_in_generated_rust`
- `keyword_crate_name_is_escaped_in_manifest`

## No-claim boundaries

- Only the Phase 1 subset is generated: a declaration needs exactly one evaluate goal and supports one constructor; any other type than `Float64`/`Bool` yields `UnsupportedType`.
- No certification power; generated crates carry invariants but the backend itself performs no evidence checks.
