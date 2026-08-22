# Chapter 7: Expressions, Equations, State and Events

## Expressions

Expressions support literals, variables, calls, records, tuples, collections, conditionals, pattern matching, binders and scoped notation.

Mathematical binders include:

```emath
sum i in 0..N: x[i]
integral x in Ω: f(x)
forall x in domain: property(x)
exists witness in candidates: valid(witness)
derivative f wrt x
jacobian f wrt x
hessian f wrt x
```

A binder produces semantic structure; it is not immediately expanded into loops.

## Definitions

A definition is directed and names a value/function:

```emath
definitions:
    energy = 0.5 * mass * velocity^2
```

Recursive definitions require explicit recursion/termination policy.

## Equations and relations

```emath
equations:
    derivative(position) == velocity
    mass * derivative(velocity) == force
```

Equations retain equality/relational meaning. Solver planning may causalize or discretize them with a trace.

## Constraints and objectives

```emath
constraints:
    capacity <= limit

objectives:
    minimize cost
    maximize quality
```

Multiple objectives declare lexicographic, weighted or Pareto semantics.

## State

State fields have ownership, initialization, mutability and clock semantics. State is not inferred merely because a Rust variable is mutable.

## Transitions and events

```emath
transitions:
    on observe(value):
        state.count += 1

events:
    event ThresholdCrossed(value: Real)
```

Events define ordering, clock/domain and delivery policy. Continuous zero crossings and discrete events are distinct.

## Effects

Effects include state mutation, randomness, IO, provider call, allocation, network and nondeterministic scheduling. Pure expressions cannot invoke effects through hidden provider behavior.

## Implemented today

Expressions that run:

```text
literals, names, + - * / ^, comparisons
min max abs floor ceil round sign is_finite
sqrt exp ln log2 log10 sin cos tan tanh sinh cosh atan atan2
cbrt recip fract hypot mod lerp clamp pow
mean norm length dot transpose
if cond then a else b
vector / matrix / tensor literals
index and slice  v[i]  m[i, j]  t[0, :, :]
sum i in 1..6: i
product i in 1..=5: i
sum i in 0..n: v[i]    (variable bound, runtime fold)
forall i in 0..n: v[i] > 0
exists i in 0..n: v[i] == 0
integral x in a..b: x * x
sum([1, 2, 3, 4, 5])
product([[1, 2], [3, 4]])
mean(v)          abs(v)
derivative(x)    der(x)    derivative(x) wrt time
derivative(y) wrt x    # forward-mode autodiff in definitions
solve(residual) wrt x    # Newton's method root-finding
minimize(loss) wrt x     # gradient descent optimization
minimize(loss) wrt x, y  # multi-variable gradient descent
maximize(score) wrt x    # gradient ascent optimization
```

`sum` / `product` run when the range is a known integer interval
(`1..6` is 1+2+3+4+5; inclusive `1..=5` keeps the upper bound) or when
the argument is a vector, matrix, or tensor with a known size. `mean(v)`
is `sum(v) / length(v)`, and `abs(v)` maps elementwise over a known-size
vector. A variable-bound range such as `0..n` lowers to a runtime fold
(EMIR `Fold` op), so `sum i in 0..n: v[i]` computes when `n` is a
runtime value like `length(v)`. `forall` and `exists` use the same
`Fold` op with `And` / `Or` combine, producing a `Bool` result.
`integral x in a..b: f(x)` lowers to a dedicated `Integral` op that
evaluates the integrand at 1001 sample points using composite Simpson's
rule (exact for polynomials of degree 3 or less).
`derivative(expr) wrt var` in a definition computes the exact derivative
of `expr` with respect to input variable `var` using dual-number
forward-mode autodiff. The value expression is inlined (definition
references resolved) and lowered to a nested EMIR sub-program; each EMIR
op carries its own derivative rule, so the tangent propagates through
the full computation chain.
`solve(residual) wrt var` finds the value of input `var` that drives
`residual` to zero, using Newton's method (`x -= f(x)/f'(x)`). Each
step uses the same dual-number evaluation for both the residual value
and its derivative. `minimize(objective) wrt var` and `maximize(objective)
wrt var` find the input value that minimizes or maximizes `objective`
using gradient descent (`x -= lr * f'(x)`) or ascent (`x += lr * f'(x)`).
The initial guess is the input value supplied at runtime.

Definitions are directed: `name = expr`. Later definitions may use
earlier ones, in source order.

Model equations that run:

```emath
der(x) = v
der(v) = (-c * v - k * x) / m
m * der(v) = -c * v - k * x    # only when m is a named scalar
I = (V - q / C) / R            # algebraic definition (semi-explicit DAE)
der(q) = I                      # rate referencing the algebraic variable
```

The third form is rewritten and recorded as `der(v) = rhs / m`. The
fourth form is an algebraic definition: `name = expr` in `equations:`
is evaluated at each time step in source order, so rate equations can
reference it. This enables semi-explicit DAE models where algebraic
variables are computed from state and inputs before the rates are
evaluated. Any other leftover equation (e.g. `0 = expr` implicit
residuals) is `E-TYPE-010`.

`emath simulate` integrates those rates with Euler, RK4, or RK45.
Default is a fixed step. `--atol` / `--rtol` turn on adaptive RK45.
`--event name=value` stops at one scalar crossing. That is not a
general event language.

`constraints:` sections in function declarations feed into the
optimization engine. Each constraint is a Bool expression (typically
a comparison like `x + y >= 1`). When `minimize` or `maximize` is
used in a definition, the compiler automatically adds penalty terms
to the objective for each constraint. Inequality constraints (`>=`,
`<=`) use `max(0, violation)^2` penalties; equality constraints
(`==`) use `violation^2` penalties.

Not admitted yet: full jacobian and
hessian, `transitions:` / `events:`, discrete hybrid models,
PDEs, `einsum`.
