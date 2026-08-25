# CONTRACT.md

## Purpose and layer

Rust backend: EMIR to deterministic Rust via the rust-ir AST. Layer: `rust-ir` (per CRATE_MAP.md). Phase 1 generates one crate per admission: a struct plus constructor for stateful declarations, a free function (not a method on an empty struct) when there is no state and no constructors, an evaluation item per `evaluate <target>` goal, explicit step methods for `model` declarations, and `#[test]` functions for the `tests:` section. Every generated crate embeds the `emath-rt` kernel module verbatim (`mod emath_rt { ... }` from `emath_rt::SOURCE`); generated expressions call `emath_rt::<kernel>(...)`, so artifacts are self-contained with no external dependencies. Everything is std-only, `#![forbid(unsafe_code)]`, and byte-deterministic.

## Public types and semantics

- `BackendInput { package, crate_name, version }`: input to `generate()`.
- `BackendOutput { files, anchors, assumptions, module, receipts }`: relative path to file content (including `Cargo.toml` and `src/lib.rs`), source-map anchors, surfaced domain obligations, the rendered module for `CrateProfile::validate`, and one `ConstructionReceipt` per generated constructor (the obligation matrix the emitted code discharges).
- `BackendAnchor`: byte-range anchor into generated `src/lib.rs`.
- `BackendError`: typed backend failure (variant list below).

## Invariants

- Generated crates are std-only, `#![forbid(unsafe_code)]`, `#![allow(dead_code)]`, and byte-deterministic.
- Generated crates embed `mod emath_rt { ... }` (the verbatim `emath-rt` kernel source) with an outer `#[allow(dead_code)]` so hosts that strip inner attributes (e.g. the demo-host `include!` driver) stay warning-free; math kernels live in exactly one place (`emath-rt`).
- Generated manifest emits `edition = "2024"`, sanitized crate name/version; keywords and reserved identifiers are escaped (`type` to `type_`) and never emitted raw.
- A declaration with no `state` and no constructors emits a free `fn` per evaluate target (no `self`, no unit struct). Worked-example tests call that function directly.
- Constructors are controlled entry points: every `require` precondition and `ensure`/`invariant` postcondition is checked in generated code before a value escapes.
- Goals and tests attach by declared ids, never by span geometry.
- `model` declarations emit explicit `step_euler` and `step_rk4` step methods over `der_<state>` rates; causalized implicit DAEs (Newton-solved residuals) are refused with `BackendError::Lowering` ("use `emath simulate`") — no silent omission.
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

Integration tests in `tests/emath-rust-backend/tests/lib.rs`:
- `keyword_declaration_name_is_escaped_in_generated_rust`
- `keyword_crate_name_is_escaped_in_manifest`
- `stateless_declaration_emits_free_function`
- `generated_constructor_carries_its_construction_receipt`,
  `expect_less_example_generates_computation_without_assert`,
  `constant_only_declaration_generates_parameterless_method`,
  `chained_definitions_emit_let_bindings_in_source_order`,
  `causalized_model_is_refused_by_rust_lowering`,
  `model_emits_explicit_step_methods`

## No-claim boundaries

- Only the Phase 1 subset is generated: a declaration needs exactly one evaluate goal and supports one constructor; any other type than `Float64`/`Bool` yields `UnsupportedType`.
- No certification power; generated crates carry invariants but the backend itself performs no evidence checks.
