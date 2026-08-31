# emath-cli CONTRACT

## Purpose and layer
- Command-line application (CRATE_MAP tier: sema/build/lab-core).
- Commands: `check`, `plan`, `planner`, `build`, `eval`, `simulate`, `artifact check/battery`, `import modelica`, `architecture`, `web` (alias `serve`), plus Semantic Genesis (`parse`, `signature`, `genesis`, `eval`, `repl`, `compile --parametric`, `world show`, `portfolio show`, `meaning list|set|unset|explain`), meaning-budget (`expand`, `solve --check|--apply`, `exactness [--raise units]`, `freeze`, `why`, `assumptions`), and tooling commands (`new`, `fmt`, `explain`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider`, `fork`, `agent`, `help [<command>]`, `version`, `capabilities`, `robot-docs`).
- Exit codes: 0 success, 1 refusal/diagnostic, 2 usage or io error (`EXIT_OK`, `EXIT_REFUSED`, `EXIT_USAGE`).
- `run(&[String]) -> CliExit` is the testable entry behind `main`; the generated crate builds an agent envelope (`emath.agent`) over the same admission/plan/build paths as interactive commands.
- Registers the in-tree static `native.rust` capability so the generic planner
  serves native evaluation and exact scalar `simplify` goals.

## Public types and semantics
- Constants `EXIT_OK` (0), `EXIT_REFUSED` (1), `EXIT_USAGE` (2).
- `run(&[String]) -> CliExit`: command dispatch (the primary host surface). `CliExit` is the closed 3-way `{Ok=0, Refused=1, Usage=2}` (`EXIT_OK`/`EXIT_REFUSED`/`EXIT_USAGE`); `main` maps those three values and no others.
- Command functions: `check`, `expand_cmd`, `plan`, `build`, `planner_cmd`, `import_modelica_cmd`, `artifact_check`, `artifact_battery`, `architecture`, and `help_text()`. Re-export `provenance_explanation`. Request types: `BuildRequest`, `PlannerRequest`, `CompileRequest`.
- JSON document builders (stdout envelopes for `--json`; parse-back seam like `catalog::capabilities_json`): `diagnostics_json_document`, `json_diagnostic_entry`, `expand_json_document`, `exactness_json_document`, `plan_json_document`, `agent_plan_json_document`, `agent_check_json_document`, `solve_check_json_document`, `architecture_json`.
- Module `genesis_cmd` (genesis/parse/signature/compile/world/portfolio commands), `meaning_cmd` (`emath meaning …` plus lock resolution for genesis/eval/compile), and `tooling_cmd` (trusted `new`/`fmt`/`explain`/`run`/`test`/`bench`/`verify`/`inspect`/`diff`/`doctor`/`vendor`/`provider`/`fork` cmds).

## Invariants
- Typed refusals carry stable E-* codes: `check`/`plan` exit `EXIT_REFUSED` when the session reports errors; empty / comment-only source is `E-PKG-081` (not a vacuous admit) for `check`/`plan` and for `expand`/`exactness`/`freeze`/`solve`/`why`/`assumptions`; `eval`/`compile`/`simulate` refuse a missing source with `E-PKG-080` (same code as `check`); `artifact check` refuses empty state dirs (`E-EVID-105`) and illegible state dirs (`E-TLT-005`).
- `--json` on `check`/`eval`/`simulate` refusal includes diagnostic `code`s (not counts); a checker lane can assert the exact E-* the CLI refused with. Missing provided file on `expand`/`exactness`/`freeze`/`solve`/`why`/`assumptions`/`plan`/`planner`/`build` `--json` is the same stdout diagnostic envelope as `check` (`command`, `admitted`, `diagnostics[{code,severity,message}]`) with `E-PKG-080` (IO exit `EXIT_USAGE`). Empty/comment-only `--json` on `expand`/`exactness`/`freeze`/`solve`/`why`/`assumptions` is that envelope with `E-PKG-081` (`EXIT_REFUSED`). Usage with no file operand stays stderr-only. `import modelica` `--json` success is `command`/`declarations`/`models`; import read/parse refusal is stderr-only (this protocol does not list an import diagnostic document).
- `eval` on a standard function spec executes ONLY admitted `emath function` declarations through the generic stack (sema admission → `definition_order` → `lower_definition` EMIR → reference-VM evaluation); there is no genesis fallback for function files, no second evaluator, and no domain branch. Plain `eval` (no `--set`) binds inputs from the spec's own single worked example (`inputs_from: example:<name>`) or evaluates a zero-input function; a spec with several examples (`E-EVAL-003`) or inputs without one (`E-EVAL-004`) must bind explicitly. `--set` values are finite decimal scalars or `[vector]` lists; inputs must be `Float64` or `Vector[Float64]` (`E-EVAL-006`). Genesis-format reference files keep the `--world` semantic-VM lane unchanged; `--world` on a function file (`E-EVAL-008`) and function flags on a genesis file (`E-EVAL-008`) are typed refusals.
- `bench` is a typed refusal (`E-TLT-004`) until the benchmark comparison ruleset lands; `fork` refuses network sync offline (`E-TLT-006`).
- The `native.rust` registry entry is an exact-capability declaration, not a new capability or prefix match.
- A goal that cannot be planned must not exit 0 (silent success); unplanned goals force `EXIT_REFUSED`.
- CLI output is deterministic and documented (`help_text`); JSON diagnostics carry codes and messages so checker lanes can assert the exact E-* code.
- `emath explain <file> --provenance` renders every admitted binding-to-root
  provenance edge; `--json` uses `emath.provenance-explanation.v1` and keeps
  `Assumed` / `Unstated` visible. `--json` refusal is a stdout diagnostic
  envelope (`{code,severity,message}`), not empty stdout.
- No Naked Answer (ADR-004, SG-09): `genesis` writes `answer-receipt.json` (schema `emath.answer-receipt` v2) before printing any answer, and a write failure refuses before the answer line. The receipt binds source, parse, signature, term, world, valuation, result, code (`artifact_hash` over the rendered parametric crate; explicit 0 = no code artifact), portfolio, trace, authority, and VM cost, and carries `receipt_id` (FNV-1a64 over the documented preimage in `genesis_cmd.rs`). Locked runs add `meaning_provenance=user-locked` plus lock ids to the receipt JSON without changing the v2 `receipt_id` preimage. Selection goes through G7 `evaluate` (`g7-portfolio-receipt.txt` is replayable). A single-world answer is legal only when the kept bag has one member or a lock committed; otherwise genesis refuses `E-GEN-095` instead of taking `kept.first()`. `cargo xtask demo semantic-genesis` independently recomputes `receipt_id`, refuses a zero `artifact_hash`, runs a tampered-result negative control, and challenges the generated Rust against the VM's portfolio answers (VM/Rust differential).
- Solve menu is closed: `emath solve --check` lists exactly `SolveWorld::ALL` (`real-pm`, `complex`, `modular`, `symbolic`, `numeric`); unknown `--apply` labels are `EXIT_REFUSED`, never a sixth world or a naked float.
- `emath freeze` emits schema `emath.freeze.lock.v1` with `authority_raised` always `false`; it does not close open holes or upgrade evidence. Claiming exactness with open holes is `E-SYN-147` (`EXIT_REFUSED`).
- JSON protocol (stdout unless noted). Named documents carry `schema`; command envelopes carry `command` and must not reuse a document schema id.
  - `capabilities --json`: schema `emath.capabilities`; required keys `schema`, `tool`, `version`, `contract`, `exit_codes.{0,1,2}`, `env_vars`, `commands[{name,usage,summary}]` (one object per catalog `COMMANDS` entry).
  - `architecture --json`: schema `emath.architecture`; required keys `schema`, `pipeline`, `required_paths`.
  - `plan --json`: `command`, `admitted`, `plans`, `goals[{kind,target}]`. `kind` is `GoalKind::as_str()` (not a concatenated string and not a duplicate count key). Missing provided file is the diagnostic envelope (`E-PKG-080`), not this success document. Empty source still uses this success document (no `diagnostics` key) plus stderr `E-PKG-081`.
  - `planner`/`build` `--json` missing source is the diagnostic envelope (`E-PKG-080`). Build IO/admission `--json` refusal uses the same envelope (`split_error_code` when the message carries an E-* code).
  - `agent check`: schema `emath.agent`; `diagnostics` is `[{code,severity,message}]` (not a count plus concatenated `diagnostics_text`). `agent plan`: schema `emath.agent`; `goals` is `[{kind,target}]` with `kind` = `GoalKind::as_str()` (not a duplicate count key); refusal also carries `diagnostics[{code,severity,message}]` (missing source `E-PKG-080`). `agent build` refusal is schema `emath.agent` with the same diagnostic objects. `agent propose` read/parse failure is schema `emath.agent` with `code`/`detail` (not stderr-only).
  - `expand --json`: `command`, `rewritten`, `level`, `ok`, `source` (original bytes), `expanded` (product), `notes[{inferred,rationale,replacement,stability}]`, `holes`, `solve_candidates`, `diagnostics`; optional `meaning_id`. Hole objects are `{name,constraints,continuation,candidates[{status,kind,label}],rejections[{attempt,reason}]}` plus `search_goal` when continuation is search. Diagnostic items are `{code,severity,message}` (`severity` is `error`/`warning`/`note`). Missing/empty `--json` is the diagnostic envelope (`E-PKG-080`/`E-PKG-081`), not this success document.
  - `exactness --json`: `command`, `declared`, `inferred`, `constructed`, `open`, `entries[{id,dimension,status,name,rationale}]`; optional `meaning_id`. Missing/empty `--json` is the diagnostic envelope (`E-PKG-080`/`E-PKG-081`).
  - `solve --check --json`: `command`, `ok`, `solve_candidates[{label,result_type,domain,exactness,method,evidence_class,holes,beginner_default,selected}]`. `solve --apply --json`: `command`, `apply`, `source`, `meaning_delta`. Missing/empty `--json` is the diagnostic envelope (`E-PKG-080`/`E-PKG-081`). `solve --check --json` with no `solve` intent is the diagnostic envelope (`admitted: false`), not the candidate menu.
  - `eval --json` on a standard function spec: schema `emath.eval-function` (v1); required keys `schema`, `schema_version`, `function` (declaration leaf), `entrypoint` (`sole` for a one-function file, `named` for `--function`), `inputs_from` (`set` for `--set` bindings, `example:<name>` when the spec's own worked example supplied the inputs, `none` for a zero-input function), `meaning_id` (package meaning identity), `inputs{name: rendered value}` (sorted by name), `outputs{name: rendered value}` (declared output order; a declared output with no computed definition is absent). Genesis-format evals keep the `emath.eval-answer` v1 document. Function-spec refusals are the diagnostic envelope (`admitted: false`) with the typed code from the E-EVAL-001..008 closure (unsupported entrypoint, unknown named entrypoint, ambiguous entrypoint, missing input, malformed/unknown/duplicate `--set`, unsupported input type, lowering/evaluation fault, `--world` misuse). Missing/empty source on the function lane is the envelope with `E-PKG-080`/`E-PKG-081`.
  - Freeze `--json` envelope: `command`, `ok`, `authority_raised`, `open_holes`, `source` (original), `frozen` (comments+expanded), `lock` (lock document string). Schema `emath.freeze.lock.v1` is only on the lock (nested `lock`, sidecar, stdout marker). Lock required keys: `schema`, `source_content_id`, `frozen_content_id`, `meaning_id`, `authority_raised`, `prelude`, `packages`, `methods`, `numeric_policy`, `providers`, `open`, `ledger`. `ledger` is `[{id,dimension,status,name}]` with `dimension`/`status` = `ExactnessDimension`/`ExactnessStatus::as_str()` (not concatenated row strings). `open` remains `["dimension:name", ...]`. Missing/empty `--json` is the diagnostic envelope (`E-PKG-080`/`E-PKG-081`).
  - `why`/`assumptions` `--json` missing/empty is the diagnostic envelope (`E-PKG-080`/`E-PKG-081`).
- `--raise` is catalog-legal only on `exactness`, and only the token `units`/`unit` (ExactnessDimension::Unit). Other commands or tokens are `EXIT_USAGE`. Raise does not rewrite non-unit ledger rows.
- Meaning lock: `emath meaning set` writes `.emath/meaning.lock` (local-side). On `genesis`/`eval`/`compile` (and malformed-file refusal on `check`/`plan`/`build`/`run`/`test`), a matching lock commits to that `WorldIr::identity` before portfolio ranking. Drift/tamper/malformed locks are typed `E-LOCK-*` refusals; never a silent fallback. `emath new` gitignores the lock file; teams MAY commit it.

## Error model
- No dedicated error enum: command functions return `CliExit` and print structured diagnostics to stderr (`print_diagnostics`, `error: ...`). `--json` documents stay on stdout. Build/planner/checker failures surface with their typed E-* codes via the stderr message text.
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
- No `tests/` directory in the crate. Workspace suites in `tests/emath-cli` include `catalog.rs`, `intent_completion_solve.rs`, `exactness_introspection.rs`, `meaning_lock.rs`, and `lib.rs`; the keep-gate bench harness is `tests/emath-cli/benches/keep_gate.rs`. Library-unit coverage comes from `emath-sema`, `emath-build`, and `emath-checker`.

## No-claim boundaries
- `bench` remains a typed refusal (`E-TLT-004`) with no Performance category claim until the comparison ruleset lands; the full formatter (`fmt`) is Phase 4.
- No third-party dependency ship beyond the toolchain (`vendor` is a zero-third-party offline snapshot).
- The keep-gate identity uses the SHA-256 primitive, not release crypto.
## Absorbed module: `diagnostics` (was `emath-diagnostics`)
- Pedagogic explanations and rendered witnesses for finite-checker
  refusals (schema `emath.diagnostic.explanation v1`); `tutor-check/v1`
  rejects synthesized narrative not backed by a checker receipt.
- Public surface (module `diagnostics`): `Explanation`, `ExplainKind`,
  `RenderedWitness`, `TableExcerpt`, `DocLink`, `RenderFormat`,
  `tutor_check_v1`, `explain_law_report`, `check_and_explain`,
  `every_failure_has_witness`, `e_law_001_demo`.
- Invariants: a `LawFalsified` explanation without a `RenderedWitness`
  and receipt id is rejected; witness cells come from the checker table,
  never invented numbers; explaining a refusal raises no authority.
- No-claim: explanations do not prove the law; they show the finite
  counterexample the checker found.
