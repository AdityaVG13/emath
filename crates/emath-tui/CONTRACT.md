# emath-tui Contract

## Purpose and layer

The loop surface crate: the headless research-loop host core (B2),
the `emath loop` line-REPL (B3), and the dashboard implementing the
user's design spec (B4). The host core is lane-agnostic session
bookkeeping over the VM engine; the native lane (B5) re-implements the
same `emath.scratch.v1` contract over the artifact ABI. This crate owns
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
  (records alphabetical by emath field name - the inverse of the
  backend's keyword escape orders and names them - sequences
  comma-space, the JsonWriter envelope shape) instead of linking
  emath-exec-ir. Cross-lane parity (native checkpoint byte-identical
  to the VM lane's for the same schedule) is enforced by tests, not
  by shared code; the state shape is parsed from the emitted artifact
  itself and checked against the loop-state contract at generation
  time. The bin's transcript mirrors the REPL's seed/batch/end lines
  plus a measured `timing native_step_total_ns` line - timing is
  reported, never claimed as a ratio.

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
`loop_export_state`, `loop_export_compile`). No silent guessing.

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
measured timing line, and argument faults are typed. Mutation probe:
swapping the rational's num/den in the generated renderer fails
`cross-lane-parity/scratch-bytes-equal`.

## No-claim boundaries

The host does not interpret case semantics, choose budgets, or
evaluate targets. The dashboard (B4) is implemented strictly from the
user's design spec; this crate does not design UI. Native-lane parity
is claimed exactly as tested: same module, surface, and budget
schedule produce byte-identical checkpoints on the valley surface;
other surfaces carry the same contract but their parity is proven
when their fixtures drive the export test. The epoch host's timing
line is a measurement, not a performance claim.
