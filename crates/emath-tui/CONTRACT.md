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
  with the target's closures lifted to module-level `Make*` functions;
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
`dream_level_exists`, `dream_level_write`, `dream_certificate`).
No silent guessing.

## Determinism class

Identical module, surface, budget schedule, and scratch produce
identical sessions: the engine is deterministic and the ledger
projection reads state fields only. REPL transcripts are
byte-deterministic: no timestamps, no timings, ordered fields, the
same script over the same module produces identical bytes
(interactive mode adds only the `> ` prompts, which are transcript
decoration, not data).

## Unsafe boundary

None. The crate inherits the workspace `forbid(unsafe_code)`.

## Feature flags

None.

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
user's design spec; this crate does not design UI. Native-lane parity
is claimed exactly as tested: same module, surface, and budget
schedule produce byte-identical checkpoints on the valley and fitting
surfaces; other surfaces carry the same contract but their parity is
proven when their fixtures drive the export test. The epoch host's
timing line is a measurement, not a performance claim.
