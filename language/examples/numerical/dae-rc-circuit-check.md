# dae-rc-circuit.emath — check record

Bead: emath-eoi0.2 (window emath-dae-disposition-b9flv)
Dispatch: emath:dae-example-expose:1:1, mail id 17 (ack_required)

## Syntax verification

CLI gate (via `cargo run -q -p emath-cli --`):

```
cargo run -q -p emath-cli -- check language/examples/numerical/dae-rc-circuit.emath
```

Result: **exit 0, admitted** (no diagnostics). Re-run after the events
surface fix below.

## What changed in the example file

- `events:` previously held two `when ... then:` blocks. The admitted
  grammar (r3-dynamical-03lh, `emath-sema/src/admit/declaration.rs`)
  only admits `event <Name>(field: Type)` declarations — the blocks
  refused with `E-SYN-101` ("only `event <Name>(field: Type)`
  declarations are allowed in `events:`"). Replaced with the named event
  surface:
  `event ThresholdCrossed(voltage: Float64)`.
- The switch actions (`closed: current = 0.0`, `open: der(charge) =
  current`) are the transitions/event-driven next slice, which admission
  explicitly does not claim computes. The event surface is named-only,
  per the admitter comment: "admitting an events section NEVER claims
  event-driven simulation computes".
- `threshold_voltage` remains declared as an input; the exact simulate
  command binds it, plus the algebraic guess for `current` (the generic
  causalized path refuses a missing algebraic guess — pass `--set
  current=...`).

## Execution evidence

Command (all declared symbols bound: inputs, algebraic guess, state):

```
cargo run -q -p emath-cli -- simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit \
  --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set charge=0 --set current=10 \
  --method backward-euler --dt 0.1 --t1 1.0
```

Result: **exit 0**, 11 samples. Trajectory (compound see log; head/tail):

```
t=0.0 charge=0.0 current=10.0
t=0.1 charge=0.90909090912248 current=9.0909090912248
...
t=0.9999999999999999 charge=6.144567105792007 current=3.8554328943890797
```

Physics check: backward Euler on `q' = (V - q/RC)` with dt=0.1, 10 steps
gives `q(1) = 10·(1 − 0.9¹⁰) = 6.144567105792...` — matches the printed
`charge` exactly; `current = (V − q/C)/R` matches at every sample. The
index-1 projection is consistent throughout.

## Disposition evidence

CLI prints the trajectory; the disposition record is the
`simulate_continuous_dispositioned` side channel. The existing
failure-first contract suite for this exact system shape
(`tests/emath-sema/tests/dae_disposition.rs`, causalized index-1 RC,
same structural surface) ran green:

```
cargo test -q -p emath-sema-tests --test dae_disposition
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Contract recording for this model (deterministic; identical inputs →
identical fields):

- `index: One` (1 `algebraic:` unknown — not a plain ODE);
- `differential_states: [charge]`, `constraint_unknowns: [current]`;
- `initialization: Consistent` (t0 projection converged, max
  |residual| ≤ 1e-6);
- `continuation: None` (nothing owed).

Negative lanes from the same suite hold too: missing algebraic guess →
`E-DAE-INIT` typed refusal with `SupplyInitialGuess` continuation (the
runner-side consistent-initialization refusal; at the CLI surface a
missing binding is an uncoded usage error — `error: missing algebraic
guess `current` (pass --set current=...)`, exit 2 — the code appears
only once the solve runs); singular residual (`R=0`) → `Regularize`
refusal, never a trajectory.

## Event locator probes (BronzeCoyote, same bindings, current=0)

- `--event current=5` → exit 0, crossing sample injected at
  `t=0.7262836955484092 charge=5.000000000366084 current=5.000000000092301`.
- `--event ThresholdCrossed=5` → exit 1: `error: event state
  `ThresholdCrossed` is missing` — declared event names are not rootable;
  the locator keys on model variables. Confirms surface-only status of
  the `events:` section.

## Command-line evidence recap

| Gate | Command | Result |
|------|---------|--------|
| check | `emath check .../dae-rc-circuit.emath` (via cargo run) | exit 0, admitted |
| simulate | full command above | exit 0, 11 samples, physics-exact |
| simulate | `--event current=5` variant | exit 0, crossing sample at t≈0.72628 |
| simulate | `--event ThresholdCrossed=5` variant | exit 1, `event state ThresholdCrossed is missing` |
| disposition | `cargo test -q -p emath-sema-tests --test dae_disposition` | 5/5 pass |

---

# Event-execution slice (follow-up: r3-dynamical-03lh ch7)

The events section now carries a payload suite with a condition and an
action, and the generic runner EXECUTES it — the example's behavior
switches at the threshold instead of merely declaring a name.

## What changed

- Example (`dae-rc-circuit.emath`): `event ThresholdCrossed(voltage:
  Float64)` gained a payload:
  ```emath
  events:
      event ThresholdCrossed(voltage: Float64):
          if charge >= capacitance * threshold_voltage:
              voltage = 0
  ```
  Action targets declared input `voltage` (persists into later steps).
- Generic glue (no domain logic):
  - `emath-ir`: `SemanticPackage.events: BTreeMap<DeclarationId,
    Vec<EventDecl>>` (`EventDecl { name, condition: ExprId, action:
    EventAction { target, expr } }`); package-side so no
    `Declaration` literal churns.
  - `emath-sema` admitter: event payload validation + lowering
    (`admit_event_payloads` after definitions/equations; inline_defs
    like residuals). Refusals: E-EVENT-001..005 (malformed payload,
    non-Bool condition, else-arm, non-numeric action, non-Float64
    slot; algebraic targets refused).
  - `emath-exec-ir` runner: conditions evaluated once per accepted
    step; rising edge → 40-iteration bisection (same budget as
    `--event` variable locator); action applies at the crossing and
    persists via a live input map (caller's map never mutated); t0
    holding conditions fire at t0; at most one event per step,
    declaration order, `Trajectory.events: Vec<EventFiring>` log.
  - `emath-cli`: `simulate` prints fired events and the crossing
    sample; `--json` gains an `events` field.

## Execution evidence (exact commands)

```
cargo run -q -p emath-cli -- check language/examples/numerical/dae-rc-circuit.emath
```
→ exit 0, admitted.

```
cargo run -q -p emath-cli -- simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set charge=0 --set current=10 \
  --method backward-euler --dt 0.1 --t1 1.0
```
→ exit 0, 12 samples:

```
event ThresholdCrossed fired at t=0.7262836954523665
t=0.7 charge=4.868418817807165 current=5.131581182261705
t=0.7262836954523665 charge=5.000000000004511 current=-5.000000000004511
t=0.8262836954523665 charge=4.545454545458647 current=-4.545454545387268
t=0.9262836954523664 charge=4.132231404962406 current=-4.132231405223728
t=1.0 charge=3.8485318584229447 current=-3.8485318583720716
```

Physics: crossing bracketed by BE charges q(0.7)=4.8684 < 5 ≤
q(0.8)=5.3349, bisection lands ON the threshold (charge =
5.0000000000045); after the action voltage=0, the discharge is exact
q/1.1 per step (5/1.1 = 4.54545, /1.1 = 4.13223, then the partial
step to t=1), and current = (V − q/C)/R stays consistent through the
projection — including changing sign at the switch.

## Failure-first / mutation evidence

`tests/emath-sema/tests/dae_events.rs` (7 tests):
- Against the pre-glue surface (payload suites silently ignored; no
  firing log) the named contract `events_fire_and_switch` FAILS at the
  `trajectory.events.len() == 1` assertion, and the admission-refusal
  tests fail (malformed suites were silently admitted).
- Mutation-red: with the runner's event slice disabled
  (`events: &[] = &[]`), `events_fire_and_switch` panics at
  `dae_events.rs:123` ("event must fire once") — the test
  discriminates.
- Restored: green.

```
cargo test -q -p emath-sema-tests --test dae_events           # 7/7 pass
cargo test -q -p emath-sema-tests --test dae_events -- events_fire_and_switch  # named, 1/1 pass
cargo test -q -p emath-sema-tests --test dae_disposition      # regression, 5/5 pass
```

## Determinism and budgets

- One event per accepted step; ties break in declaration order.
- Crossing bisection ≤ 40 iterations per firing (same budget as the
  variable locator); step budget 1_000_000 unchanged; conditions are
  evaluated only at accepted step boundaries.
- Replay-deterministic: same source + inputs + policy → identical
  trajectory INCLUDING the firing log (`event_firing_is_replay_deterministic`).
- An event fires at most once per rising edge; unreachable thresholds
  never fire and leave the plain RC trajectory byte-identical
  (`event_below_reachable_threshold_never_fires`).

## Docs

- `language/reference/expressions-equations-state-and-events.md`
  Events section: payload grammar, scheduling semantics, refusal codes,
  determinism, budgets; variable locator and declared-event execution
  are now documented as separate mechanisms.
- `language/examples/README.md` entry 25 + oracle updated.

---

# Transitions slice (follow-up: r3-dynamical-03lh ch7)

The switch dispatches through the **transitions** channel: the event
DETECTS the crossing, the firing `on ThresholdCrossed:` rule applies
the switch. The event's payload carries the condition plus an identity
self-assignment (`voltage = voltage`) because the payload contract is
exactly one action; the behavioral write lives in the transition.

## Final example (dae-rc-circuit.emath)

```emath
events:
    event ThresholdCrossed(voltage: Float64):
        if charge >= capacitance * threshold_voltage:
            voltage = voltage
transitions:
    on ThresholdCrossed:
        voltage = 0
```

`voltage` on the event is a runtime-capture slot: at the crossing it
binds the live input value (`voltage = 10`), injected into the rule's
scope. The transition re-assigns the declared input slot `voltage`,
persisting into all later steps. A bare capture-only `event
Name(field: T)` that never fires would make the transition dead
surface — the payload condition is what the runner schedules.

## Execution evidence (exact commands)

```
cargo run -q -p emath-cli -- check language/examples/numerical/dae-rc-circuit.emath
```

```
cargo run -q -p emath-cli -- simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set charge=0 --set current=10 \
  --method backward-euler --dt 0.1 --t1 1.0
```

Result: **exit 0**, 12 samples. Observed:

```
event ThresholdCrossed fired at t=0.7262836954523665
t=0.7 charge=4.868418817807165 current=5.131581182261705
t=0.7262836954523665 charge=5.000000000004511 current=-5.000000000004511
t=0.8262836954523665 charge=4.545454545458647 current=-4.545454545387268
t=0.9262836954523664 charge=4.132231404962406 current=-4.132231405223728
t=1.0 charge=3.8485318584229447 current=-3.8485318583720716
```

Physics: crossing bracketed by BE charges q(0.7)=4.8684 < 5 ≤
q(0.8)=5.3349, bisection lands ON the threshold (charge =
5.0000000000045); the transition action `voltage = 0` flips `current`'s
sign; the post-switch discharge is exact `charge / 1.1` per step
(5/1.1 = 4.54545, /1.1 = 4.13223, then the partial step to t=1). The
transition changed a real trajectory deterministically. A never-fire
control (threshold 50) leaves the plain RC trajectory: `charge` ends ≈
6.1446 at t=1.

> Resolution (this pass): the shipped example uses the identity-latch
> form above — payload condition + identity action + the transition
> carry the behavioral write — so `check` admits, the runner fires the
> event, and the transition dispatches. Bare capture-only events are
> documented in the reference as declarative surface that is never
> scheduled on its own (`language/reference/expressions-equations-state-and-events.md`).

## MR evidence (pass 6, oracle-free, from `tests/emath-sema/tests/dae_transitions.rs`)

- MR-1 timestep-refinement: BE firing time t(h) closes on t* = ln2;
  |t(h/2)−t(h)| halves as h halves (factors 1.60, 1.94), firing sample
  charge sits ON thr=5 within 1e-6.
- MR-2 no-event identity: unreachable-threshold model (thr=50) reproduces
  the plain-RC trajectory bit-for-bit with an empty firing log and no
  stray dispatch (`voltage = 999` never applies).
- MR-3 payload-vs-transition: the same write `state.y = v` via event
  payload vs `on E:` transition → identical Trajectory.
- MR-4 capture-vs-direct: `state.y = x` (captured param) vs
  `state.y = state.x` (direct read) → identical.
- MR-5 V/thr input-scaling invariance of t_fire (homogeneous degree 0).
- MR-6 re-armed-oscillator firing-count invariance under dt refinement
  (count 3 and y_final 3 for dt ∈ {0.1, 0.05, 0.02}).

### Mutation-kill matrix (summary)

- Bisection 40→4: killed by the `on-threshold <1e-6` charge assertion in
  MR-1 (overshoot 5.05/5.02/5.007).
- Double-apply of a transition: killed by MR-1's firing-count assertion
  (`events.len() == 1`).
- Edge-flip on a condition: killed by 6 tests across MR-2/4/6.
- No survivors. All named contracts discriminate a planted runner bug.

## Test counts

```
cargo test -q -p emath-sema-tests --test dae_transitions   # 27/27 pass
cargo test -q -p emath-sema-tests --test dae_events        # 9/9 pass
cargo test -q -p emath-sema-tests --test dae_disposition   # 5/5 pass
```
> Fresh rerun at pass-8 close (exact command):
>
> ```
> cargo test -q -p emath-sema-tests --test dae_transitions --test dae_events --test dae_disposition
> ```
> → `dae_transitions` 27/27, `dae_events` 9/9, `dae_disposition` 5/5 — all three
> binaries green, 41 tests total.

## Determinism and budgets

- One event per accepted step; ties break in declaration order; conditions
  evaluated only at accepted step boundaries.
- Crossing bisection ≤ 40 iterations per firing (identical budget to the
  `--event` variable locator); step budget 1_000_000.
- Replay-deterministic: same source + inputs + policy → identical
  trajectory INCLUDING the firing log and transition application
  (`transition_fires_and_switches_state`, `re_armed_event_redispatches_transition`,
  `admission_is_replay_deterministic`, `refusal_is_replay_deterministic_for_switched_singular`).
- Mid-run singular switch refuses through the causalized-Newton projection
  (raw `E-DAE-INIT` / `Regularize` text), never a partial trajectory.

## CLI E2E status (pass-8 close)

The `check` / `simulate` CLI gates are currently BLOCKED at compile time
by an external lane, not by this slice: `emath-rust-backend` fails to
compile with

```
error[E0004]: non-exhaustive patterns: `&EmirOp::ExactProductDelta(_, _)` not covered
  --> crates/emath-rust-backend/src/codegen_render.rs:885:11
```

`op_expr`'s strict-backend match lacks the `ExactProductDelta` arm
(the op landed in exec-ir as part of the exact-arithmetic wave).
`crates/emath-rust-backend/src/codegen_render.rs` is exclusively
reserved to the rust-backend lane (SilverMaple); per dispatch
no cross-owner edits. Interp-world DAE execution is unaffected (the
41 tests above are green); the CLI evidence recorded earlier in this
record is the last green state of that gate.

## No-claim

One RC fixture demonstrates the generic event/transition mechanism;
capture semantics do not prove general transition systems (no general
state-machine model is claimed).


