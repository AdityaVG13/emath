# Running a simulation

A simulation advances a state according to a rule you write.
In emath, the state, rate law, and stepping method are ordinary
definitions. You run them with `emath run`, not a separate
`emath simulate` command.

This page models decay:

$$\frac{dy}{dt} = -y,\qquad y(0)=1.$$

The exact solution is $y(t)=e^{-t}$. We approximate it by stepping
through time. The runnable file is
[`docs/guide/examples/decay.emath`](examples/decay.emath).
Commands run from the repo root; see [setup](README.md#setup).

## 1. Write the state and rate

Import the trajectory tools:

```emath
use analysis.evolution.trajectories
```

The import brings in `SimState`, which holds time `t: Rat` and a list
of state values `y: sequence(Rat)`. Our model has one value, `y[0]`.
The rate function returns its time derivative, not the next state:

```emath
emath function rate:
    inputs:
        t: Rat
        s: SimState
    outputs:
        result: SimState
    definitions:
        result = SimState: {t: t, y: [-s.y[0]]}
```

The returned `t` field does not specify the time increment. The stepper
uses the supplied step size. A model with several variables puts their
derivatives in `y` in the same order as the state.

## 2. Choose a method and endpoint

```emath
emath function decay:
    inputs:
        end_time: Rat
        step_size: Rat
        budget: Int
    outputs:
        result: RolloutResult
    definitions:
        start = SimState: {t: 0 / 1, y: [1 / 1]}
        result = rollout_to(rate, euler_tableau(0), 1, step_size, 1, end_time, start, budget)
```

`rollout_to` takes these arguments, in order:

| Argument | Meaning here |
| --- | --- |
| `rate` | The time derivative |
| `euler_tableau(0)` | Forward Euler's coefficients |
| First `1` | Claimed method order, checked against the coefficients |
| `step_size` | Positive size of a full step |
| Second `1` | Forward direction; `-1` means backward |
| `end_time` | Requested endpoint |
| `start` | Initial time and state |
| `budget` | Maximum accepted steps |

Euler updates $y$ by $y_{\mathrm{new}}=y+h(-y)$. With
$h=1/2$, each step halves the current value:

| Time | Euler value |
| --- | --- |
| 0 | 1 |
| 1/2 | 1/2 |
| 1 | 1/4 |

## 3. Test the model

```console
$ emath check docs/guide/examples/decay.emath
$ emath test docs/guide/examples/decay.emath
ok endpoint --
ok budget_halt --
ok clipped_step --
ok zero_step_refused -- demanded fault `non_positive_step`
ok rows_include_initial_state --
ok two_rk4_steps --
6 authored tests passed
```

The examples check both computed values and what the run says it
completed. A zero step is deliberately refused, so that refusal is
a passing test.

## 4. Run to the endpoint

```console
$ emath run docs/guide/examples/decay.emath --function decay --set end_time=1 --set step_size=1/2 --set budget=2
RolloutResult(accepted_t=1/1, attempted_t=1, complete=true, final_state=SimState(t=1/1, y=[1/4]), n_steps=2)
```

Read the result before treating it as an answer:

- `complete=true`: the requested endpoint was reached.
- `accepted_t=1/1`: time of the last accepted state.
- `final_state`: that state's time and values.
- `n_steps=2`: accepted steps.
- `attempted_t`: the requested endpoint when complete, or the next
  attempted step time when the step budget stops the run.

Integer-valued rationals can print as `1` or `1/1`; both mean the same
value.

### An insufficient step budget

```console
$ emath run docs/guide/examples/decay.emath --function decay --set end_time=2 --set step_size=1/2 --set budget=1
RolloutResult(accepted_t=1/2, attempted_t=1/1, complete=false, final_state=SimState(t=1/2, y=[1/2]), n_steps=1)
```

This is **not** an estimate of $y(2)$. It is a state at $t=1/2$.
Increase `budget` to reach the endpoint.

The command still exits 0: it computed the `RolloutResult` value
successfully. `complete=false` is a field in that value. Scripts must
check it, not just the command's exit code.

This step budget is separate from `emath run --work N`, which caps
engine work. Engine-work exhaustion returns a partial receipt and
exit 3 rather than this completed value.

### An endpoint between full steps

```console
$ emath run docs/guide/examples/decay.emath --function decay --set end_time=3/4 --set step_size=1/2 --set budget=2
RolloutResult(accepted_t=3/4, attempted_t=3/4, complete=true, final_state=SimState(t=3/4, y=[3/8]), n_steps=2)
```

The final step is shortened from $1/2$ to $1/4$. It takes
$y=1/2$ to $(1/2)(1-1/4)=3/8$. The driver does not overshoot
and relabel a later state as the endpoint.

## 5. Save a trajectory

The example's `trace` function uses `rollout_trace` to return a fixed
number of steps. Each row is `[time, y[0], y[1], ...]`, starting with
the initial state:

```console
$ emath run docs/guide/examples/decay.emath --function trace --set step_size=1/2 --set steps=2
[[0/1, 1/1], [1/2, 1/2], [1/1, 1/4]]
```

Redirect stdout to save the displayed rows, or add `--json` for a
structured constructor receipt. The receipt contains the result as its
payload; it is not a CSV file.

`rollout_trace` stores every row, so storage grows with step count.
For a running total without storing every state, use `rollout_fold`.
For just an endpoint, use `rollout_to` as above.

## 6. Separate arithmetic accuracy from model accuracy

All these numbers use `Rat`. Their arithmetic has no floating-point
rounding, but Euler still approximates the differential equation.
Exact arithmetic does not make the time-stepping method exact.

At $t=1$, the true solution is about 0.367879. Our two Euler steps
give 0.25. Smaller steps reduce this error for this problem; a different
method can reduce it much further.

The file also defines `decay_rk4`, using the same rate and initial state.
The changed call is:

```emath
        result = rollout_to(rate, rk4_tableau(0), 4, step_size, 1, end_time, SimState: {t: 0 / 1, y: [1 / 1]}, budget)
```

Change both the coefficients and claimed order, not just the order:

```console
$ emath run docs/guide/examples/decay.emath --function decay_rk4 --set end_time=1 --set step_size=1/2 --set budget=2
RolloutResult(accepted_t=1/1, attempted_t=1, complete=true, final_state=SimState(t=1/1, y=[54289/147456]), n_steps=2)
```

This is about 0.368171, closer to the true value. It is still an
approximation, not a certified error interval. Neither
`complete=true` nor the checked method order proves an error bound for
your model. Sharp changes, unstable equations, and stiffness can demand
smaller steps or a different method.

## Refusals and next steps

```console
$ emath run docs/guide/examples/decay.emath --function decay --set end_time=1 --set step_size=0 --set budget=2
error: non_positive_step: mathematical method refused: non_positive_step
```

This is a named refusal, exit 4. Other important checks include:

| Code | What to fix |
| --- | --- |
| `direction_mismatch` | Endpoint must lie in the chosen direction from the initial time |
| `bad_direction` | Direction must be `1` or `-1` |
| `bad_step_count` | Step budget must not be negative |
| `dim_mismatch` | Rate and state must have the same number of components |
| `order_conditions_unmet` | Claimed order needs supporting method coefficients |

Start with a model whose small steps you can calculate by hand.
Keep tests for initial values, endpoint values, clipped steps, and
budget stops. Then change the rate law or add components.

The installed library includes adaptive trajectories, implicit steps,
and event handling under `analysis.evolution`. Find entry points with
`emath api --search "rollout"` and read their contracts before choosing
a method. For comparing numerical representations, see
[running and testing](running-and-testing.md).
