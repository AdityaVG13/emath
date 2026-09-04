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
- Semantic commands load and verify the nearest authored `language/` distribution before admission or execution. Source ancestry takes precedence over working-directory ancestry. Missing roots, stale locks, generated drift, invalid capsules, missing source maps, duplicate authority, and capsule-active blocking holes refuse with `E-LANG-IMAGE`; a distribution-hash string alone is never authority.
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
  - `sweep` (`emath sweep <file.emath> --function NAME --grid name=v1,v2,... [--expect name=value] [--out <file>] [--json]`): cartesian parameter-grid runner over ONE admitted stateless `emath function`, sharing eval's generic stack (sema admission, EMIR lowering, reference VM) and its E-EVAL-* refusal closure. Grid axes bind `--set`-grammar values; the grid must cover every declared input. Cells enumerate deterministically (axes in CLI order, first axis slowest, values in CLI order). Text stdout is one line per cell: `<function> <axis>=<v>...: <computed values> OK|MISMATCH (want <raw>)` (values = expected outputs in expect order, or all outputs when no `--expect`; `error <code>: <message>` for cell evaluation faults). Exit 0 only when every cell passes; any mismatch/error is exit 1. `--json` prints the artifact; `--out` additionally writes it (artifact + newline). Artifact schema `emath.sweep.v1`: required keys `schema`, `schema_version`, `function`, `meaning_id`, `grid{axes[{name,values[]}]}` (raw CLI strings, CLI order), `expect{name: value}` (CLI order), `cells[{index,bindings{...},outputs{...},status}]` (`status` = `ok`/`mismatch` (adds `want`,`got`)/`error` (adds `code`,`error`)), `summary{total,ok,mismatch,error}`. No wall-clock field exists; byte-identical invocations produce byte-identical output.
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
- No `tests/` directory in the crate. Workspace suites in `tests/emath-cli` include `catalog.rs`, `intent_completion_solve.rs`, `exactness_introspection.rs`, `meaning_lock.rs`, and `lib.rs`; the keep-gate bench harness is `tests/emath-cli/benches/keep_gate.rs`. Library-unit coverage comes from `emath-sema`, `emath-build`, and `emath-evidence`.

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

## Absorbed module: `lsp` (was `emath-lsp`)

# emath-lsp CONTRACT.md

## Purpose and layer

- Tier: sema/cli (CRATE_MAP); the language-server-protocol transport and
  server skeleton for emath.
- Minimal, std-only LSP server: base-protocol framing
  (`Content-Length` headers, JSON-RPC messages), `initialize` capabilities
  with incremental text synchronization, `textDocument/didOpen`/`didChange`
  with incremental edits, and `publishDiagnostics` computed by the real
  compiler session (`emath_sema::CompilerSession::check_owned`), so LSP and
  CLI agree on diagnostics (shared admission path).
- Skeleton `completion` (Phase 1 grammar keywords), `hover` (keyword
  documentation), `signatureHelp` (null response). Typed method refusal
  (`-32601`), deterministic writes.
- Two lanes with identical wire syntax:
  - **default blocking stdio lane** (`crate::run`, `crate::protocol`,
    `crate::server`): std-only, dependency-free.
  - **`async-runtime` lane** (`crate::transport`, `crate::lab`): the same
    framing on the asupersync `Cx`/io traits, plus a Cx/lab-runtime test
    entry. Feature-gated; the blocking lane is untouched by it.

## Public types and semantics

- `run(input: &mut impl Read, output: &mut impl Write) -> u8`: the blocking
  server loop until EOF. Returns `0` if the client performed `shutdown`
  before `exit`, `1` otherwise (the LSP exit-code contract).
- `Transport<R, W>::new` / `Transport::with_control` / `Transport::serve`:
  the async lane generic over `R: AsyncRead + Unpin`, `W: AsyncWrite +
  Unpin`. `serve(&Cx)` returns `Ok(0)` on `shutdown`-then-EOF / host
  `Control::Shutdown`, `Ok(1)` on abnormal exit, or `Err(TransportError)`.
- `TransportError`; the async lane error surface: `Io(io::Error)`,
  `Frame(String)`, `BodyTooLarge { length, max }`, `Cancelled`.
- `Control::Shutdown`; host stop signal on an optional bounded mpsc.
- Modules: `json` (deterministic JSON), `protocol` (blocking framing),
  `server` (`ServerState`, diagnostics/publish dispatch), `lab` and
  `transport` (feature-gated).

## Invariants

- **Framing parity:** the async lane is byte-for-byte identical to the
  blocking `protocol.rs`; identical `Content-Length` headers and the same
  exit-code contract, so the two lanes are indistinguishable on the wire
  (enforced by `async_written_frame_reads_back_through_blocking_protocol`).
- **Exit-code contract:** `0` ⇔ `shutdown` preceded the terminal event
  (EOF, `exit`, or host `Control::Shutdown`); `1` ⇔ any other terminal.
- **Frame cap:** async lane refuses a `Content-Length` body > 16 MiB
  (`TransportError::BodyTooLarge`) before any allocation, answering `-32700`
  and exit code `1`; header lines are capped at 4096 bytes in both lanes.
  The blocking `protocol::read_message` body stays uncapped.
- **Checkpoint discipline:** the async loop checkpoints before every frame
  read, before every dispatch, and before every write, so upstream
  cancellation (region close / abort) and any region budget (deadline / poll
  quota) are acknowledged at message boundaries; in-flight frame I/O is
  dropped.
- **Std-only default:** no network, filesystem watch, or third-party
  dependencies unless `async-runtime` is enabled.
- UTF-8 byte offsets are converted to LSP character semantics on
  glyph-bearing lines; `positionEncoding: utf-8` is advertised at
  `initialize`. Incremental `didChange` edits round-trip byte offsets
  correctly. Unknown methods get a typed refusal (`-32601`); parse/framing
  failures yield JSON-RPC `-32700`. Deterministic output ordering.

## Error model

- **JSON-RPC (`-32700`/`-32601`):** the entire agent/user-visible surface.
  Protocol parse-error responses are code `-32700` (id `null`), unknown-method
  refusals are `-32601`. Message text is deterministic. Both lanes emit the
  identical wire behavior. See `implementation/ERROR_CODES.md` (LSP entry);
  this is the whole surface; no `E-LSP-*` family is defined or necessary.
- **`TransportError` (async lane only, internal):** typed, not user-facing.
  `Io` carries the underlying async I/O error; `Frame` is a malformed-framing
  class that maps to the `-32700` response; `BodyTooLarge` is the 16 MiB cap
  refusal (also → `-32700` + exit 1); `Cancelled` is cooperative
  cancellation observed at a checkpoint (propagates upward, no response
  written). Writes are bounded and check-flushed: `write_all`/`flush`
  failures surface as `TransportError::Io`, never swallowed.
- No panics escape either loop on malformed input.

## Determinism class

- **Deterministic.** No wall clock, no network, no filesystem watch, no
  third-party dependency in the default build. Output derives only from the
  message stream and shared compiler diagnostics. The async lane checkpoints
  are deterministic; region budgets are the caller's to set. Tests run on a
  deterministic lab runtime (`crate::lab::run_with_cx`), never wall-clock.

## Cancellation behavior

- Blocking lane: not applicable (synchronous request loop, no background
  work).
- Async lane: the whole loop is one region-owned unit (typically
  `Cx::spawn` + `TaskHandle`). Cancellation is acknowledged at message
  boundaries via `cx.checkpoint()` (→ `TransportError::Cancelled`); region
  close drains/loses the in-flight frame; host `Control::Shutdown` stops
  cleanly after flushing the in-flight frame with exit code `0`. EOF beats a
  queued control signal. `read_exact`/`write_all` retain their crate-documented
  (non-drop-cancel-safe) semantics; a documented seam, not a guarantee.

## Unsafe boundary

- None. Crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags

- None in the default build.
- `async-runtime = ["dep:asupersync"]`: the single gate under which
  emath-lsp pulls the pinned asupersync git revision and exposes the
  `lab` and `transport` modules. This feature is the only path to the
  asupersync dependency.

## Conformance tests

Integration suite from the former `tests/emath-lsp` package, now in this
package's `tests/`: `tests/lab.rs`, `tests/server.rs`, `tests/transport.rs`. 26 tests:

`server.rs`:
- `range_offsets_use_utf8_byte_characters_on_glyph_lines`
- `initialize_advertises_utf8_position_encoding`
- `glyph_bearing_did_change_round_trips_byte_offsets`

`transport.rs`:
- `async_written_frame_reads_back_through_blocking_protocol`
- `serve_round_trips_initialize_shutdown_exit_with_zero_exit_code`
- `serve_eof_before_shutdown_returns_one`
- `serve_shutdown_then_eof_returns_zero`
- `aborted_serve_task_join_reports_cancelled`
- `dropped_serve_region_drains_cleanly`
- `read_frame_refuses_oversized_content_length`
- `serve_refuses_oversized_frame_with_parse_error_and_exit_one`
- `control_shutdown_exits_zero_after_flushing_in_flight_frame`
- `writer_error_propagates_as_typed_io_error`
- `writer_flush_error_surfaces`
- `serve_refuses_invalid_content_length_with_parse_error`
- `serve_refuses_eof_mid_header_with_parse_error`
- `serve_refuses_short_body_with_parse_error`
- `read_frame_accepts_header_case_and_whitespace_variants`
- `read_frame_refuses_oversized_header_line`
- `serve_partial_second_frame_yields_clean_error_no_partial_output`
- `region_close_drains_mid_body_pending_read`
- `serve_is_deterministic_identical_input_identical_output`
- `serve_dispatches_sequentially_with_strict_single_flight_order`

`lab.rs`:
- `region_task_completes`
- `region_close_cancels_dropped_task`
- `abort_makes_join_report_cancellation`

## No-claim boundaries

- No real-stdio binding yet: the async `Transport` is generic over
  `AsyncRead`/`AsyncWrite` in-memory impls; OS stdin/stdout (native or an
  asupersync-tokio-compat io bridge) is a future drop-in. Tests use
  in-memory readers/writers only.
- No `tokio` in the crate; asupersync only behind the `async-runtime`
  feature. No network transport, no filesystem watch (`fs.watch` /
  distribution-tools unavailable), no third-party protocol client.
- Skeleton coverage only: `completion` is Phase 1 keyword completion,
  `signatureHelp` returns null, no formatting/rename/code-action support.
- Per-message wall-clock budgets are a documented seam, not wired; the async
  lane defers budgets to the caller's region-scoped `Cx` budgets.
- The blocking `protocol::read_message` body is uncapped (16 MiB is the
  async-lane-only bound).
- Mid-handler cancellation of the sync `ServerState::handle` is an indivisible
  seam; per-message `Scope` isolation is deferred pending an actor/`Arc<Mutex>`
  refactor.

## Absorbed module: `layout` (was `emath-layout`)

# emath-layout

## Purpose and layer

- Frontier lane for math *layout* input (SG-11 LaTeX, SG-12 PDF fixtures).
- Shared [`MathLayoutGraph`](src/graph.rs) is the IR both frontends emit.
- Depends on `emath-term`, `emath-genesis` (scoped binders), and `emath-world-ir` (`fnv1a64`).
- Fixture-driven: not a production LaTeX engine and not a PDF binary parser.

## Public types and semantics

- `LAYOUT_SCHEMA` = `emath.math-layout-graph` (same string as the disclosed emath-schema registry id), `LAYOUT_VERSION` = 1. `check_version` refuses unknown versions.
- `MathLayoutGraph`: ordered nodes and edges, retained source bytes, retained ambiguities, optional unlowered regions. `canonical()` is a versioned text encoding; `graph_id()` is FNV-1a64 over that form. `source()` is the original input (LaTeX) or the deterministic fixture serialization (PDF).
- `LayoutNode { id, content, source_span }`. `LayoutContent`: Glyph, Row, Superscript, Subscript, Fraction, Radical, BigOp(kind name), FormulaRegion.
- `LayoutEdge { parent, child, relation }` with `SpatialRelation`: RightOf, Above, Below, SuperscriptOf, SubscriptOf, Contains.
- `RetainedAmbiguity { node_id, reading_a, reading_b, reason }`: both readings stay on the graph.
- `UnloweredRegion { node_id, reason }`: formula extracted, term not fabricated.
- `parse_latex`: mixed documents detect `$...$` (inline) and `\[...\]` (display); bare math (no delimiters) is one formula. Structured subset only.
- `to_binder_term`: `\sum`/`\prod` → Structural binders (FiniteRange when bounds are integer literals, else Symbolic); `\int` → Integral / FiniteAnalogue; `\lim` → Limit / Conventional; infix `+ - * / =` → `Term::Apply`; letters/Greek → Variable; digits → Constant; non-binder `x^2` → `Apply("pow", [x, 2])`. A top-level `var = binder` equation lowers to the binder (term IR cannot wrap a binder in `Apply`).
- `PositionedGlyph` / `PdfPageFixture`: milli-unit integers. `reference_fixture()` is the supplied 2D formula `E = Σ_{i=1}^{3} i²`. `extract` groups y-bands, detects super/sub by offset + smaller font, keeps the 20–45% font-size band as a retained ambiguity, and marks formula vs prose runs.

## Invariants

- LaTeX `source()` is byte-exact with the input.
- Formula-region spans slice the source to the delimited region (`$...$` or `\[...\]`) exactly.
- Canonical form and `graph_id` are identical across independent rebuilds of the same input.
- Ambiguities are retained; the frontend never picks a single reading in the ambiguous band.
- Extraction never invents a binder term: failure is `LayoutError::Unlowered` (and is recorded on the PDF graph).

## Error model

- `LayoutError::UnknownVersion`: schema handshake.
- `UnexpectedToken { token, offset }`: character or token outside the subset.
- `UnknownMacro { name, offset }`: backslash command outside the subset (offset is the `\`).
- `UnterminatedDollar` / `UnterminatedDisplay`: missing closer.
- `Unlowered { reason }`: graph may still exist; no term is produced.

## Determinism class

- Deterministic: `BTree`-style sorted nodes/edges/ambiguities, sequential node ids, integer milli-units, FNV-1a64 identities.
- No floats in the graph or fixture coordinates.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- `graph_unknown_version_refused`
- `graph_canonical_identical_across_rebuilds`
- `latex_source_preserved_byte_exact`
- `latex_sum_lowers_to_structural_finite_range_and_expands`
- `latex_unknown_macro_refused_with_offset`
- `latex_unterminated_dollar_refused`
- `latex_formula_region_spans_byte_exact`
- `pdf_reference_fixture_graph_id_deterministic`
- `pdf_superscript_detection_emits_relation`
- `pdf_ambiguous_band_retains_both_readings`
- `pdf_prose_only_has_zero_formula_regions`
- Production path: `cargo xtask demo math-layout` parses a mixed LaTeX document, extracts the PDF reference fixture, expands the LaTeX sum through `emath_genesis::run` / `FreeTermWorld`, records a typed unknown-macro refusal and a retained ambiguity, and emits `math-layout.json` with a tamper negative control.

## No-claim boundaries

- Not a general LaTeX engine: structured subset only (letters, digits, `+ - * / = ( )`, `^`/`_`, `\frac`, `\sqrt`, `\sum_{v=a}^{b}`, `\prod_{v=a}^{b}`, `\int_{a}^{b}`, `\lim_{v \to a}`, named Greek). Everything else is a typed refusal naming the token and byte offset; there is no recovery or guess.
- Not a PDF binary parser: positioned-glyph fixtures only. No page stream, font program, or content-stream interpretation.
- Ambiguities are retained, not resolved. The 20–45% font-size y-offset band emits both readings.
- There is no persisted layout store. `LAYOUT_VERSION` consumers refuse unknown versions (`check_version`), so rollback/migration is refuse-and-rebuild; an old artifact citing a different version is rejected, never reinterpreted.

## Absorbed module: `agent_protocol` (was `emath-agent-protocol`)

# CONTRACT; emath-agent-protocol

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Agent-native meaning proposals: submission envelope, admission, challenge loop, feedback, capability gates.
- A proposal traverses: schema admission, capability admission, deterministic checker suite, counterexample generation, evidence and cost gates, portfolio ranking, then revision request or world candidate.
- Depends on: emath-term, emath-world-ir, emath-genesis, emath-portfolio.

## Public types and semantics
- `AgentProposal` - one proposal envelope (problem id, base world or hole ids, changes, claimed obligations, derivation, required providers, estimated cost, requested authority).
- `ProposalKind` - what kind of proposal it is.
- `ChallengeLoop` - runs the deterministic challenge over a proposal against a portfolio; returns `ChallengeOutcome`.
- `ChallengeOutcome` - `Refused(AdmissionRefusal)` or a continuation: `RevisionRequest` / `WorldCandidateRef`.
- `CheckerSuite` / `NamedCheck` - ordered deterministic checks over a proposal.
- `AdmissionRefusal` - stable admission refusal (code, detail, proposal identity).
- `AgentFeedback` - structured feedback to the agent (solved holes, failed constraints, counterexample, unmet evidence, cost regression, portfolio dominance).
- `EXECUTION_AUTHORITIES` / `PROPOSAL_AUTHORITIES` - the two authority namespaces agents may request.
- (not exhaustive)

## Invariants
- A proposal carries no direct execution authority; the loop never grants execution authority and never grants `Certified` or `Proved`.
- `Certified`/`Proved` require external compiler, capability, evidence, and benchmark gates, not the challenge loop.
- Challenge runs checks in deterministic order.
- A proposal either yields a revision request or a world candidate, never a granted authority.

## Error model
- Admission failures surface as typed `AdmissionRefusal` (machine-readable `code`, human-readable `detail`, `proposal_identity`); exposed as `ChallengeOutcome::Refused`.
- Individual checks return `Result<(), String>`; the first failure text is surfaced as the smallest counterexample.
- No panics; no untyped string fallbacks for admission.

## Determinism class
- Deterministic: schema admission, capability admission, checker suite, counterexample generation, evidence and cost gates, and portfolio ranking are ordered and canonical; proposal and feedback carry canonical forms.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- Integration tests from the former `tests/emath-agent-protocol` package, now `tests/challenge.rs` here:
  - admission refuses an execution-authority claim with `capability:authority-not-admitted`
  - admission refuses an incomplete schema (missing base worlds) with `schema:incomplete`
  - a valid proposal runs to a `WorldCandidate` whose proposal identity and candidate identity are deterministic across two constructions of the same envelope

## No-claim boundaries
- A slice of the planned governance surface, not the full production admission service.
- Meaning proposals are proposals, not certified answers; their authority claims are requests, not grants.
- World candidates from the loop are ranked by portfolio, not certified by admission.

## Absorbed module: `portfolio` (was `emath-portfolio`)

# emath-portfolio

## Purpose and layer

- Tier 6, semantic genesis substrate (per CRATE_MAP; may depend on meaning-provider-api).
- Deterministic interpretation portfolios: collects, ranks, and selects interpretation candidates over world identities.
- G7 extends this crate in place: integer-metric ranking, Pareto archive, selection policies with an explicit collapse gate, disqualification ledger, and byte-identical receipt replay.

## Public types and semantics

- InterpretationPortfolio: deterministic candidate collection, sorted by a stable policy; new sorts and candidates exposes the order.
- InterpretationCandidate: world id, name, canonical answer, scoped Authority, score vector, provenance summary.
- translated_candidate(morphism, base, answer): admits a candidate into the morphism target world, records the morphism identity in provenance, and caps authority by the morphism's preservation relation.
- Authority enum: Structural < Tested < Certified < Proved (lattice rank 0..=3). `as_str` / `lattice_rank` are the wire and ranking forms. Ranking never raises a label.
- ScoreVector: multi-objective cost, complexity, evidence, utility (f64); lower cost/complexity and higher evidence/utility preferred. Genesis-era only; G7 uses integer metrics.
- PORTFOLIO_SCHEMA_VERSION (1): version constant for the portfolio document layout (durable id emath.interpretation-portfolio lives in the schema registry).
- WorldCandidate (G7 record): `world_fingerprint`, `provider_id`, `evidence_authority`, `labeled_authority`, `metrics` (`BTreeMap<String, i64>`), `artifact_hash`, optional `guard_failure`. `new` sets the label equal to evidence. `with_claimed_label` refuses if the claim is strictly above evidence.
- CandidateRecord::world_candidate: projects the genesis-era record onto WorldCandidate; ScoreVector floats become milli-unit integers.
- MetricAxis / MetricPolarity: declared ranking and Pareto axes (`max` / `min`).
- RANKING_KEY_SPEC: `authority.desc,axes.declared,fingerprint.asc,provider.asc,artifact.asc`.
- InterpretationPolicy: `Portfolio` (keep all non-dominated), `SingleBest { collapse }`, or `UserLocked { lock_id, origin_receipt_id, method }` (single-world user lock; provenance `user-locked`). `canonical()` is the wire name.
- CollapsePolicy: `RequireUnique` (exit gate) or `RankKey` (explicit collapse).
- MeaningLock (project-local `.emath/meaning.lock`, schema `emath.meaning-lock` v1): BTreeMap-ordered, byte-stable JSON. Entries keyed by `(declaration_id, hole_id)` store `world_fingerprint` (the same `WorldIr::identity` / `WorldCandidate::world_fingerprint` used by G7), `portfolio_receipt_id`, `selection_method`, `source` / `source_hash` drift witnesses, and `selected_at` (excluded from `lock_id`). `DEFAULT_PORTFOLIO_CAP` is 5 (receipted `portfolio_cap`, not a hidden constant). `commit_locked_world` skips ranking and emits a `UserLocked` receipt. `refuse_disqualified` refuses `set` when the fingerprint is on the checker/guard ledger (dominated worlds remain choosable).
- Re-exports: `MeaningLock`, `LockEntry`, `LockKey`, `LockError`, `SelectionMethod`, `commit_locked_world`, `refuse_disqualified`, `apply_portfolio_cap`, `DEFAULT_PORTFOLIO_CAP`, `PROVENANCE_USER_LOCKED`, `WHOLE_TERM_HOLE`, `LOCK_SCHEMA` / `LOCK_SCHEMA_VERSION`.
- ParetoArchive: non-dominated set in ranking-key order; dominated members recorded with the lowest-fingerprint witness.
- PortfolioReceipt: `input` (policy, axes, candidates sorted by fingerprint), `ranked`, `selected`, `archived`, `ledger`, `receipt_id` (FNV-1a64 of the canonical body). `encode()` is the durable byte form.
- ReceiptInput + `replay(input)`: re-runs selection; success is byte-identical to the original `encode()`.
- Re-exports from submodules: select, SelectionOutcome, SelectionPolicy, SelectionWeights; CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict, GuardFailure, WorldCandidate; replay_identity, PortfolioLock. (not exhaustive.)

## Ranking key

Total order, no floating comparison:

1. `evidence_authority` descending (`Proved > Certified > Tested > Structural`)
2. each declared metric axis in declaration order: Maximize → larger `i64` first; Minimize → smaller `i64` first
3. `world_fingerprint` ascending (bit-exact tie-break)
4. `provider_id` ascending
5. `artifact_hash` ascending

Missing declared metrics disqualify as `failed-guard:missing-metric` and never enter ranking or Pareto.

## Invariants

- Portfolio order is deterministic by policy: authority descending, utility descending, cost ascending, complexity ascending, then world id ascending.
- f64 comparisons in the genesis-era `InterpretationPortfolio` use total_cmp, giving a total order over finite and NaN values alike.
- G7 ranking uses only `i64` metrics and `Ord` on authority / fingerprints.
- Pareto semantics are kept: candidates are retained rather than dropped on conflicting objectives. Dominated G7 candidates are recorded on the archive and on the ledger; they are not silently dropped.
- translated_candidate never raises authority. Exact and refinement relations keep the base authority; approximation, simulation, and observational-equivalence degrade it to Structural. When obligations disagree, any degrading relation wins.
- Authority non-escalation: `labeled_authority` must be `<= evidence_authority`. `evaluate` refuses the whole run on a seeded escalation. Ranking and selection never emit a candidate labeled above its evidence.
- Exit gate: `SingleBest { collapse: RequireUnique }` with more than one non-dominated world is `PortfolioError::AmbiguousSingleBest`. There is no hidden single-world selection.
- Ledger completeness: for a successful receipt, `selected ∪ archived ∪ ledger` is a partition of the input fingerprints (`selected + archived + disqualified = input`).
- Receipt replay: `replay(&receipt.input).encode()` equals `receipt.encode()` byte-for-byte.
- Meaning lock: a matching lock commits to that world fingerprint before ranking; the run is single-world. Drifted, missing, or inadmissible locked worlds refuse (`E-LOCK-004`) with a hint to `emath meaning unset`; never a silent fallback to another world. Tampered `lock_id` refuses (`E-LOCK-003`). Malformed files refuse (`E-LOCK-001`); unknown `schema_version` refuses (`E-LOCK-002`). Locked receipts record provenance `user-locked` and copy the candidate's evidence authority; a lock never escalates authority. Locks are local-side (per-user, per-project) and are not baked into shared source; teams MAY commit `.emath/meaning.lock` to share one interpretation.

## Error model

- Genesis-era `select` emits no errors; it consumes precomputed scores and verdicts.
- G7 `evaluate` / `replay` / `WorldCandidate::with_claimed_label` return `PortfolioError`:
  - `AmbiguousSingleBest { nondominated }`: single-best exit gate.
  - `NoViableCandidate`: single-best with an empty archive.
  - `AuthorityEscalation { fingerprint, evidence, claimed }`: label above evidence.
  - `DuplicateFingerprint { fingerprint }`: two candidates share a world fingerprint.
- Meaning-lock `LockError` (every token is registered in `implementation/ERROR_CODES.md`):
  - `E-LOCK-001` `Malformed`: truncated JSON, unknown fields, missing fields, unreadable file.
  - `E-LOCK-002` `UnknownVersion`: `schema_version` other than 1.
  - `E-LOCK-003` `Tampered`: stored `lock_id` does not match the identity body (fingerprint edits without a matching id).
  - `E-LOCK-004` `Drifted`: locked world missing from current identities, source/declaration witness mismatch, or the locked candidate is no longer admissible.
  - `E-LOCK-005` `Disqualified`: `set` targeted a checker/guard-disqualified world; the diagnostic includes the ledger row.
  - `E-LOCK-006` `UnknownCandidate`: `set` named a fingerprint that is not in the current portfolio.

## Determinism class

- Deterministic: identical candidates yield identical portfolio order, enabling replay.
- G7 receipts are order-independent: input candidates are stored fingerprint-ascending; ranking ignores caller order.
- Meaning-lock encode is BTreeMap-ordered and byte-stable for the same entries and `selected_at`. `selected_at` is excluded from `lock_id`; changing the timestamp does not change identity.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- `tests/portfolio.rs` + `tests/portfolio_meaning_lock.rs` (from the former `tests/emath-portfolio` package): stable-policy ordering (authority > utility > cost) and world-identity tie-breaking; exit-gate translation keeps both source and target worlds and deopts on a failed fast-path guard; approximation caps Tested to Structural while exact preserves it.
- `interpretation` unit tests: ranking determinism and fingerprint tie-break; hand-computed 3-candidate Pareto archive; single-best refusal gate; ledger completeness; receipt replay byte-identity; authority non-escalation plus seeded escalation refusal; explicit `RankKey` collapse.
- `meaning_lock` unit tests: encode round-trip byte-determinism; timestamp excluded from `lock_id`; unknown version; malformed file; tampered fingerprint; fingerprint match vs source drift; `commit_locked_world` single-world user-locked receipt; disqualified `set`; drifted locked candidate.
- Production path: `cargo xtask demo interpretation-portfolio`.

## No-claim boundaries

- Keep-pareto semantics only over the candidates supplied; E-GEN-090/091 deferred worlds are recorded as deferred entries, not prioritized by this crate.
- Selection correctness depends on precomputed, honest ScoreVector / integer-metric and authority inputs. This crate does not mint evidence or raise authority.
- Genesis still emits the genesis-era `InterpretationPortfolio` JSON bag (`keep: pareto N` is a cap on that bag). Answer selection is `evaluate` / `replay` over `InterpretationCandidate::world_candidate` (uniform `cost=1`, so domination cannot drop a kept world). `g7-portfolio-receipt.txt` is the selection artifact. Hidden single-winner collapse is `E-GEN-095`.
- Meaning-provider discovery and world construction live in other crates; this crate ranks and selects records it is given.
- A user lock does not promote a world to `tested`/`certified`/`proved`. Provenance `user-locked` is a selection source, not an authority upgrade.
