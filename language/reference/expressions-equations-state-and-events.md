# Expressions, Equations, State, and Events

## Binding and equality

`=` binds a name or derivative. `==` states an equation or comparison.

```emath
definitions:
    energy = 0.5 * mass * velocity^2

equations:
    mass * derivative(velocity) == force
```

The compiler never silently converts one meaning into the other.

## Expressions

The language admits numeric and Boolean literals, names, calls, tuples, vectors, matrices, tensors, sets, path-prefixed records, indexing, slicing, conditionals, and mathematical binders.

Common operators and builtins include:

```text
+ - * / ^
== != < <= > >=
and or not ==> <==>
min max abs floor ceil round sign is_finite
sqrt cbrt exp ln log2 log10 sin cos tan tanh sinh cosh atan atan2
recip fract hypot mod lerp clamp pow
sum product mean length norm dot transpose einsum
gamma beta erf zeta lambert_w0 elliptic_k elliptic_e elliptic_pi
```

Special-function calls have matching `<name>_error_bound` accessors.
Poles and exits from the declared real carrier are runtime refusals
(`E-SPECIAL-POLE`, `E-SPECIAL-DOMAIN`); `elliptic_pi` currently refuses
`E-SPECIAL-NOT-IMPLEMENTED`.

Implication is right-associative. Precedence from low to high is `<==>`, `==>`, `or`, `and`, comparisons, then arithmetic.

## Binders

Every binder follows `keyword variable in domain: body`:

```emath
sum i in 0..n: values[i]
product i in 1..=5: i
forall x in domain: valid(x)
exists x in candidates: accepts(x)
integral x in 0.0..1.0: x * x
```

Multiple variables and a filter are allowed:

```emath
sum i in 0..n, j in 0..m if i + j < k: matrix[i, j]
```

Empty folds return their identities: `0` for sum, `1` for product, `true` for forall, and `false` for exists. Bound variables are lexically scoped and may shadow outer names.

Finite numeric integrals use composite Simpson quadrature. A declared measure is a separate world and is never inferred from this syntax.

## Sequences and generating functions

Indexed rows define a memoized sequence when every self-reference strictly decreases the index:

```emath
fib[0] = 0
fib[1] = 1
fib[n] = fib[n-1] + fib[n-2]
value = fib[n]
```

The admitted recurrence is a finite linear combination of earlier terms with contiguous base cases. A same-index or forward reference refuses `E-SEQ-TERMINATION`; malformed or missing base cases refuse `E-SEQ-RECURRENCE`.

`generating_function(initial, recurrence, budget)` constructs the same value explicitly. Recurrence coefficients are ordered by offset, so `[1, 1]` means `a[n] = a[n-1] + a[n-2]`. `coefficient(f, n)` (or `f[n]`) extracts a coefficient, and `convolution(f, g, count)` returns the first `count` coefficients of the Cauchy product. Budgets must be finite nonnegative integers no larger than one million (`E-SEQ-BUDGET`).

## Conditional expressions

```emath
value = if x > 0: x else: -x
```

`cases` is a total piecewise expression:

```emath
value = cases x:
    | x > 0 => 1.0
    | x < 0 => -1.0
    | else => 0.0
```

An `else` arm is required (`E-SYN-110`). Conditions must be Boolean and all result arms must have compatible types.

`match` is expression sugar for literal dispatch:

```emath
value = match x { 0.0 => 1.0, other => other * 2.0 }
```

Patterns are literals, `_`, or a binding name. The final arm must be a catch-all. Formatting expands `match` to canonical `cases` form.

## Claims and approximations

A limit or series is a claim, not a numeric computation:

```emath
limit x -> 0: sin(x) / x
limit x -> 0+: 1 / x
series n in 0..inf: a[n]
f(n) ~~ g(n)
```

Use `sample_limit` when a numerical approximation is intended:

```emath
estimate = sample_limit x -> 0: sin(x) / x
```

Approximate equality requires a tolerance:

```emath
invariant:
    measured ≈ predicted within rtol=1e-9, atol=0
```

ASCII `~=` is equivalent to `≈`. A missing tolerance is `E-APPROX-TOL`. At runtime the check is `abs(left-right) <= atol + rtol*abs(right)`; failure refuses the run.

## Automatic differentiation

```emath
dx = derivative(expression) wrt x
px = partial(expression) wrt x holding pressure
dt = total(expression) wrt time
g = grad(loss)
```

`derivative` and `partial` use forward-mode automatic differentiation. A partial derivative requires an explicit `holding` set. `grad` uses reverse mode and returns derivatives with respect to all declaration inputs.

`jacobian(body) wrt v1, v2, ...` (Track A3, bead emath-9bj1) computes the Jacobian as a value: a list body `[f1, f2]` differentiates component-wise (one row per component, one column per `wrt` variable), a scalar body yields a single row (`Matrix[1, n]`). Ordering is source order: row `i` is component `i` of the list in written order and column `j` is the `j`-th `wrt` variable in written order; no sorting, deduplication, or reordering ever applies. A component that is not a scalar number (vector, matrix, or a nested `jacobian`) refuses with `E-TYPE-012`; it is never silently flattened. The form is parse-time sugar for a matrix literal of `derivative` cells, so it uses the same forward-mode engine as `derivative`; no second engine and no domain ops. Unsupported shapes and non-numeric (nondifferentiable) bodies are typed refusals (`E-TYPE-012`); a `wrt` name that is not an input refuses with the derivative form's input-scope code (`E-TYPE-010`). `hessian` is not admitted yet; it refuses as an unknown keyword.

## Solving and optimization

```emath
root = solve(residual) wrt x
minimum = minimize(loss) wrt x, y
maximum = maximize(score) wrt x
```

The supplied runtime values are initial guesses. Non-convergence, singular derivatives or Hessians, and wrong curvature are refusals, not fabricated answers. Optimization strategy names are explicit; methods never gain authority merely by being declared.

`solve` runs Newton's method with a deterministic robustness fallback (Track A3, bead emath-9bj1): when the derivative vanishes or the residual/step becomes non-finite, the solver scans a fixed geometric grid (alternating ± steps around the seed, ×8 growth, 48 levels) for a sign-changing bracket and bisects it with a fixed 120-iteration budget. A root is reported only when `|residual| < tolerance`; no bracket; or a divergent bisection; is a typed refusal, never a hang and never an invented root. All scan constants are fixed, so the fallback is bit-deterministic across runs and seeds.

## Arrays and spatial operators

```emath
x = vector[0]
y = matrix[1, 2]
plane = tensor[0, :, :]
```

Indexing drops selected axes; slicing with `:` keeps them. Spatial builtins include gradient, divergence, Laplacian, and boundary-specific variants for supported one-, two-, and three-dimensional carriers.

Einstein summation is available through `einsum`. Bare indexing never silently becomes Einstein notation.

## Notation packs

Notation packs are opt-in imports. Their glyphs are not ambient.

### Braket notation

```emath
use sci::physics::notation::braket(convention = physics)

zero = |0⟩
overlap = ⟨0|1⟩
projector = |1⟩⟨1|
```

The admitted carrier is a real two-level system. Basis labels outside `0` and `1` refuse. Unmounted glyphs are `E-SYN-157`.

### Nabla notation

The nabla pack maps declared gradient, divergence, curl, and Laplacian glyphs to the corresponding spatial builtins. Dimensional mismatches, such as three-dimensional curl on a two-dimensional field, refuse.

## Graph literals

```emath
net = graph { 1, 2, 3; 1 --> 2, 2 -[0.5]-> 3, 1 -[2.0]- 3 }
```

Edges are:

| Syntax | Meaning |
|---|---|
| `a --> b` | directed, weight 1 |
| `a -[w]-> b` | directed, weight `w` |
| `a - b` | undirected, weight 1 |
| `a -[w]- b` | undirected, weight `w` |

Edge syntax is valid only inside a graph literal. Malformed or dangling edges refuse at parse time.

## String interpolation

```emath
headline = "x = {x:.3f}, path = {model.coeff}, literal {{raw}}"
```

A hole may contain only a name or dotted path. Calls, indexing, and arbitrary expressions are refused. The fixed format suffix is `.Nf`; doubled braces escape literal braces. String templates evaluate to NFC-normalized `Text`. The intentionally small text surface is `nfc`, `text_length`, equality, report construction, and pure rendering; arbitrary text-processing operations remain refused.

## Definitions and equations

Definitions are directed and evaluated in source order:

```emath
definitions:
    current = (voltage - charge / capacitance) / resistance
```

Model equations may contain rates, algebraic definitions, or implicit residuals:

```emath
equations:
    der(position) = velocity
    mass * der(velocity) == force
    current = (voltage - charge / capacitance) / resistance
    der(charge) = current
```

A scalar equation containing `der(state)` may be causalized into an explicit rate. The transformation is recorded.

## Implicit DAEs

Declare coupled algebraic unknowns separately and write a square residual system:

```emath
emath model RC:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
```

At each step the runner solves the residuals with a finite-difference Jacobian and then advances the differential state. Generated Rust uses the same solve and returns `Result` on non-convergence. The number of residuals must match the algebraic unknowns plus implicit rate unknowns; unused algebraic values are refused.

## Simulation

`emath simulate` supports fixed-step Euler, RK4, adaptive RK45, implicit backward Euler, and symplectic velocity Verlet. `--model Name` selects a model when a file declares more than one. Inputs, state, and algebraic guesses are supplied with `--set name=value`. Adaptive stepping is enabled by explicit absolute and relative tolerances.

Select the structural method with `--method backward-euler` or `--method velocity-verlet`. Velocity Verlet requires two scalar states satisfying `q' = v` and `v' = a(q)`; dissipative or velocity-dependent acceleration refuses `E-ODE-002`. Invalid time steps, non-finite coefficients, and nonlinear solve failure are typed refusals.

## Events and transitions

Two cooperating, generic surfaces (r3-dynamical-03lh ch7) let a
continuous model switch deterministically at a detected crossing. An
`events:` section declares named events that **detect** a rising
condition; a `transitions:` section declares rules that **dispatch** the
switch on an `on <Event>:` trigger, re-assigning declared `inputs:` /
`state:` slots. Either mechanism can carry an action; the common split is
"event declares the crossing, transition dispatches the action."

### The `events:` section (admission grammar)

`events:` declares named events; `event Name(field: Type)` or no-arg
`event Name`; on `emath model` declarations. Admission validates the
surface: duplicate names refuse `E-NAME-022`, anything that is not an
event declaration refuses `E-SYN-101`.

A declared event may carry a payload suite of exactly one deterministic
arm: a Boolean condition and one assignment action on a declared
`inputs:` or `state:` Float64 slot:

```emath
events:
    event ThresholdCrossed(voltage: Float64):
        if charge >= capacitance * threshold_voltage:
            voltage = 0
```

Bare `event Name(field: Type)` declarations without a payload suite are
admitted as declared surface and never scheduled on their own; they only
become meaningful when a `transitions:` rule dispatches on a fired event,
and the firing condition then comes from a payload-bearing event of the
same name (an action-less suite is refused: the payload arm must contain
exactly one assignment).

### The `transitions:` section (grammar and actions)

`transitions:` appears after `events:` (parser admits either order; the
convention keeps `events:` first). Each rule is a trigger clause with one
or more assignment actions:

```emath
transitions:
    on ThresholdCrossed:
        voltage = 0
        state.counter = state.counter + 1
```

Every `on <Event>:` trigger must name an event declared in the same
declaration's `events:` section (`E-TRANS-001`). Each action is an
assignment whose target is a declared input or state slot; either a bare
name (`voltage = 0`, re-assigning a declared input/state) or a dotted
`state.<name>` form that unambiguously addresses a state slot. Actions
against `algebraic:` unknowns are refused (`E-TRANS-005`): the Newton
projection owns those. The action value may reference the fired event's
captured parameters and any declared input/state. When an event fires,
the runner applies the event's own payload action (if any) and then every
matching `on <Event>:` rule in declaration order; each rule's actions run
in rule order (for the same target, the last write wins).

### Event-parameter capture semantics

A declared parameter `f: T` on an event is a **runtime-capture slot**, not
a definition input. When the event fires, each parameter binds the live
value of the **same-named** model variable (a declared `inputs:`, `state:`,
or `algebraic:` name) at the crossing; never a fixed argument. Parameters
are in scope inside the event's payload and inside the matching
`on <Event>:` rule's actions, so a transition can snapshot the crossing
value into a slot:

```emath
events:
    event Snap(charge: Float64):
        if charge >= capacitance * threshold_voltage:
            voltage = 0
transitions:
    on Snap:
        state.captured_charge = charge
```

Here `charge` inside `on Snap:` is the value captured at the crossing.
If an event parameter matches no declared input/state/algebraic variable,
there is no capture source and admission refuses `E-TRANS-006` (naming the
parameter and the event). At a firing whose capture is otherwise missing
at runtime, the refusal is `E-TRANS-007`.

### Scheduling

- **Once per rising edge.** Conditions are evaluated once per accepted
  step. An event fires exactly when its condition rises (false → true)
  across a step; a condition that never rises never fires.
- **t0-holds fire at t0.** A condition already true on the initial sample
  fires at `t0`, before the first sample is pushed, so the first sample
  already carries the switch.
- **At most one event per accepted step**, the deterministic tie-break.
  Ties across events break in declaration order.
- **Crossing bisection ≤ 40 iterations** per firing; the same
  `--event`-locator budget; snapping a sample on the threshold.
- **Step budget 1_000_000** accepted steps per trajectory, unchanged.
- Conditions and capture values bind inputs/state through the same
  lowering path as definitions, so projected algebraic values participate
  in the condition.

### Determinism

Same source + inputs + policy → an **identical firing log** (every fired
event name and crossing time) and **identical transition application**.
Conditions are evaluated only at accepted step boundaries; declaration
order and the at-most-one-per-step rule remove run-to-run ambiguity; the
firing log is part of the replayed `Trajectory` (replay-tested, including
admission replay and refusal-text reproducibility). No RNG; the harness is
deterministic-by-contract.

### Typed refusals

Admission (events payload): `E-EVENT-001` (not a single if/assign pair;
indexed/dotted or unknown target; or an `algebraic:` unknown as target;
the Newton projection owns those), `E-EVENT-002` (condition not Boolean),
`E-EVENT-003` (`else` arms; the contract is one condition, one action),
`E-EVENT-004` (action not numeric scalar), `E-EVENT-005` (target not a
Float64 scalar slot). Runtime (events): `E-EVENT-006` (condition did not
evaluate to `Bool` at the step), `E-EVENT-007` (action target not bound in
the live inputs map / not a bound input or state at `t`, or an expression
name is unbound; pass `--set name=...`), `E-EVENT-008` (event expression
refused or faulted during evaluation), `E-EVENT-009` (event action value
non-finite; NaN/±Inf never poisons a slot, the run refuses).

Admission (transitions): `E-TRANS-001` (`on <Event>:` names an event not
declared in `events:`, or there is no `events:` section), `E-TRANS-002`
(action target is not a declared input/state slot: bare unknown name,
dotted `state.<missing>`, deep path), `E-TRANS-003` (rule body is not an
assignment or the action value is non-numeric), `E-TRANS-004` (empty
`on <Event>:` body), `E-TRANS-005` (action targets an `algebraic:`
unknown), `E-TRANS-006` (event parameter matches no declared variable, so
no capture value exists). Runtime (transitions): `E-TRANS-007` (event
parameter has no capture value at `t`; a transition targets a non-state;
or the target is not bound in the live inputs map), `E-TRANS-008`
(transition action value non-finite; never poison, the run refuses).

All event/transition faults are typed refusals, never silent drops. In
particular, a **mid-run singular switch** (for example a transition that
rewrites `resistance = 0`, making the causalized residual independent of
the algebraic unknown) refuses through the causalized-Newton projection
with the raw `E-DAE-INIT` / `Regularize` typed refusal text; the run
returns the refusal and **never a partial trajectory**.

> No-claim: one RC fixture (`dae-rc-circuit.emath`) demonstrates this
> generic mechanism; capture semantics do not prove general transition
> systems (no general state-machine model is claimed).

### The separate `--event` variable locator

`--event name=value` is the independent variable-based tool: it roots on
model variables (state, input, or algebraic unknown), detects the first
zero crossing of `(variable − value)`, injects a sample there, and stops.
Declared event names are not roots for `--event`; it shares the 40-iteration
bisection budget but is otherwise unrelated to the scheduled event /
transition mechanism above.

## Polynomial, control, graph, and probability calls

Domain capabilities use ordinary calls rather than new expression grammars. Examples include polynomial evaluation and multiplication, linear solves and decompositions, shortest paths and spectral graph operations, transfer-function evaluation, and seeded probability sampling. Their exact names, shapes, numeric policies, and refusal codes are documented in `standard-library-constitution.md` and `language/stdlib/cells/`.

## Unit and dimension queries

`unit of expression` and `dimension of expression` inspect admitted quantities. These queries return semantic metadata and do not reinterpret units as runtime numbers.

## Unsupported forms

The following remain explicit refusals: general recursive definitions without a termination policy, action-functional and variation syntax, arbitrary text-processing outside the declared `core::text` operations, and callbacks inside declarative figure specifications. Fit-goal syntax is implemented (04 §5.3); see [`goals-requests-strategies-and-resolution.md`](goals-requests-strategies-and-resolution.md) for the generic fit program surface and the honest unresolved disposition without a structural-identifiability provider.
