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
- `model` declarations emit explicit `step_euler` and `step_rk4` step methods over `der_<state>` rates; models with `algebraic:` residual equations emit the same two steps embedding the interpreter's causalized Newton solve (forward-difference Jacobian, Gaussian elimination, 30 iterations, 1e-9 solve tolerance, 1e-6 convergence check), returning `Result<Self, String>` that errors on non-convergence instead of inventing a value — no silent omission. Algebraic unknowns are fields of `Self` (extended DAE state). After the differential update each step re-solves them at the accepted state so the algebraic residual at the returned point is ~0 (index-1 projection), matching `emath simulate`.
- Phase 1 subset: one constructor and one evaluate goal per declaration.
  `Float64` is `f64`; `Int`/`Nat` are exact `i64` (`ConstI64` is not
  widened through f64). Mixed Int/Float64 arithmetic widens to `f64`.
  Rank-3+ values are `emath_rt::Tensor`. Index/slice emit checked
  `emath-rt` helpers (`vec_index_checked`, `tensor_slice_as_*`); evaluate
  methods that can fault return `Result<T, String>` instead of panicking
  `[]`.

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
  `causalized_model_emits_newton_step_methods`,
  `model_emits_explicit_step_methods`,
  `const_i64_past_f64_mantissa_stays_i64`,
  `factorial_twenty_calls_i64_kernel`,
  `einsum_codegen_calls_rt_kernel_not_panic_stub`,
  `vector_index_codegen_uses_checked_helper_not_index`,
  `tensor_face_slice_codegen_is_not_a_clone`,
  `integer_product_fold_uses_i64_kernel`,
  `sign_zero_uses_mathematical_sgn`,
  `folded_nonfinite_constants_emit_valid_rust`

## No-claim boundaries

- Only the Phase 1 subset is generated: a declaration needs exactly one evaluate goal and supports one constructor. Admitted types: `Float64`, `Bool`, `Int`, `Nat`, vectors/matrices/tensors, host opaques. Other types yield `UnsupportedType`.
- No certification power; generated crates carry invariants but the backend itself performs no evidence checks.
