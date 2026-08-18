# emath-cli CONTRACT

## Purpose and layer
- Command-line application (CRATE_MAP tier: sema/build/lab-core).
- Commands: `check`, `plan`, `planner`, `build`, `artifact check/battery`, `import modelica`, `architecture`, plus Semantic Genesis (`parse`, `signature`, `genesis`, `compile --parametric`, `world show`, `portfolio show`) and tooling commands (`new`, `fmt`, `explain`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider`, `fork`, `agent`, `help [<command>]`, `version`, `capabilities`, `robot-docs`).
- Exit codes: 0 success, 1 refusal/diagnostic, 2 usage or io error (`EXIT_OK`, `EXIT_REFUSED`, `EXIT_USAGE`).
- `run(&[String]) -> u8` is the testable entry behind `main`; the generated crate builds an agent envelope (`emath.agent`) over the same admission/plan/build paths as interactive commands.
- Registers the in-tree static `native.rust` capability so the generic planner serves the same goals as the native pipeline.

## Public types and semantics
- Constants `EXIT_OK` (0), `EXIT_REFUSED` (1), `EXIT_USAGE` (2).
- `run(&[String]) -> u8`: command dispatch (the primary host surface).
- Command functions: `check`, `plan`, `build`, `planner_cmd`, `import_modelica_cmd`, `artifact_check`, `artifact_battery`, `architecture`, and `help_text()`.
- Module `genesis_cmd` (genesis/parse/signature/compile/world/portfolio commands) and `tooling_cmd` (`tooling_dispatch`).

## Invariants
- Typed refusals carry stable E-* codes: `check`/`plan` exit `EXIT_REFUSED` when the session reports errors; `artifact check` refuses empty state dirs (`E-EVID-105`) and illegible state dirs (`E-TLT-005`).
- `bench` is a typed refusal (`E-TLT-004`) until the benchmark comparison ruleset lands; `fork` refuses network sync offline (`E-TLT-006`).
- The `native.rust` registry entry is an exact-capability declaration, not a new capability or prefix match.
- A goal that cannot be planned must not exit 0 (silent success); unplanned goals force `EXIT_REFUSED`.
- CLI output is deterministic and documented (`help_text`); JSON diagnostics carry codes and messages so checker lanes can assert the exact E-* code.

## Error model
- No dedicated error enum: command functions return exit codes and print structured diagnostics to stdout (`print_diagnostics`) or stderr (`error: ...`). Build/planner/checker failures surface with their typed E-* codes via the stderr message text.
- `artifact battery` treats an escaped control (admitted dishonest artifact) as a refusal.

## Determinism class
- Deterministic: artifact ids, plan output, JSON documents and diagnostics ordering are fixed; output is documented in `help_text`.

## Cancellation behavior
- Not applicable: CLI is synchronous; the only long-running steps (`build --verify`, `run`, `test`, `bench`-adjacent cargo) are bounded by `emath-build`'s `run_cargo_timed` wall-clock budget (`E-RES-120`). The keep-gate harness runs subprocesses, not unsafe code.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml. It declares a `[[bench]] comprehensive_bench` harness (`benches/keep_gate.rs`, `harness = false`) and a `[[bin]] emath`.

## Conformance tests
- None on disk: no `tests/` directory; crate relies on library-unit coverage of `emath-sema`, `emath-build`, and `emath-checker`, plus the keep-gate bench harness (not a test).

## No-claim boundaries
- `bench` remains a typed refusal (`E-TLT-004`) with no Performance category claim until the comparison ruleset lands; the full formatter (`fmt`) is Phase 4.
- No third-party dependency ship beyond the toolchain (`vendor` is a zero-third-party offline snapshot).
- The keep-gate identity uses the SHA-256 primitive, not release crypto.