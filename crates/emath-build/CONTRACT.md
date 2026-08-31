# emath-build CONTRACT

## Purpose and layer
- Build-step backend (CRATE_MAP tier: sema/artifact).
- Pipeline: `.emath` text or an elaborated `SemanticPackage` runs check, plan, EMIR Rust backend, staged artifact, atomic publish, then independent verification.
- `compile_direct_module` keeps the V3 build-script contract (spec file to output dir). `build_text` and `build_file` expose the full report for hosts and the CLI. `build_package` is the shared artifact path for programmatic and macro-expanded sources.
- `build_into_out_dir` / `emit_rerun_if_changed` are build.rs helpers (`AGENT_BUILD_PROMPT`).

## Public types and semantics
- `BuildOptions`: `verify_generated_crate` controls whether the staged crate runs `cargo test` before publish.
- `BuildReport`: artifact_dir, artifact_id, package_id, crate_name, plan_ids, assumptions, exports, refusal_codes. Empty refusal_codes on success.
- Evidence honesty: admission claims are E1/`static-semantics`; cargo-test verification claims are E3/`codegen` only when verification actually ran (otherwise `not-run` at E0). Manifest `evidence_level` is the delivered bar. A goal requiring a higher bar than delivered is refused with `E-EVID-103`.
- `BuildError`: enum over ReadFailed, AdmittedWithErrors(String codes), Backend, VerifyFailed, Artifact, Io.
- Dependency policy (module `deps`): `DepPolicy`, `DepPlan`, `DepRequest`, `CargoDependency`, `DepError`, `DepSource`, `RuntimeKind`, `TargetKind`; entry points `check_declared`, `plan_dependencies`, `requests_for`.
- Build script support (module `script`): `ScriptLock`, `ScriptReport`, `ScriptError`, `locked_build_script`.
- Metrics (module `metrics`): `MetricsCollector` (named phase durations + counters, accumulating), `BENCHMARK_RECEIPT_SCHEMA` (`emath.benchmark-receipt`), `BENCHMARK_RECEIPT_VERSION` (1). `build_text` records `check_plan` / `artifact_pipeline` durations and `plan_count` / `diagnostics` / `compile_success` / `artifact_bytes` counters, and writes `benchmark-receipt.json` next to the publish tree (never inside the identity-verified artifact package). The receipt format is deterministic (fixed schema/version, sorted `duration_ns.*` / `count.*` keys); the recorded durations are measurements and vary run to run. Receipts are evidence objects and never escalate authority.
- `COMPILER_DESCRIPTOR`: compiler identity string (`emath-phase1/<version>`).
- (not exhaustive: free functions `build_file`, `build_text`, `build_package`, `run_cargo_timed`.)

## Invariants
- Typed refusal: any admission error (`AdmittedWithErrors`) means no artifact and no half-built crate.
- The single artifact identity is the manifest-body hash (`manifest_identity`) frozen over the resolved manifest; the independent checker recomputes the identical value. `stage` fingerprints are never advertised as identity.
- Generated module must be profile-safe (E-CODEGEN-002) and every public item source-anchored (E-CODEGEN-004) before staging.
- Verification honesty (E-TLT-012 / E-TLT-013): `--verify` refuses a generated crate with no `#[test]` functions rather than reporting a vacuous pass; unverified steps are claimed `not-run`.
- Records (admitted, generated-crate) are claims that state exactly what ran, with the checker used.

## Error model
- `BuildError`, a typed enum with Display text (codes embedded in messages for refusals).
- Admission diagnostics carry stable E-* codes (sorted, deduped) in `AdmittedWithErrors`; `compile_direct_module` maps them to a refusal string.
- `cargo test` under budget: running past timeout kills the child and yields `E-RES-120`.
- `run_cargo_timed` returns `Result<Output, String>` with concrete spawn/wait/kill messages.

## Determinism class
- Deterministic: content ids, plan records, source map, manifest and evidence all derive from a fixed pipeline with no wall-clock or randomness input.

## Cancellation behavior
- `--verify` carpocs run under a wall-clock budget via `run_cargo_timed`; a child still running past the timeout is killed and reported as `E-RES-120`, so cargo cannot block a session forever. No other async cancellation surface.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]` and the workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml.

## Conformance tests
- Workspace suite `tests/emath-build`:
  - `run_cargo_timed` (`tests/lib.rs`, `run_cargo_timed_tests`): a live child past the budget is SIGKILL'd after its direct children (cargo, then rustc) and reported as `E-RES-120`; a child that already exited is not reported as a timeout. The child stays in the terminal process group so Ctrl-C reaches it.
  - `metrics` (`tests/metrics.rs`): receipt format byte-stable for the same recorded values (sorted keys, schema/version pinned); collectors accumulate re-entered phases and counters.

## No-claim boundaries
- Provider registry and provider-specific planning live upstream (`emath-plan`, `emath-provider-api`); `build` does not choose providers.
- `verify_crate` runs only when `verify_generated_crate` is set; otherwise the generated-crate claim is `not-run`, never overclaimed.
- Identity uses the keep-gate content-id scheme (manifest-body hash), not release-grade crypto.

## Absorbed module: `builder` (was `emath-builder`)

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
