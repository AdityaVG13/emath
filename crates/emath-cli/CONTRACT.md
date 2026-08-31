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

- Tier: sema/cli (CRATE_MAP) — the language-server-protocol transport and
  server skeleton for emath.
- Minimal, std-only LSP server slice: base-protocol framing
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
- `TransportError` — the async lane error surface: `Io(io::Error)`,
  `Frame(String)`, `BodyTooLarge { length, max }`, `Cancelled`.
- `Control::Shutdown` — host stop signal on an optional bounded mpsc.
- Modules: `json` (deterministic JSON), `protocol` (blocking framing),
  `server` (`ServerState`, diagnostics/publish dispatch), `lab` and
  `transport` (feature-gated).

## Invariants

- **Framing parity:** the async lane is byte-for-byte identical to the
  blocking `protocol.rs` — identical `Content-Length` headers and the same
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
  this is the whole surface — no `E-LSP-*` family is defined or necessary.
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
  (non-drop-cancel-safe) semantics — a documented seam, not a guarantee.

## Unsafe boundary

- None. Crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags

- None in the default build.
- `async-runtime = ["dep:asupersync"]`: the single gate under which
  emath-lsp pulls the pinned asupersync git revision and exposes the
  `lab` and `transport` modules. This feature is the only path to the
  asupersync dependency.

## Conformance tests

Workspace integration suite `tests/emath-lsp` (no `tests/` dir on disk in the
crate and no `#[cfg(test)]` in `src/`). 26 tests, in `tests/server.rs`,
`tests/transport.rs`, `tests/lab.rs`:

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
- There is no persisted layout store. `LAYOUT_VERSION` consumers refuse unknown versions (`check_version`), so rollback/migration is refuse-and-rebuild — an old artifact citing a different version is rejected, never reinterpreted.
