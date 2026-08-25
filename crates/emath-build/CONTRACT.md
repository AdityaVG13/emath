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
