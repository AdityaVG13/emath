# emath-cli CONTRACT

## Purpose and layer
- Command-line application (CRATE_MAP tier: sema/build/lab-core).
- Commands: `check`, `plan`, `planner`, `build`, `eval`, `simulate`, `artifact check/battery`, `import modelica`, `architecture`, `web` (alias `serve`), plus Semantic Genesis (`parse`, `signature`, `genesis`, `eval`, `repl`, `compile --parametric`, `world show`, `portfolio show`, `meaning list|set|unset|explain`) and tooling commands (`new`, `fmt`, `explain`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider`, `fork`, `agent`, `help [<command>]`, `version`, `capabilities`, `robot-docs`).
- Exit codes: 0 success, 1 refusal/diagnostic, 2 usage or io error (`EXIT_OK`, `EXIT_REFUSED`, `EXIT_USAGE`).
- `run(&[String]) -> u8` is the testable entry behind `main`; the generated crate builds an agent envelope (`emath.agent`) over the same admission/plan/build paths as interactive commands.
- Registers the in-tree static `native.rust` capability so the generic planner
  serves native evaluation and exact scalar `simplify` goals.

## Public types and semantics
- Constants `EXIT_OK` (0), `EXIT_REFUSED` (1), `EXIT_USAGE` (2).
- `run(&[String]) -> u8`: command dispatch (the primary host surface).
- Command functions: `check`, `plan`, `build`, `planner_cmd`, `import_modelica_cmd`, `artifact_check`, `artifact_battery`, `architecture`, and `help_text()`.
- Module `genesis_cmd` (genesis/parse/signature/compile/world/portfolio commands), `meaning_cmd` (`emath meaning …` plus lock resolution for genesis/eval/compile), and `tooling_cmd` (`tooling_dispatch`).

## Invariants
- Typed refusals carry stable E-* codes: `check`/`plan` exit `EXIT_REFUSED` when the session reports errors; empty / comment-only source is `E-PKG-081` (not a vacuous admit); `eval`/`compile`/`simulate` refuse a missing source with `E-PKG-080` (same code as `check`); `artifact check` refuses empty state dirs (`E-EVID-105`) and illegible state dirs (`E-TLT-005`).
- `--json` on `check`/`eval`/`simulate` refusal includes diagnostic `code`s (not counts); a checker lane can assert the exact E-* the CLI refused with.
- `bench` is a typed refusal (`E-TLT-004`) until the benchmark comparison ruleset lands; `fork` refuses network sync offline (`E-TLT-006`).
- The `native.rust` registry entry is an exact-capability declaration, not a new capability or prefix match.
- A goal that cannot be planned must not exit 0 (silent success); unplanned goals force `EXIT_REFUSED`.
- CLI output is deterministic and documented (`help_text`); JSON diagnostics carry codes and messages so checker lanes can assert the exact E-* code.
- `emath explain <file> --provenance` renders every admitted binding-to-root
  provenance edge; `--json` uses `emath.provenance-explanation.v1` and keeps
  `Assumed` / `Unstated` visible.
- No Naked Answer (ADR-004, SG-09): `genesis` writes `answer-receipt.json` (schema `emath.answer-receipt` v2) before printing any answer, and a write failure refuses before the answer line. The receipt binds source, parse, signature, term, world, valuation, result, code (`artifact_hash` over the rendered parametric crate; explicit 0 = no code artifact), portfolio, trace, authority, and VM cost, and carries `receipt_id` (FNV-1a64 over the documented preimage in `genesis_cmd.rs`). Locked runs add `meaning_provenance=user-locked` plus lock ids to the receipt JSON without changing the v2 `receipt_id` preimage. Selection goes through G7 `evaluate` (`g7-portfolio-receipt.txt` is replayable). A single-world answer is legal only when the kept bag has one member or a lock committed; otherwise genesis refuses `E-GEN-095` instead of taking `kept.first()`. `cargo xtask demo semantic-genesis` independently recomputes `receipt_id`, refuses a zero `artifact_hash`, runs a tampered-result negative control, and challenges the generated Rust against the VM's portfolio answers (VM/Rust differential).
- Meaning lock: `emath meaning set` writes `.emath/meaning.lock` (local-side). On `genesis`/`eval`/`compile` (and malformed-file refusal on `check`/`plan`/`build`/`run`/`test`), a matching lock commits to that `WorldIr::identity` before portfolio ranking. Drift/tamper/malformed locks are typed `E-LOCK-*` refusals; never a silent fallback. `emath new` gitignores the lock file; teams MAY commit it.

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
- None: no `[features]` in the crate's Cargo.toml, which declares only `[[bin]] emath`. The `keep_gate` bench harness (`harness = false`) lives in the workspace test member at `tests/emath-cli/benches/keep_gate.rs`.

## Conformance tests
- No `tests/` directory in the crate. Workspace suites in `tests/emath-cli` (`tests/catalog.rs`, `tests/meaning_lock.rs`, `tests/lib.rs`); the keep-gate bench harness is `tests/emath-cli/benches/keep_gate.rs` (not a test). Library-unit coverage comes from `emath-sema`, `emath-build`, and `emath-checker`.

## No-claim boundaries
- `bench` remains a typed refusal (`E-TLT-004`) with no Performance category claim until the comparison ruleset lands; the full formatter (`fmt`) is Phase 4.
- No third-party dependency ship beyond the toolchain (`vendor` is a zero-third-party offline snapshot).
- The keep-gate identity uses the SHA-256 primitive, not release crypto.