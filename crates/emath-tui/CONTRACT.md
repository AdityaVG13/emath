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
- `LoopSession::save/load` - `emath.scratch.v1` checkpoints through
  the exec-ir scratch contract, plus the host-side laws (state type
  match, batch counter equals revision).
- `value_int/value_rat/value_bool/value_sequence`, `state_int`,
  `incumbent_record` - loop-state field readers for surfaces and the
  REPL. Exact integers read as rationals where a score is expected
  (authored targets may return exact zero as an Int).
- `verdict_name` - the verdict vocabulary for display only.

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
`loop_ledger_law`). No silent guessing.

## Determinism class

Identical module, surface, budget schedule, and scratch produce
identical sessions: the engine is deterministic and the ledger
projection reads state fields only.

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

## No-claim boundaries

The host does not interpret case semantics, choose budgets, or
evaluate targets. The dashboard (B4) is implemented strictly from the
user's design spec; this crate does not design UI. Native-lane parity
is B5's acceptance, not a claim of this crate.
