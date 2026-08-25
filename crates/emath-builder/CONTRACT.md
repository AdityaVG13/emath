# emath-builder CONTRACT

## Purpose and layer
- Programmatic model builder (CRATE_MAP tier: build).
- Builds the same semantic representation (SIR package + GIR goals) that `.emath` text admission produces, without a source file. Hosts and the laboratory compose models in Rust.
- Also hosts the macro rendering half shared by `emath-macro`: parsing (`macro_expand`) and runtime artifact building (`build_from_source`, `build_from_model`) through the exact `emath-build` artifact path.
- Phase 1 supports the strict-f64 subset with one declaration; the constructor surface admits overloads, factories, delegation, defaults, derived fields, postconditions and typed errors without bypassing schema or constructor admission.

## Public types and semantics
- `BuilderModel`: the collected model (name, kind, inputs, outputs, state, constructors, derived, definitions, goals, tests, compile); `Default` and builder-method constructible.
- `ModelBuilder` trait: chainable `custom`, `kind`, `input`, `output`, `constructor`, `define`, `goal`, `test`, `compile`, terminating `build() -> Result<SemanticPackage, BuilderError>`.
- `ConstructorModel`: parameters, defaults, preconditions, assignments, postconditions, `error_type`, delegation `delegate`, and public flag.
- `Expression` enum: Float, Int, Bool, Symbol, Unary, Binary, Constraint; with `UnaryOp`, `BinaryOp`, `CmpOp`.
- `BuilderError(pub String)`: typed builder failure.
- `KindRef` (Function, Policy); support types `TypeKind`, `GoalModel`, `TestModel`, `CompileModel`, `BuilderPolicy`.
- Macro surface: `MacroExpansion` (source + identity), `MacroError` (code + message), `macro_expand`, `build_from_source`.
- (not exhaustive.)

## Invariants
- Lowering emits the same SIR package and admission path as text; it never bypasses schema or constructor admission.
- Derived fields must be outputs (E-NAME-024).
- Compile spec defaults to `rust`/`library`/`StrictF64`/`ForbidUnsafe`; anything else is outside Phase 1 (E-CODEGEN-012).
- Constructor admission: policies require a public `new` (E-CTOR-031); functions cannot carry constructors (E-KIND-010); primary must be `new` (E-CTOR-036); no duplicate `new` (E-CTOR-034). Defaults only for declared params (E-CTOR-039), no state reads while constructing (E-CTOR-033), exact state coverage (E-CTOR-030 / E-CTOR-035), delegation to declared constructors only (E-CTOR-037 / E-CTOR-038).
- Macro input must be a single string literal; token text is parsed, never concatenated (E-CODEGEN-011).

## Error model
- `BuilderError`, a string-wrapped typed error; contract codes are embedded in messages (E-CTOR-*, E-KIND-010, E-CODEGEN-012, E-NAME-024).
- `build_from_model` / `build_from_source` propagate `emath_build::BuildError` wrapped in `BuilderError`.
- `MacroError` carries a stable code (`E-CODEGEN-011`).

## Determinism class
- Deterministic: builder model lowering is seed- and clock-free; `MacroExpansion.identity` is a deterministic content id over source text.

## Cancellation behavior
- Not applicable: std-only synchronous crate; no cancellation surface (artifact verification, when invoked, defers to `emath-build`'s timed cargo runner).

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml.

## Conformance tests
- Workspace integration suite `tests/emath-builder` (`tests/lib.rs`): `builder_model_tests_surface_on_declaration_tests`, `builder_model_goals_surface_on_declaration_goals`. No `tests/` directory on disk in the crate.

## No-claim boundaries
- Single-declaration strict-f64 subset only; multi-declaration and other numeric/kind profiles are not supported here.
- The builder shared the kind schema with the compiler; `kind_schema()` reflects `core_policy`/`core_function` plus an optional rendered predicate, not newly invented kinds.
- Macro expansion is a compile-time convenience; it performs no I/O and touches no files.
