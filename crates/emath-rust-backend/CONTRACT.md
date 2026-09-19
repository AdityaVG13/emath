# CONTRACT.md

## Purpose and layer

Rust backend: universal EMIR and artifact contracts to deterministic Rust via the rust-ir AST. Layer: `rust-ir` (per CRATE_MAP.md). The backend emits structural operations and universal control/data operations; mathematical meaning is not selected by feature names or domain-specific emitter branches. Semantic operations must arrive through `ApplyCapability` plus a materializable artifact contract. The backend generates one crate per admission: a struct plus constructor for stateful declarations, a free function (not a method on an empty struct) when there is no state and no constructors, an evaluation item per `evaluate <target>` goal, explicit step methods for `model` declarations, and `#[test]` functions for the `tests:` section. Every generated crate embeds the `emath-rt` kernel module verbatim (`mod emath_rt { ... }` from `emath_rt::SOURCE`) and remains std-only, `#![forbid(unsafe_code)]`, and byte-deterministic.

## Public types and semantics

- `BackendInput { package, crate_name, version }`: input to `generate()`.
- `BackendOutput { files, anchors, assumptions, module, receipts }`: relative path to file content (including `Cargo.toml` and `src/lib.rs`), source-map anchors, surfaced domain obligations, the rendered module for `CrateProfile::validate`, and one `ConstructionReceipt` per generated constructor (the obligation matrix the emitted code discharges).
- `BackendAnchor`: byte-range anchor into generated `src/lib.rs`.
- `BackendError`: typed backend failure (variant list below).

## Invariants

- Generated crates are std-only, `#![forbid(unsafe_code)]`, `#![allow(dead_code)]`, and byte-deterministic.
- Generated crates embed `mod emath_rt { ... }` (the verbatim `emath-rt` kernel source) with an outer `#[allow(dead_code)]` so hosts that strip inner attributes (an `include!` driver pattern) stay warning-free.
- The emitter is exhaustive over the contracted universal `EmirOp`; active backend modules contain no removed domain-op references. Obsolete domain/dual helper files remain unreferenced only because deletion is forbidden.
- The emitter never maps a mathematical feature name or legacy domain operation to a runtime function. A semantic operation without an `ApplyCapability` artifact contract fails as `MissingArtifactContract`.
- `ApplyCapability` dispatches only on generic cell class. Unsupported provider and intrinsic/native bindings fail as `UnsupportedBinding`; other applications without an executable artifact body fail as `MissingArtifactContract`. No identity, interpreter fallback, or compatibility shim is emitted.
- Generated manifest emits `edition = "2024"`, sanitized crate name/version; keywords and reserved identifiers are escaped (`type` to `type_`) and never emitted raw.
- A declaration with no `state` and no constructors emits a free `fn` per evaluate target (no `self`, no unit struct). Worked-example tests call that function directly.
- Constructors are controlled entry points: every `require` precondition and `ensure`/`invariant` postcondition is checked in generated code before a value escapes.
- Goals and tests attach by declared ids, never by span geometry.
- `model` declarations emit explicit `step_euler` and `step_rk4` step methods over `der_<state>` rates; models with `algebraic:` residual equations render the shared typed residual step program and its authored Newton cells (forward-difference Jacobian, Gaussian elimination, 30 iterations, 1e-9 solve tolerance, 1e-6 convergence check), returning `Result<Self, String>` that errors on non-convergence instead of inventing a value; no silent omission. Algebraic unknowns are fields of `Self` (extended DAE state). After the differential update each step re-solves them at the accepted state so the algebraic residual at the returned point is ~0 (index-1 projection), matching `emath simulate`.
- Program literals retain typed captures. Literal frames bind their own inputs and state. Nested frames and programs disable outer register inlining, so outer substitutions cannot change inner bindings.
- Text formatting uses positional operands and preserves literal braces. Statistical estimates use the authored `Estimate` record layout.
- `SameVector`, `SameMatrix`, and `SameTensor` result signatures retain their dense carrier kind. Authored guards retain shape validation.
- Current subset: one constructor and one evaluate goal per declaration.
  `Float64` is `f64`; `Int`/`Nat` are exact `i64` (`ConstI64` is not
  widened through f64). Mixed Int/Float64 arithmetic widens to `f64`.
  Rank-3+ values are `emath_rt::Tensor`. Index/slice emit checked
  `emath-rt` helpers (`vec_index_checked`, `tensor_slice_as_*`); evaluate
  methods that can fault return `Result<T, String>` instead of panicking
  `[]`.

## Error model

`BackendError` enum: `NoEvaluateGoal`, `UnknownTarget`, `MissingInput`, `MissingGiven`, `UnsupportedType`, `MultipleConstructors`, `UnsupportedBinding`, `MissingArtifactContract`, `Lowering`. Provider/native absence and semantic operations that bypass the universal artifact seam are typed backend refusals. All variants implement `Display`/`Error`. Profile validation surfaces E-CODEGEN-002/`E-CODEGEN-004` on the exact rendered module.

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
  `vector_index_codegen_uses_checked_helper_not_index`,
  `integer_product_fold_uses_i64_kernel`,
  `sign_zero_uses_mathematical_sgn`,
  `folded_nonfinite_constants_emit_valid_rust`

Legacy domain-render assertions are not part of this contract. They must be migrated outside this crate to construct `ApplyCapability` programs with executable artifact contracts.

## No-claim boundaries

- Only the current subset is generated: a declaration needs exactly one evaluate goal and supports one constructor. Admitted types: `Float64`, `Bool`, `Int`, `Nat`, vectors/matrices/tensors, authored records, host opaques. Other types yield `UnsupportedType`.
- Capability generation reads the verified installed distribution. With no native binding, an installed reference program supplies the body. Argument count must match. Separate cell guards and result guards still refuse rather than disappear. Unsupported instructions retain their existing refusals. Native bindings still require a matching artifact contract; the backend does not recover legacy domain dispatch.
- No certification power; generated crates carry invariants but the backend itself performs no evidence checks.

## Absorbed module: `rust_ir` (was `emath-rust-ir`)

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
- `Expr::F64` renders finite values with Debug (`1.0`); NaN/Inf use `f64::from_bits(0x…)` so generated crates compile (Debug `NaN`/`inf` are not Rust literals).
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

No `crates/`-side tests for the profile surface. The former
`tests/emath-rust-ir` package's suite lives in `tests/emath-rust-backend`:
`tests/profile_validate.rs` exercises `CrateProfile::validate`
(`E-CODEGEN-002`/`E-CODEGEN-003`/`E-CODEGEN-004`).

## No-claim boundaries

No additional no-claim boundaries documented.

## Shared authored carriers

Generated input and state loads borrow non-Copy carriers. Repeated record
arguments do not move the source value. Existing owning boundaries
materialize returned values and record fields. Input-dependent quantized
self-products exercise this rule through compiled search.

## Matrix carrier

Generated `Matrix<Float64>` values use `emath_rt::Matrix`, not nested vectors.
The carrier stores both dimensions and flat row-major data. It preserves
`0×N` and `N×0` without allocating rows. `Matrix::new(rows, cols, data)`
checks the dimension product and data length. `get(row, col)` checks both
indices. The constructor does not perform mathematical operations.
The reference operators `matrix_rows`, `matrix_cols`, `matrix_at`, and
`matrix_pack` use this carrier. Their checked operations return errors.
