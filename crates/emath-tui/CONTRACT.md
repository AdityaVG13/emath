# emath-tui Contract

## Purpose and layer

The loop surface crate: the headless research-loop host core (B2),
the `emath loop` line-REPL (B3), and the dashboard implementing the
user's design spec (B4). The host core is lane-agnostic session
bookkeeping over the VM engine; the native lane (B5) re-implements the
same `emath.scratch.v1` contract over the artifact ABI. The dream
driver (bead emath-gav6o) is thin orchestration over the host core for
open-ended self-escalating research. This crate owns
no loop policy: proposal order, charging, freshness, promotion, and
verdicts are the authored module's mathematics.

## Session surface contract

A module exposes a session surface as an authored pair:

- `emath function Step<Name>` with inputs `(state: T, budget: Int)`
  and exactly one output of type `T` - one `research_batch` per call,
  with the target's closures lifted to module-level `Make*` functions.
  An optional third input of type `sequence(Observation)`
  (`search.observations`) receives the run's evidence (empty outside
  `emath dream`, and until the inbox delivers some);
- `emath function Seed<Name>` with exactly one `Int` input and one
  output of type `T` - the initial state.

`scan_session_surfaces` reports complete pairs plus every near-miss as
a problem (unpaired `Step`/`Seed`, wrong signatures, state-type
mismatch). Near-misses are never silently ignored. `LoopHost::open`
refuses `loop_surface` / `loop_target` / `loop_ambiguous` with the
named lift diagnostic (the exact pattern to author, pointing at
`tests/fixtures/constructor/research_step_valley.emath`).

The state type `T` is expected to carry the loop state contract fields
(`archive`, `incumbent`, `case_set`, `batch`, `used`, `verdict`,
`mode`); deviations refuse `loop_state_contract` naming the field.

## Unchanged native exports

Admission and module identity are checked on every export. Identical generated
artifact/host sources and manifests preserve mtimes through the shared generated
file writer; changed contents still rebuild through Cargo.

## Public types and semantics

- `LoopHost::open(module, target)` - parse, scan, choose surface,
  compute identity (module meaning id via the compiler session; the
  engine image label as language id; the Step name as target).
- `LoopHost::begin()` - the seed state, validated as the surface's
  state record.
- `LoopSession::step(host, budget)` - one batch; projects the ledger
  entry from the previous and next states (acceptance flips diff by
  archive ordinal; append-only archive enforced); enforces the
  module-semantic ledger law.
- `LoopSession::grow_case(case_id)` - the host curriculum seam: freeze
  one more case ordinal into the state's `case_set.ids`. Duplicates
  refuse `loop_case_duplicate`. The next batch re-probes and re-scores
  the whole archive on the grown set (the authored freshness law).
- `LoopSession::save/load` - `emath.scratch.v1` checkpoints through
  the exec-ir scratch contract, plus the host-side laws (state type
  match, batch counter equals revision).
- `value_int/value_rat/value_bool/value_sequence`, `state_int`,
  `incumbent_record`, `case_ids` - loop-state field readers for
  surfaces and the REPL. Exact integers read as rationals where a
  score is expected (authored targets may return exact zero as an
  Int).
- `verdict_name` - the verdict vocabulary for display only.
- `repl::run_repl(host, config, input, out)` - the line-REPL core
  (`ReplConfig { default_budget, interactive }`): `help`, `step
  [budget]`, `run [n]`, `show`, `grow-case <id>`, `save/load <path>`,
  `export-native <dir>`, `quit`. Blank lines and `#` comments are
  ignored. `run` without a count stops on the closing verdicts
  (`goal_attained`, `domain_exhausted`) and resumes across `plateau`
  and `budget_exhausted` per the authored vocabulary; a bare `run`
  that reaches 1000 batches without closing refuses `loop_run_cap`.
  Errors print `error <code>: <message>` and the session continues;
  only a failed session start or a broken stream ends the run.
- `export::export_native(host, out_dir)` - the native epoch export
  (bead emath-8k3zw): emits the artifact crate through the SHARED
  constructor-crate emitter (`emath-rust-backend`'s
  `constructor_crate`, the same core `emath build` uses, so the two
  emissions are byte-identical by construction), then generates and
  compiles a standalone std-only epoch-host bin as the artifact's
  path sibling (the compiled-probe doctrine). The host bin drives
  `Seed<Name>(0)` / `Step<Name>(state, budget)` over the artifact
  ABI, projects the ledger, enforces the ledger law, and writes an
  `emath.scratch.v1` checkpoint that MIRRORS the exec-ir interchange
  (records alphabetical by authored field name, taken from the
  emission's own authored record list - the backend's keyword escape
  is non-injective, so the mirror never guesses names back; sequences
  comma-space; the JsonWriter envelope shape) instead of linking
  emath-exec-ir. Cross-lane parity (native checkpoint byte-identical
  to the VM lane's for the same schedule) is enforced by tests, not
  by shared code; the state shape is parsed from the emitted artifact
  itself and checked against the loop-state contract at generation
  time. The bin's transcript mirrors the REPL's seed/batch/end lines
  plus a measured `timing native_step_total_ns` line - timing is
  reported, never claimed as a ratio. Before emitting, the export
  re-admits the module on disk and refuses `loop_export_emit` if it no
  longer mints the session's meaning id: the VM lane steps the
  open-time admission, and an export over edited bytes would pair a
  stale meaning id with new math.

## Dream driver contract (bead emath-gav6o)

A dream target is an ordinary module: a Step/Seed session surface
whose module owns every loop law, plus the self-escalating curriculum
(`next_cases` grows the frozen audit when the incumbent masters it)
and, where the target wants the compute law, graduation (retiring
records that can never win on any future frozen set). The driver
`dream::run_dream(module, target, config)` drives ONE level over ONE
module and owns only the dream laws:

- **Resume:** on verdict 4 (budget exhausted, partial-but-committed)
  the logical-unit watermark becomes `2 * used + budget_step`. The
  doubling shape is the law: the freshness law re-charges the whole
  retained archive from ordinal 0 every batch, so a linear ladder
  smaller than the per-batch refresh cost locksteps forever (the same
  first records re-probed, the margin never accumulates); doubling
  the committed spend always outruns any finite per-batch cost. Every
  unit stays charged - only the watermark moves.
- **Plateau:** consecutive verdict-3 batches close the level at
  `plateau_close` (default 3). Any verdict-0 batch (promotion or
  audit growth) resets the counter: a level only closes after the
  plateau survives every growth cycle the module's curriculum can
  attempt.
- **Close:** verdict 2 (domain exhausted) and verdict 1 (goal)
  close immediately. An unknown verdict refuses `dream_verdict`.
- **Stop:** `max_batches` is the external stop button (budget-halted
  batches count; they commit too). A stopped run returns no level and
  emits nothing. There is no self-scheduler: nothing re-invokes the
  dream on its own.
- **Emission:** at close the driver emits `level_001.emath` into
  `out_dir` - the discovery audit trail. The incumbent key, the
  frozen case set, the incumbent's observed predictions over the
  whole world, and the observed world table are pinned as literals
  with authored tests (the `LevelData` pin and the `frozen_errors`
  recomputation), and the driver verifies that certificate
  in-process before reporting the level verified. Re-verification
  later is cheap: `emath test` on the level file. The emission seam
  is the authored pair `dream_world` (one scalar input, the case table
  as `sequence(Int)` or `sequence(Rat)`) and `dream_pred` (two inputs
  of the world's carrier, the prediction): the world's element
  carrier is the seam's carrier, the pred's declared inputs must pair
  with it (uniform, matching the world), and the driver's Int key and
  case ordinal ride the engine's exact scalar widening into
  Rat-declared inputs. A module without the pair, a mixed world
  table, or a mismatched pred refuses `dream_emission_surface` at
  open when an out_dir is configured (fail fast, no driving). An
  existing level file refuses `dream_level_exists` - the dream never
  overwrites a level.

`DreamConfig { budget0, budget_step, max_batches, plateau_close,
out_dir }` (defaults 64 / 64 / 10,000 / 3 / none); `DreamOutcome {
stop, level, resumes }`; `LevelRecord` is the level journal entry
(close reason, batches, used units, incumbent key and score, frozen
case set, path, certificate).

`DreamRun` is the batch-granular form of the same driver:
`DreamRun::open(module, target, config)` (or `DreamRun::restore` from
a checkpoint plus a `DreamLadder`), then `advance() -> BatchOutcome`
runs exactly one `Step` call (the only commit point) and applies the
ladder/plateau/close laws; `finish()` returns the `DreamOutcome`.
`run_dream` is a loop over it with identical behavior.

## Dream runs and dashboard contract (B4, bead emath-3vium)

Design authority: `internal/design/dream-dashboard/SPEC.md`. A run is
one `DreamRun` over one module and target, owned by a driver process
that writes a run directory; any number of viewers read it. The driver
adds no loop policy and the viewer computes no mathematics.

- **Run directory** (`dream::rundir`): `create_run(root, name,
  overwrite, RunConfig, argv)` makes `<root>/<name>/` (default root
  `.emath/runs`, default name `<target-lowercase>-NNN`; an existing
  name refuses `dream_run_exists` unless `overwrite`). Files:
  `journal.jsonl` (canonical), `metrics.jsonl` (measurement),
  `run.json` (context, `emath.dream.run.v1`, rewritten with `ended`
  at the end), `control/requests.jsonl` + `control/acks.jsonl`,
  `driver.lock` (driver pid, removed on clean exit), `driver.log`,
  `checkpoints/bNNNNNN.scratch` (`emath.scratch.v1`) and
  `level/level_001.emath` when the module has the emission seam.
  Whole files are written atomically; journal and metrics lines are
  appended one complete line per write.
- **Journal `emath.dream.journal.v1`:** events `start`, `seed`,
  `records`, `cases`, `batch`, `budget`, `steer`, `evidence`,
  `evidence_rejected`, `level_next`, `level_rejected`, `level`, `end`, with
  contiguous `seq` from 1. Record values render through
  `emath_exec_ir` `render_scratch_value`, so a record reads
  identically in the journal and in a checkpoint. No timestamps, no
  paths except `level.file`, no pids, no measurement. A record without
  `key` faults `dream_record_contract`; a different schema refuses
  `dream_journal_schema`.
- **Driver** (`drive(dir)`, the hidden `emath dream __drive <dir>`):
  a live pid in `driver.lock` refuses `dream_run_live`; otherwise the
  run opens fresh or resumes. Loop: poll control, one batch, journal,
  metrics, checkpoint. Checkpoints every `checkpoint_every` batches
  and at pause, stop, close and on request; the newest
  `keep_checkpoints` plus the final one are retained.
- **Resume law:** the newest checkpoint `C` at or before the last
  complete `batch` line; the journal is truncated after batch `C`'s
  events (a trailing `end` is dropped) and metrics to batches <= `C`;
  the ladder (watermark, plateau streak, resumes) is refolded from the
  kept journal and the checkpoint is loaded. A run is resumable with
  no `end` or an `end` of `stopped`, `batch_cap` or
  `budget_exhausted`; otherwise `dream_run_closed`.
- **Fork:** `fork_run` starts a new run from the source journal
  truncated at `--at` (default the latest checkpoint) plus that
  checkpoint; none at or before `--at` refuses `dream_no_checkpoint`.
- **Evaluation ledger** (`dream::ledger`): a function whose name
  starts with `fact_` is a fact; its completed calls are memoized
  across batches, resumes, forks and runs. The fact identity
  (`emath:facts:v1:<sha256>`, `exec_ir::fact_identity`) hashes every
  reachable fact definition, the object schemas and the checkpoint
  ABI, so editing a fact starts a new ledger. The store is
  `<root>/ledger/<hex>.jsonl` (`emath.dream.ledger.v1`): a header line
  then `{key, value, run, batch}` lines. A driver tails the file by
  byte offset: before each batch it carries what sibling drivers
  appended, after each batch it appends its fresh facts minus any a
  sibling already wrote. Writers hold an exclusive OS file lock and
  readers a shared one, so any number of parallel runs under one root
  share one ledger without duplicates. Facts are flushed on every
  exit path: a committed batch, a batch whose evaluation faulted (a
  memo entry exists only for a call that returned, so its completed
  facts are kept, labelled with the uncommitted batch number), stop,
  cap, close, and any fault that escapes the loop. A hard kill loses
  at most the in-flight batch. A writer trims a torn tail (a writer
  killed mid-write) before appending, and restarts a file with no
  complete line; any other damage refuses `dream_ledger`. Carrying
  changes compute only: journals, verdicts and charged logical units
  are identical with or without the ledger, and with or without
  siblings (a warm cache never flips a budget verdict).
  `metrics.jsonl` fills `cache_lookups`, `cache_hits` and
  `computed_units` (fresh fact calls) per batch, and leaves them
  `null` for a module with no fact functions. `ledger` is not a valid
  run name. A fact whose body ends in a tail call is memoized too:
  every `fact_` frame the tail call replaces is remembered with the
  chain's final value. A step evaluation never suspends, so it
  memoizes only `fact_` calls (no key is built for any other call or
  closure application); results and journals are identical to the
  all-calls memo.
- **World copy and write scope:** `create_run` copies the launch
  module and every local file it imports (a `use a.b` with
  `<module dir>/a/b.emath` on disk, transitively) into `<run>/world/`.
  Level 1 runs from that copy, so later edits to the source never
  reach the run, and the driver writes only under the run directory.
  Runs created before the copy existed still read the launch module.
  A fork copies `world/`.
- **Level chain** (`dream::chain`): the authored pair `dream_carry`
  (`(Int) -> C`) and `dream_next` (`(T) -> C`, T the state type); one
  without the other, or mismatched types, refuses
  `dream_chain_surface`. At a close, equal `dream_carry(0)` and
  `dream_next(state)` are a fixed point and the run closes as before.
  Otherwise the current level's source with `dream_carry`'s
  definitions replaced by one binding of the rendered value (Int,
  Rat, Bool, sequences and records have literals; anything else
  rejects) is written to `world/level_NNN.emath` and verified: it
  opens as the same Step surface and every authored test passes. A
  verified file becomes the next level (fresh Seed, `budget0`
  watermark, evidence carried) and the journal gets `level_next
  {batch, level, file, carry, used, watermark, incumbent, archive,
  case_ids}`; a file that fails gets `level_rejected {batch, level,
  file, reason}`, stays on disk, and the run closes (`level`, `end`).
  Journal batches count across levels. Resume and fork take the level
  from the last kept `level_next`: a checkpoint after the level began
  is restored under that level's file; at the level's first boundary
  the level restarts from its Seed.
- **Inbox** (`dream::inbox`): `<run>/inbox/*.jsonl`, one observation
  per line, `{"subject": int, "case": int, "value": number | "n/d"}`.
  Values are exact (a decimal is its exact rational; exponents and
  non-integers elsewhere refuse). Before each batch the driver takes
  every not-yet-taken file in name order whose text ends in a newline
  (a writer renames a finished file into place). A taken file becomes
  `evidence {batch, file, source, observations}` (`source` = 1-based
  evidence ordinal, observations as `Observation` records) or, on any
  bad line, an empty file, or a Step with no evidence input,
  `evidence_rejected {batch, file, reason}`; neither is retried. The
  Step's evidence input then carries every accepted observation in
  order. The journal is the authority: resume and fork rebuild the
  evidence from kept events, a file whose event a resume dropped is
  taken again at the boundary that first took it, and the driver never
  edits or deletes an inbox file. Evidence is the only way outside
  information enters a run; it never selects what the run does next.
- **Work limit:** `--work N` on `start`, `resume` and `fork` (stored
  as `work` in `run.json`, default the engine's `DEFAULT_WORK`) caps
  the machine work of one step evaluation. Exhausting it ends the run
  `budget_exhausted`, which is resumable, so a world that outgrows the
  cap resumes with a larger `--work` instead of starting over.
- **Parallel runs:** default run names are claimed with an atomic
  directory create, so simultaneous launches get distinct names. A
  driver holds an OS lock on `driver.lock` for its lifetime; that
  lock, not the pid written in the file, refuses a second driver
  (`dream_run_live`), and it dies with its process, so a crashed
  driver never leaves a run locked. The pid is for viewers only.
- **Control channel:** requests `pause`, `resume`, `step`,
  `grow_case(id)`, `stop`, `checkpoint` apply at batch boundaries in
  file order and are acknowledged `applied` or `refused` with a code:
  `step` while running refuses `dream_not_paused`, `grow_case`
  without an id `dream_control_arg`, an unknown action
  `dream_control_unknown`, any request after the end
  `dream_run_ended`; a `grow_case` host fault refuses with its own
  code. Only an applied `grow_case` enters the journal (`steer`).
- **Outcomes:** `end.outcome` is `goal_attained`,
  `domain_exhausted`, `plateau`, `budget_exhausted` (only under
  `max_units`, when the next raise would pass the ceiling),
  `stopped`, `batch_cap` or `fault` (`code` = the HostFault code
  verbatim; the message goes to `run.json`, not the journal).
- **Export:** `export_run(run, batches|records, csv|jsonl)` is a
  deterministic function of the journal; an unknown table or format
  refuses `dream_export`.
- **Viewer** (`dashboard`): `watch::RunWatch` reads complete journal
  lines incrementally from the last byte offset and folds them into
  `model::RunModel`; `render::render(model, view, ctx, w, h) -> Frame`
  is pure: a theme-free `canvas::Canvas` of style tokens plus layout
  and hit regions;
  `dashboard::app` (feature `dashboard`) is the FrankenTUI presenter
  that maps tokens to theme colors, owns the terminal, and writes only
  control requests. The RUN badge precedence is ended, stalled (lock
  pid not alive), paused, pausing/stopping, running. Record state
  follows the archive contract: `counterexample >= 0` is refuted,
  `refusal > 0` is refused (`0` means none). Panels whose data needs
  engine dependencies D1-D5 show their named empty state.
- **Presenter** (`dashboard::app`): FrankenTUI's frame budget is strict
  and generous (its 16 ms default skips presenting any slower frame,
  which left large runs on a blank screen); idle ticks repaint the last
  frame and re-render only on new run data, input or animation. Glyphs
  go through `canvas::Glyphs`: every non-ASCII glyph that FrankenTUI
  measures wider than one cell (or all of them in its ASCII glyph mode)
  is painted as its ASCII stand-in; the canvas keeps the canonical
  glyphs. A crowded body keeps panels in priority order (Score first).
  Run controls the run cannot take (ended or stalled; step while not
  paused) drop their status-bar keycap chip.
- **Fold cost:** each record's ordering fields (score, parent, tier,
  refusal, counterexample, accepted) are decoded once per journaled
  state; the shared record list's groups and the parent index are
  derived once after new events, not per frame. `RunWatch` reads files
  by (device, inode) and offset, so a journal or metrics file rewritten
  by a resume or fork is refolded from its first line.
- **Measured** (not gating; `cargo run --release -p emath-tui-tests
  --example dream_perf`, synthetic 10,000-batch / 50,001-record journal,
  Apple M5 Max): ingest (open + fold) median 229 ms (SPEC target < 2 s);
  160 × 50 Overview frame p95 0.13 ms (target < 8 ms), 9.4 ms for the
  first frame after new events (rebuilds the derived orderings); before
  the fold-cost changes the Overview p95 was 323 ms. Peak RSS of a
  one-shot `watch` of that journal: about 267 MB (debug build); no
  memory target is claimed.

## Experiment host contract (`experiment`, `emath experiment`)

`ExperimentHost::admit(manifest)` validates an `emath.experiment.v1`
manifest. It declares two Cargo crates (each with `Cargo.lock`), a
workload, an authored evaluator (module + function), a pair count, a
warmup count, a seed, a budget (`experiment_ms`, `build_ms`, `trial_ms`,
`output_bytes`, optional `memory_mb`), access (`network`,
`filesystem_write`, `require_isolation`), and an optional
`proposal_cost`. Admission also admits the evaluator. Every evaluator
input must be one of `HOST_FACTS`; anything else refuses
`experiment_evaluator`. Admission then digests sources and workload,
fingerprints rustc, cargo, OS, CPU, parallelism and RUSTFLAGS, and
decides enforcement. Nothing runs at admission.

`ExperimentHost::run(config)` builds each arm with `cargo build --release
--locked --offline` through `emath_build::run_cargo_timed`. Builds are
timed separately from trials. Each session runs warmups, which are
recorded but never sampled. Then it runs paired trials. Pair `i` runs the
candidate first when `candidate_first(seed, i)` is true, and the order is
recorded. Wall time is host-clocked with monotonic `Instant`, from before
spawn to reap. It includes start-up for both arms and has 50 µs polling
granularity. Stdout must be whitespace-separated integers. After all
pairs, the host calls the evaluator with the declared facts and stores
the returned record. The host never ranks, filters or averages.

- **Outcomes:** success, `timeout`, cancellation, `crash` (exit code or
  signal), `invalid_output`, `output_limit`, `spawn_failed`,
  `build_failed`, `build_timeout`, and `experiment_budget` are distinct.
  Any non-success is recorded as a terminal failure or a cancellation,
  and no evaluation follows. The whole-experiment budget spans builds and
  runs across sessions. The authored evaluation is outside that clock.
- **Commit:** state is an `emath.scratch.v1` checkpoint written by
  temp+rename. Its identity is the evaluator meaning id plus an
  experiment digest. A changed identity refuses `experiment_identity`. A
  pair is journaled `pending` before launch. On resume, a pending pair
  becomes an `uncertain` attempt, is never counted, and reruns. Execution
  is at least once; counting is exactly once. A rebuilt binary whose
  digest differs from the measured one refuses `experiment_artifact`.
  The host does not change engine snapshot timing; the checkpoint carries
  only plain carriers (no closures).
- **Audit ledger:** the audit id is a 60-bit prefix of the workload
  content digest. The ledger entry is `pending` before evaluation and
  `consumed` after the decision is committed. The evaluator receives
  every other consumed or pending id, so A → B → A never looks fresh.
  Evaluators that declare `audit_id` must return `consumed` containing it.
- **Enforcement (macOS, `sandbox-exec`):** trials cannot fork, cannot
  exec anything but the arm binary, and cannot read the workload, ledger
  or checkpoint files. Network and writes are denied unless the manifest
  permits them. Builds are denied network and protected reads. Children
  always start with an empty environment. Memory limits use `prlimit
  --as` on Linux (untested here); a memory limit that cannot be enforced
  refuses `experiment_unenforceable`. Without a sandbox,
  `require_isolation` refuses `experiment_isolation`; otherwise each
  unenforced limit is reported as `UNENFORCED`.

Faults: `experiment_read`, `experiment_manifest`, `experiment_evaluator`,
`experiment_isolation`, `experiment_unenforceable`, `experiment_identity`,
`experiment_artifact`, `experiment_state`, `experiment_ledger`,
`experiment_evaluation`.

Conformance: `tests/emath-tui/tests/experiment_host.rs` (real decision
and replay, faster-but-wrong rejection, A → B → A audit reuse, timeout,
cancellation, interrupted resume with an injected pending pair, and
evaluator/workload identity refusals). The following mutants each fail a
test: ignoring the ledger, a constant checkpoint identity, and dropping
the uncertain-attempt record.

No-claims: `sandbox-exec` is deprecated by Apple, and it is not a
hardened security boundary. There is no ledger file locking, so run one
host per ledger. A decision's status 1 means that every observed paired
gain was positive. It is not statistical significance, and it does not
extend to other workloads or machines. Candidates are explicitly
supplied; there is no proposal loop.

## Invariants

- The ledger is immutable and append-only; its length is the revision.
- After every step, the state's own `batch` counter must equal the
  committed revision + 1 (`loop_ledger_law`); on load it must equal
  the checkpoint revision.
- Identity is carried, never computed from the scratch file's content.

## Error model

`HostFault { code, message }`. Engine and scratch faults pass through
with their own codes (`scratch_identity`, `scratch_ledger`,
`bad_budget`, ...); host-owned refusals use `loop_*` codes
(`loop_read`, `loop_parse`, `loop_surface`, `loop_target`,
`loop_ambiguous`, `loop_identity`, `loop_state_contract`,
`loop_ledger_law`, `loop_case_duplicate`, `loop_command`,
`loop_run_cap`, `loop_stream`, `loop_export_emit`,
`loop_export_state`, `loop_export_compile`) and `dream_*` codes
(`dream_config`, `dream_verdict`, `dream_emission_surface`,
`dream_level_exists`, `dream_level_write`, `dream_certificate`;
runs and control: `dream_run_exists`, `dream_run_name`,
`dream_run_missing`, `dream_run_live`, `dream_run_closed`,
`dream_run_ended`, `dream_run_json`, `dream_no_checkpoint`,
`dream_journal_schema`, `dream_journal_parse`, `dream_journal_io`,
`dream_record_contract`, `dream_not_paused`, `dream_control_arg`,
`dream_control_unknown`, `dream_export`, `dream_spawn`, `dream_io`).
No silent guessing.

## Determinism class

Identical module, surface, budget schedule, and scratch produce
identical sessions: the engine is deterministic and the ledger
projection reads state fields only. REPL transcripts are
byte-deterministic: no timestamps, no timings, ordered fields, the
same script over the same module produces identical bytes
(interactive mode adds only the `> ` prompts, which are transcript
decoration, not data).

Dream runs: the same module, language, target, config and applied
`steer` events at the same batches produce a byte-identical
`journal.jsonl`; pausing, stepping, resuming, detaching, attaching,
checkpoint requests, stop + resume and crash + resume never change it.
The dashboard's content is a pure function of the journal prefix it
has read and the view state; with `--no-measure` the rendered frame is
too. Measurement lives only in `metrics.jsonl` and the viewer's frame
timer.

## Unsafe boundary

None. The crate inherits the workspace `forbid(unsafe_code)`.

## Feature flags

`dashboard`: adds the `ftui` dependency and compiles `dashboard::app`,
the terminal presenter. The fold, model, layout and headless render
are always compiled, so the goldens need no terminal. `emath-cli`
enables it.

## Conformance

`tests/emath-tui/tests/host_sessions.rs` drives the valley and targets
session surfaces (`tests/fixtures/constructor/research_step_*.emath`)
batch by batch to the authored outcomes, checks the lift diagnostics
and near-miss problems, and proves scratch resume equality with the
straight run plus the identity and ledger refusals.
`tests/emath-tui/tests/repl_sessions.rs` drives scripted REPL
sessions over the valley surface: the goal walk with pinned batch
lines, byte-determinism, save/load resume, grow-case freeze and
duplicate refusal, budget-halt resume, error continuation, and the
export-native receipt. `tests/emath-cli/tests/loop_cmd.rs` runs the
`emath loop` binary end to end: scripted goal, default budget, auto
target, ambiguity refusal, lift diagnostic, usage fault, and piped
stdin sessions. Mutation probe: disabling the grow-case duplicate
guard fails `repl-grow-case/duplicate-refused`.
`tests/emath-tui/tests/export_native.rs` is the native-lane
acceptance: the export emits and builds both crates, the native
checkpoint is byte-identical to the VM lane's for the same schedule
(cross-lane parity), the native lane is deterministic, a different
schedule diverges, the transcript mirrors the REPL's lines with a
measured timing line, argument faults are typed, a module edited
after the session opened refuses the export, and the SECOND real
surface (the fitting target, whose applied local closure is the
session-surface lift's closure-valued-def pattern) exports with the
same byte parity. Mutation probe: swapping the rational's num/den in
the generated renderer fails `cross-lane-parity/scratch-bytes-equal`.

The dream fixtures (`tests/fixtures/constructor/dream_*.emath`) pin
the dream worlds module-side with authored walks: the linear world
(master k = 11, full audit, plus the trickle surface whose
plateau/growth interleave is pinned verdict by verdict), the
quadratic world (master k = 25, the winner's lineage roots at the
seed), the Goodhart world (the transient cheater quarantined with
counterexample 1, graduated by the incumbent's full-world bound, the
seed honestly refuted at case 0), the unmasterable world (the honest
partial: frozen {0, 1}, four full-world errors), and the plateau
world (an unbounded domain that can never exhaust - only the
plateau law closes it).
`tests/emath-tui/tests/dream_sessions.rs` is the driver acceptance:
the linear close with a mid-walk budget resume and an independently
re-verified level certificate, the dead-start budget ladder, the
unmasterable honest partial, the plateau close at three, the
max-batches stop with no level and no file, the emission-seam
refusal on a module without `dream_world`/`dream_pred`, the
plateau-counter reset on growth (the trickle surface's interleaved
walk closes at the post-mastery plateau run, not the cumulative
count), and the Rat-world program-space dream (a `sequence(Rat)`
world with a Rat-keyed pred closes goal_attained and emits a level
whose world and prediction rows are Rat literals, independently
re-verified). Mutation probes: reverting the resume law to a linear ladder
fails the linear and dead-start cases (the refresh lockstep);
off-by-one in the plateau close fails both batch pins; removing the
verdict-0 reset fails the trickle case's batch pin (cumulative close
mid-walk); truncating the rational row rendering fails the Rat-world
case's literal pins (the level's internal consistency alone cannot
catch a uniformly wrong renderer - the explicit Rat-literal demands
are the discriminator).
`tests/emath-tui/tests/dream_journal.rs` is the run acceptance (SPEC
A1-A7, A12): byte-identical journals for identical runs, pause/step/
resume invariance, stop + resume and crash (truncated journal, dead
lock) + resume equal to the uninterrupted run, a live lock refusing a
second driver, one `steer` for a grow and a fork at b2 replaying it to
the same bytes, parity with `run_dream` on close reason, incumbent,
used, batches and certificate, no context or measurement keys in the
journal, every outcome's `end` (the fault case runs the linear
world's `StepDreamFault` surface, whose second batch reads past the
world and ends `fault` with the engine's own code, not resumable),
and the export tables against
`tests/fixtures/dream/golden/export-batches.csv`. Mutation probes:
swapping the `used` and `delta_used` export columns fails
`a12-export/csv-golden`; ending a fault as `stopped` fails
`a7-outcomes/fault` and `a7-outcomes/fault-not-resumable`.
`tests/emath-tui/tests/dream_dashboard.rs` is the viewer acceptance
(A8-A10, A13): text and style-class goldens for each tab, expanded
panel, overlay, the ended banner and the size tiers
(`tests/fixtures/dream/golden/`), the spacing grid on every golden,
hidden panels costing nothing, and a dead driver shown as stalled.
It also pins the three theme token tables against SPEC §8.1, the
glyph fallback (a wide glyph falls back, its narrow neighbours do not,
and every golden frame has an ASCII stand-in for each glyph), the mouse
regions (tab, expand/close, row select, wheel, keycap, panel switch,
click outside), and the fault-after-steer banner with its fork undo.
The fixture journal is the linear world's two-family trickle surface
(`StepDreamFamilies`). Mutation probes: dropping the tab click fails
`mouse/click-tab`; never attributing a fault to its steer fails
`fault-after-steer/banner`; reversing the panel priority fails
`golden-size-120x36`; a changed Nordic color fails `a8-theme-tables`;
ignoring measured widths fails `glyph-fallback/wide-falls-back`.
It also checks that controls the run cannot take lose their keycap
chip, that a model folded in two halves (rendered in between) renders
exactly like a fresh fold, and that an attached `RunWatch` refolds
after a stop and resume instead of misreading the rewritten journal.
Mutation probes: dropping the per-ingest reset of the derived
orderings fails `incremental-fold-renders-like-a-fresh-fold`; not
refolding a replaced journal fails `watch-follows-a-resume/batches-once`.
`tests/emath-cli/tests/dream_cmd.rs` is the end-to-end A11: `emath
dream` on a pipe returns while the detached driver keeps committing,
and a new `watch` shows the batch the journal holds. Mutation probe:
making `spawn_driver` wait for the driver fails
`driver-outlives-viewer`. Its `dream_evidence` drops inbox files into a
stopped run: a valid file moves the incumbent at the next batch, a bad
file is journaled `evidence_rejected`, an unfinished file is not taken,
and a fork with no inbox reproduces the journal byte for byte from the
kept evidence. Mutation probes: never binding taken evidence fails
`evidence-moves-the-incumbent`; not replaying evidence on resume/fork
fails `fork-replays-evidence`; a wrong decimal scale fails
`taken-once-exact-and-journaled`. Its `dream_revision` drives a Step
that revises its model (`search.revision`) from inbox evidence: plans
0, 1, 5, 6 are measured and the run adopts plan 3, which never was.
Mutation probe: a revision law that never accepts fails
`revised-model-finds-the-unmeasured-plan`. Its `dream_chain` runs three
verified levels (starts 0, 3, 6), rejects the fourth on its authored
test, keeps the source byte-identical, and forks inside level 3 and at
its first boundary reproduce the journal. Mutation probes: resuming
without the kept `level_next` fails both `fork-at-*-reproduces`;
restoring the previous level's checkpoint at a level's first boundary
fails `fork-at-8-reproduces`; skipping authored tests in verification
fails `failing-authored-test-rejects-the-level`. Its `dream_strategy`
carries a strategy genome through the chain: each close revises it once
(`search.strategy`: training screen, then held-out confirmation under a
shared cost ceiling), so the levels run on steps 1, 2, 3 (closing at
batches 13, 20, 25) and the climb stops where the held-out tasks refuse
step 4; a fork inside level 3 reproduces the journal. Mutation probes: a
climb that never accepts fails `evolved-strategy-shortens-each-level`;
dropping the held-out confirmation makes level 2's file fail its
authored test (`level_rejected`), failing all three checks.
Re-bless both with `EMATH_DREAM_BLESS=1`.

## No-claim boundaries

The host does not interpret case semantics, choose budgets, or
evaluate targets. The dream driver owns no loop law either: the
curriculum, the graduation bound, and every verdict are the module's
mathematics; the driver only resumes, counts, closes, stops, and
emits. One `run_dream` call drives one level of one module - level
chaining across worlds is caller orchestration, not claimed here.
The dream has no self-scheduler: unattended multi-level runs are a
caller loop around the stop button, not a driver behavior. The level
file's certificate pins observed data and its own recomputation; it
is evidence, not a theorem. The dashboard (B4) is implemented strictly from the
user's design spec; this crate does not design UI. A dream run is one
level; the engine dependencies D1-D5 (batch report, per-case scores,
results store, native parity lane, family pinning) are not built, and
the SPEC's ingest and frame-time targets are not claimed. Native-lane parity
is claimed exactly as tested: same module, surface, and budget
schedule produce byte-identical checkpoints on the valley and fitting
surfaces; other surfaces carry the same contract but their parity is
proven when their fixtures drive the export test. The epoch host's
timing line is a measurement, not a performance claim.
