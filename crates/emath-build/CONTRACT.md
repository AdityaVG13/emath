# emath-build CONTRACT

## Purpose and layer
- Build-step backend (CRATE_MAP tier: sema/artifact).
- Source builds consume one semantic-session check and the shared constructor
  crate emitter. `build_file` retains the full source path and honors its
  package edition; imports, entries and embedded sources come from that check.
  Relative paths also work for engine-backed entries: embedded import keys
  and the root use the same absolute-path policy.
  Ancestor edition-manifest lookup includes the working directory's parents
  for relative source paths, without canonicalizing symlinks.
  An already-elaborated `SemanticPackage` uses the separate historical
  plan/artifact pipeline through `build_package`.
- `compile_direct_module` keeps the V3 build-script contract (spec file to output dir). `build_text` and `build_file` expose the full report for hosts and the CLI. `build_package` is the shared artifact path for programmatic and macro-expanded sources.
- `build_into_out_dir` / `emit_rerun_if_changed` are build.rs helpers (`AGENT_BUILD_PROMPT`).

## Public types and semantics
- `BuildOptions`: on `build_package`, `verify_generated_crate` controls whether
  the staged crate runs `cargo test` before publish. Constructor source builds
  do not execute those historical options.
- `BuildReport`: artifact_dir, artifact_id, package_id, crate_name, plan_ids, assumptions, exports, refusal_codes. Empty refusal_codes on success.
- Evidence honesty: admission claims are E1/`static-semantics`; cargo-test verification claims are E3/`codegen` only when verification actually ran (otherwise `not-run` at E0). Manifest `evidence_level` is the delivered bar. A goal requiring a higher bar than delivered is refused with `E-EVID-103`.
- `BuildError`: enum over ReadFailed, AdmittedWithErrors(String codes), Backend, VerifyFailed, Artifact, Io.
- Dependency policy (module `deps`): `DepPolicy`, `DepPlan`, `DepRequest`, `CargoDependency`, `DepError`, `DepSource`, `RuntimeKind`, `TargetKind`; entry points `check_declared`, `plan_dependencies`, `requests_for`.
- Build script support (module `script`): `ScriptLock`, `ScriptReport`, `ScriptError`, `locked_build_script`.
  With `allow_network = false`, engine-backed source crates refuse
  `E-CODEGEN-008` before writing outputs: their registry dependencies are
  pinned but not vendored. Self-contained source crates remain allowed.
  This is a dependency-source policy, not a check of the caller's Cargo cache.
- Metrics (module `metrics`): `MetricsCollector` (named phase durations +
  counters), `BENCHMARK_RECEIPT_SCHEMA` (`emath.benchmark-receipt`),
  `BENCHMARK_RECEIPT_VERSION` (1). Receipt encoding is deterministic for fixed
  measurements; durations vary and never escalate authority. Constructor
  source builds do not collect these metrics or write a benchmark receipt.
- `COMPILER_DESCRIPTOR`: compiler identity string (`emath-phase1/<version>`).
- (not exhaustive: free functions `build_file`, `build_text`, `build_package`, `run_cargo_timed`.)

## Generated-file reuse

`write_generated_file` preserves identical files and mtimes. Changed or absent
contents use the ordinary write path; failed reads also fall back to writing.
CLI emission, compiled probes, and native exports share this helper, allowing
Cargo reuse in a stable output directory. This is not a binary cache and does
not promise cross-directory reuse or concurrent publication isolation.

## Invariants
- Manifest/profile/claim verification below describes `build_package`'s
  historical staged pipeline, not constructor source-build fingerprints.
- Typed refusal: any admission error (`AdmittedWithErrors`) means no artifact and no half-built crate.
- The single artifact identity is the manifest-body hash (`manifest_identity`) frozen over the resolved manifest; the independent checker recomputes the identical value. `stage` fingerprints are never advertised as identity.
- Generated module must be profile-safe (E-CODEGEN-002) and every public item source-anchored (E-CODEGEN-004) before staging.
- Verification honesty (E-TLT-012 / E-TLT-013): `--verify` refuses a generated crate with no `#[test]` functions rather than reporting a vacuous pass; unverified steps are claimed `not-run`.
- Records (admitted, generated-crate) are claims that state exactly what ran, with the checker used.
- Language publication has cumulative `Framework`, `CandidateImage`, and
  `StableLanguage` gates. Stable publication refuses incomplete projections,
  missing live/independent evidence, split authority, blocking holes, invalid
  migrations, stale views, and unauthorized semantic change.

## Error model
- `BuildError`, a typed enum with Display text (codes embedded in messages for refusals).
- Admission diagnostics carry stable E-* codes (sorted, deduped) in `AdmittedWithErrors`; `compile_direct_module` maps them to a refusal string.
- `cargo test` under budget: process execution and output drain share the
  timeout; an unfinished drain also yields `E-RES-120` after the parent exits.
- `run_cargo_timed` returns `Result<Output, String>` with concrete spawn/wait/kill messages.
- Generated Cargo commands default to one build job and one libtest thread, even outside the repository. `CARGO_BUILD_JOBS` and `RUST_TEST_THREADS` inherit caller environment values; explicit per-command settings (including removal) take precedence. These are concurrency defaults, not CPU-percentage or memory quotas.

## Determinism class
- Deterministic: content ids, plan records, source map, manifest and evidence all derive from a fixed pipeline with no wall-clock or randomness input.

## Cancellation behavior
- `--verify` Cargo runs under a wall-clock budget via `run_cargo_timed`.
  Safe nonblocking Unix socket pairs replace blocking output-reader threads.
  The budget includes spawn and stdout/stderr completion. A live child is
  killed and reaped on timeout; after reap, retained descendant output handles
  are closed without signalling the reaped pid. No detached reader threads
  remain. No other async cancellation surface.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]` and the workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml.

## Conformance tests
- Workspace suite `tests/emath-build`:
  - `run_cargo_timed` (`tests/lib.rs`, `run_cargo_timed_tests`): live child
    timeout, exited-parent inherited-output timeout, nested descendant retained
    output, in-budget descendant stdout/stderr capture, and ordinary completion.
    The child stays in the terminal process group so Ctrl-C reaches it.
  - `metrics` (`tests/metrics.rs`): receipt format byte-stable for the same recorded values (sorted keys, schema/version pinned); collectors accumulate re-entered phases and counters.

## No-claim boundaries
- Timed subprocess stdio currently requires Unix socket pairs; other hosts
  refuse before spawning. Direct-child cleanup before killing a live parent
  remains best effort, not recursive descendant containment. Reparented or
  escaped descendants may survive, but cannot extend output drain indefinitely.
- Constructor `build_text` / `build_file` write the complete emitted crate,
  but do not run the historical `BuildOptions` verification/probe paths.
  They do not claim atomic publication or an independent artifact seal.
  Their artifact id remains the generated library's content fingerprint.
- Provider registry and provider-specific planning live upstream (`emath-plan`, `emath-provider-api`); `build` does not choose providers.
- `verify_crate` runs only when `verify_generated_crate` is set; otherwise the generated-crate claim is `not-run`, never overclaimed.
- Identity uses the keep-gate content-id scheme (manifest-body hash), not release-grade crypto.
