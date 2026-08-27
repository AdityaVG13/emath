# Chapter 7: Expressions, Equations, State and Events

## Expressions

Expressions support literals, variables, calls, records, tuples, collections, conditionals, pattern matching, binders and scoped notation.

Mathematical binders include:

```emath
sum i in 0..N: x[i]
sum i in 0..N if i > 0: x[i]    # B02: filtered fold
sum i in 0..N, j in 0..M: A[i,j] # multi-binder
integral x in Ω: f(x)
forall x in domain: property(x)
exists witness in candidates: valid(witness)
derivative f wrt x
jacobian f wrt x
hessian f wrt x
partial(T) wrt x holding p
total(t) wrt time
```

A binder produces semantic structure; it is not immediately expanded into loops.

The optional `if <condition>` guard clause (B02) filters the fold: only
iterations where the condition is true contribute. An always-false guard
produces the identity element (0 for `sum`, 1 for `product`, `true` for
`forall`, `false` for `exists`). Multi-binder guards cover all variables
in the binder list: `sum i in 0..n, j in 0..m if i + j < k: f(i, j)`.

An empty range is the same identity (`sum i in 0..0: f(i)` is 0,
`product i in 0..0: f(i)` is 1, `forall` over empty is true, `exists`
over empty is false) and does not evaluate the body. The bound name is
visible only in the body and optional `if` guard: it shadows an outer
input, definition, or binder of the same spelling, and it does not leak
after the binder. `sum i in 0..n: sum i in 0..m: i` is `n` copies of
the inner sum, not a replay of the outer index.

## Logic connectives

Boolean operators and logic connectives:

```emath
a and b       # conjunction
a or b        # disjunction
not a         # negation
a ==> b       # implication (right-associative: a ==> (b ==> c))
a <==> b      # biconditional
```

`==>` and `<==>` are B12 logic connectives. They use distinct tokens
from `=>` (match/lambda/notation arrow) and `<=>` (chemistry equilibrium
arrow) per C5. Precedence (lowest to highest): `<==>` < `==>` < `or` <
`and` < comparisons < arithmetic. Both produce `Bool` results and
require `Bool` operands.

## Limits, series, and asymptotic equivalence

### Limit as a claim (B04)

`limit` is a contextual keyword that produces a **claim**, not a
computation. It states that a function approaches a value as the
variable tends to a target:

```emath
limit x -> 0: sin(x) / x          # two-sided limit
limit x -> 0+: 1 / x              # one-sided from above
limit x -> 0-: 1 / x              # one-sided from below
limit n -> inf: (1 + 1/n) ^ n     # limit at infinity
```

The `+`/`-` suffix before `:` selects the direction. Without a suffix,
the limit is two-sided. The target is parsed at multiplicative
precedence, so complex targets need parentheses: `limit x -> (a + b): f(x)`.

`limit` expressions are usable in `require`, `ensure`, and `invariant`
sections. They are not computable - use `sample_limit` for numerical
evaluation.

### sample_limit as a computation (B04)

`sample_limit` is a contextual keyword that produces a **computation**.
It numerically approximates the limit by sampling the body at points
approaching the target:

```emath
definitions:
    l = sample_limit x -> 0: sin(x) / x    # returns ~1.0
```

`sample_limit` returns `Float64`. It is admitted in `definitions:` and
`equations:` sections.

### Series (B06)

`series` is a contextual keyword that produces a **claim** about
convergence. It follows the binder syntax:

```emath
series n in 0..inf: a[n]           # convergence claim
series k in 0..10: 1 / (k + 1)     # finite series (admitted as binder)
```

`series` is not a computation. It is usable in `require` and
`invariant` sections to state convergence properties.

### Asymptotic equivalence (B18)

`~~` is a binary operator for asymptotic equivalence. It states that
two functions are asymptotically equal (their ratio tends to 1):

```emath
factorial(n) ~~ sqrt(2 * pi * n) * (n / e) ^ n    # Stirling's approximation
f(n) ~~ g(n)                                       # general form
```

`~~` has the same precedence as comparison operators. It is a **claim**,
not a computation - it lowers to a limit claim (`limit x -> inf: f(x)/g(x) == 1`).
Per C7, `~` is reserved for the distribution tag; asymptotic equivalence
uses `~~`.

`limit`, `sample_limit`, and `series` are contextual keywords: they
activate only in their syntactic positions (followed by an identifier
and `->` for limits, or `in` for series). In all other positions they
are regular user identifiers.

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
add sub mul div neg
min max abs floor ceil round sign is_finite
sqrt exp ln log2 log10 sin cos tan tanh sinh cosh atan atan2
cbrt recip fract hypot mod lerp clamp pow
and or not ==> <==>
mean norm length dot transpose einsum
laplacian laplacian_neumann laplacian_dirichlet laplacian_2d laplacian_2d_neumann
laplacian_3d laplacian_3d_neumann
gradient gradient_2d_x gradient_2d_y gradient_3d_x gradient_3d_y gradient_3d_z
div_1d div_2d div div_3d
if cond: a else: b
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
partial(y) wrt x holding p    # same autodiff; holding required
x^2                  # integer exponents admitted as dimensionless numeric
grad(f)                # reverse-mode AD: gradient w.r.t. all inputs
cases x: | x > 0 => 1 | else => 0  # piecewise conditional
solve(residual) wrt x    # Newton's method root-finding
minimize(loss) wrt x     # Newton on ∇f = 0 (stationarity)
minimize(loss) wrt x, y  # multi-variable Newton on ∇f = 0
maximize(score) wrt x    # Newton on ∇f = 0; Hessian must be a max
sample_limit x -> 0: sin(x) / x    # numerical limit approximation
```

Expressions that parse but do not compute (claims - usable in
`require`/`invariant`):
```text
limit x -> 0: f(x)          # B04: limit claim (two-sided)
limit x -> 0+: f(x)         # B04: one-sided from above
limit x -> 0-: f(x)         # B04: one-sided from below
series n in 0..inf: a[n]    # B06: series convergence claim
f(n) ~~ g(n)               # B18: asymptotic equivalence
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

### Reverse-mode autodiff (gradient)

`grad(expr)` computes the gradient of a scalar expression with respect
to all declaration inputs in a single backward (adjoint) pass. It
returns a `Vector[N]` where N is the number of inputs, containing
`[df/dx1, df/dx2, ..., df/dxN]`.

```emath
emath function grad_example(x: Float64, y: Float64) -> Vector[2]:
    definitions:
        f = x * y + y * y
        g = grad(f)    # [df/dx, df/dy] = [y, x + 2*y]
```

Reverse-mode is efficient for scalar functions of many inputs: the cost
is O(function_cost) regardless of input count, versus O(N * function_cost)
for forward-mode (`derivative` with N separate passes). The interpreter
uses a Wengert tape: a forward pass records all primal values, then a
backward pass propagates adjoints from the output to all inputs in one
traversal. Codegen emits inline Rust with the same primal/adjoint
structure.

`grad` is admitted in `definitions:` and `equations:`. The expression
argument must be scalar numeric; `grad` on a vector or non-numeric
expression is refused with `E-TYPE-012`.

### cases expression (U1)

`cases` is a piecewise conditional expression with mandatory `else`
(totality enforced at parse time). It lowers to nested conditional
expressions.

```emath
emath function signum(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0.0 => 1.0
            | x < 0.0 => -1.0
            | else => 0.0
```

Syntax:
- `cases [subject]:` introduces the expression. The subject is
  optional (for readability; arm conditions are full expressions,
  not pattern matches).
- Arms are delimited by `|` and use `=>` as the arm arrow.
- A mandatory `else` arm enforces totality. Missing `else` is a
  parse error (`E-SYN-110`).
- At least one condition arm is required before `else`.

`cases` is a contextual keyword: it activates only when followed by
`:` or by an identifier and then `:`. In all other positions it is a
regular user identifier.

Lowering: `cases x: | c1 => e1 | c2 => e2 | else => e3` lowers to
`if c1: e1 else: if c2: e2 else: e3` (nested `ExprNode::If`). All arm
values must have the same type (`E-TYPE-012` on mismatch). Arm
conditions must be Boolean (`E-TYPE-012` on non-Boolean).

### Partial and total derivatives (04 section 2.2)

Three derivative operators are distinguished by kind:

- `derivative(expr) wrt x` - unqualified derivative (existing, computes via autodiff).
- `partial(expr) wrt x holding p` - partial derivative. The `holding`
  set (variables held constant) is part of the term's identity:
  `partial(H) wrt T holding p` and `partial(H) wrt T holding V` are
  different terms.
- `total(expr) wrt t` or `d(expr) wrt t` - total/material derivative
  (distinct operator, distinct glyph).

`partial` and `total` are contextual keywords: they activate only when
followed by `(` in expression position. In all other positions they are
regular identifiers, so `partial + 1` and `d = 5` still work.

The Unicode partial derivative symbol `∂` (U+2202) is accepted as an
alias for `partial`: `∂(T) wrt x holding p` is the same as
`partial(T) wrt x holding p`.

A partial derivative without an explicit `holding` set is a MeaningHole
refusal - the compiler will not guess which variables are held fixed.
This prevents the most error-prone notation in physics from being
silently ambiguous.

`solve(residual) wrt var` finds the value of input `var` that drives
`residual` to zero, using Newton's method (`x -= f(x)/f'(x)`). Each
step uses the same dual-number evaluation for both the residual value
and its derivative. A vanished derivative or exhausting the iteration
budget without `|f| < tolerance` is a typed refusal, not a silent
non-root. `minimize(objective) wrt var` and `maximize(objective)
wrt var` find a stationary point of `objective` (`∇f ≈ 0`) by Newton's
method on the gradient (`x -= H^{-1} ∇f`), with `H` from a
forward-difference of the dual-number gradient. The initial guess is
the input value supplied at runtime. A vanished Hessian, a stationary
point of the wrong curvature (a min when `maximize` was asked), or
exhausting iterations without `|∇f| < tolerance` is a typed refusal.

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
evaluated.

## Fully implicit DAEs: `algebraic:` unknowns and residuals

For coupled systems that cannot be written as explicit rates or
algebraic definitions, declare the implicit unknowns with an
`algebraic:` section and write the balance laws as residuals:

```emath
emath model CausalizedRC:
    inputs:
        V: Float64
        R: Float64
        C: Float64

    algebraic:
        I: Float64

    state:
        q: Float64

    equations:
        V - R * I - q / C == 0   # residual: implicit constraint on I
        der(q) = I               # rate, solved together with the residual
```

A residual is any `lhs == rhs` comparison or bare `expr` (meaning
`expr == 0`) in `equations:`. `==` is always a residual - even a bare
`a == 5` constrains `a` instead of defining it. Residuals are stored
as implicit constraint records keyed by the model declaration.
Automatic causalization rewrites `der(state)` inside a residual to a
synthetic rate placeholder, so `M * der(v) == f` (non-scalar mass)
solves for the rate `der(v)` together with the algebraic unknowns. At
each time step the runner Newton-solves the coupled residual system
with a finite-difference Jacobian and Gaussian elimination; definitions
are re-evaluated with the solved algebraic values, and the solved rates
feed the integrator. So a fully implicit DAE needs no manual `solve`
op - see `language/examples/intro/causalized-rc.emath`.

**Capability status (Phase 1):** fully implicit DAEs run both ways. `emath
simulate` solves the residual system with the runner's causalized Newton;
`rust.library` codegen embeds the same Newton solve (forward-difference
Jacobian, Gaussian elimination, 30 iterations, 1e-9 tolerance) into the
generated `step_euler` / `step_rk4`, which return `Result<Self, String>`
and refuse on non-convergence instead of inventing a value. Algebraic
unknowns are part of the returned DAE state: after the differential
update, both the interpreter and the generated steps re-solve them at
the accepted point (index-1 projection) so the algebraic residual is ~0
— a successful step does not leave `g(q_new, I_old) = O(dt)`. Parity
between the generated steps and `emath simulate` is pinned by the
examples lane (`language/examples/intro/causalized-rc.emath` builds a
crate whose steps match the interpreter on the same inputs). Semi-explicit
DAEs (algebraic definitions plus one rate in `equations:`) build fine
without a Newton solve; see `language/examples/intro/algebraic-dae.emath`.
`emath simulate <file> --set <guess>...` takes the `algebraic:` initial
guesses alongside inputs and state.

Conformance checks at admission time:

- The residual system must be square: the number of residuals must
  equal the number of declared algebraic unknowns plus the distinct
  `der(state)` terms appearing in residuals (`E-TYPE-010` otherwise).
- Every `algebraic:` unknown must appear in some residual (`E-TYPE-002`
  otherwise); an `algebraic:` section with no residuals is `E-TYPE-010`.
- `algebraic:` sections are `AtMostOne` in `emath model` declarations.

Boundary of the admitted spelling: the `derivative(...)` keyword
greedily consumes its argument, so a scalar implicit rate such as
`0 = m * derivative(v) + v` parses as `m * derivative(v + v)` and is
refused with guidance. The non-scalar mass form `M * der(v) == f` is
the admitted spelling for implicit rates.

`emath simulate` integrates those rates with Euler, classic 4-stage RK4,
or Cash-Karp RK45. Default is fixed-step RK4 (not a predictor-corrector
and not symplectic). `--atol` / `--rtol` turn on adaptive RK45.
`--set name=value` binds inputs, algebraic guesses, and state: scalars,
vectors (`--set s=[1,0]`), or matrices (`--set u=[[1,0],[0,1]]`).
`--event name=value` stops at one scalar crossing. That is not a
general event language.

`constraints:` sections in function declarations feed into the
optimization engine. Each constraint is a Bool expression (typically
a comparison like `x + y >= 1`). When `minimize` or `maximize` is
used in a definition, the compiler automatically adds penalty terms
to the objective for each constraint. Inequality constraints (`>=`,
`<=`) use `w * max(0, violation)^2` penalties; equality constraints
(`==`) use `w * violation^2`. Weight is 1000. This is a soft exterior
penalty: the returned point is stationary for the penalized objective,
which sits slightly on the infeasible side of an inequality (for
`min x^2+y^2` s.t. `x+y>=1`, `x+y ≈ 0.9995`). It is not a projection
onto the feasible set.

Not admitted yet: full jacobian and
hessian, `transitions:` / `events:`, discrete hybrid models.

`einsum("ik,kj->ij", A, B)` is admitted and evaluates: the subscript
string defines the Einstein summation contraction (input indices,
arrow, output indices). Identities the interpreter keeps:

- `einsum("ik,kj->ij", A, B)` equals matrix product `A * B`.
- `einsum("i,i->", u, v)` equals `dot(u, v)`.
- Implicit mode (no `->`) emits the unique free indices in
  alphabetical order: `einsum("ik,kj", A, B)` is `A * B`, and
  `einsum("ji", A)` is `transpose(A)`.
- Repeated output labels write the diagonal: `einsum("i->ii", v)` is
  `diag(v)`, off-diagonal zeros.
- Size-1 axes broadcast; genuine extent mismatches are a typed fault.
- Spaces in the subscript string are ignored (`"i j -> j i"` transposes).

The interp and rust.library share the `emath-rt` kernel: Vector, Matrix,
and Tensor operands (generated Rust treats `Vec<f64>` as rank-1,
`Vec<Vec<f64>>` as rank-2, and `emath_rt::Tensor { shape, data }` as
rank-3+). Extent mismatches are a typed fault
(`EvalFault` in interp, `EinsumError` from `einsum_checked`). Block
matrices (`[A | b]`) are not yet admitted.

Index and slice (`v[i]`, `m[i, j]`, `t[0, :, :]`) use the same checked
kernels. `:` is a half-open range; point axes drop rank and range axes
keep it, so `t[0, :, :]` is the first 2×2 face of a rank-3 tensor (a
`Matrix`). Out-of-bounds, negative, and non-whole indices are a typed
fault — interp `EvalFault::IndexOutOfBounds`, rust.library
`Result<T, String>` — never panicking `[]`. A negative constant is
refused at admission (`E-SHAPE-006`).

### Unit and dimension queries (04 section 1.4)

`unit of E` and `dimension of E` are compile-time query operators
with precedence just above `==`. They are usable in `require`,
`tests:`, and `expect`:

```emath
require dimension of thrust == Force
expect unit of (m * c^2) == kg*m^2/s^2
```

`unit` and `dimension` are contextual keywords: they activate only
when followed by `of`. In all other positions they are regular
identifiers. These queries parse today; compile-time evaluation
requires a unit inference engine (not yet implemented in Phase 1).

Spatial operators (admitted in `definitions:` and `equations:`):

1-D Laplacian - second derivative via the stencil `[1, -2, 1] / dx²`:
- `laplacian(u, dx)` - clamped (face-insulated) edges. Conserves the
  unweighted sum `Σ u_i` (finite-volume flux zero at the face).
- `laplacian_neumann(u, dx)` - even reflection through the boundary
  *cell* (`u[-1] = u[1]`). This is a second-order `du/dn = 0` at the
  cell center. It conserves the trapezoidal inner product
  `(u_0 + u_{n-1})/2 + Σ_{i=1}^{n-2} u_i`, not the unweighted sum, when
  heat sits on the boundary.
- `laplacian_dirichlet(u, dx, g_left, g_right)` - fixed boundary values.

2-D Laplacian - 5-point stencil `[[0,1,0],[1,-4,1],[0,1,0]] / dx²` over a
`Matrix`:
- `laplacian_2d(u, dx)` - clamped edges.
- `laplacian_2d_neumann(u, dx)` - mirror edges.

1-D gradient - first derivative via central differences
`[-1/(2dx), 0, +1/(2dx)]`:
- `gradient(u, dx)` - one-sided edges (linear ghost; a slope-1 ramp is
  1 at the boundary, not the clamp-central artifact 0.5); returns a `Vector`.

2-D gradient - first derivative along one axis (central differences):
- `gradient_2d_x(u, dx)` - `du/dc` (along columns); returns a `Matrix`.
- `gradient_2d_y(u, dx)` - `du/dr` (along rows); returns a `Matrix`.

3-D operators over rank-3 `Tensor` values:
- `laplacian_3d(u, spacing)` uses the 7-point stencil with equal spacing.
- `laplacian_3d(u, dx, dy, dz)` uses independent axis coefficients
  `1/dx²`, `1/dy²`, and `1/dz²`.
- `laplacian_3d_neumann` accepts the same signatures with mirrored faces.
- `gradient_3d_x/y/z(u, spacing)` returns the selected partial derivative.

Divergence is available in each admitted spatial dimension:
- `div_1d(vx, dx)`
- `div_2d(vx, vy, spacing)` or `div_2d(vx, vy, dx, dy)`
- `div(vx, vy, vz, spacing)` or `div(vx, vy, vz, dx, dy, dz)`;
  `div_3d` is an explicit alias.

Spatial spacing values must be positive literal constants. The 3-D forms
support non-cubic physical grids through distinct `dx`, `dy`, and `dz`.

Heat equation as a continuous model: an `emath model` with a `Vector`
(1-D), `Matrix` (2-D), or rank-3 `Tensor` state and
`der(u) = alpha * laplacian[_2d|_3d](u, 1.0)` admits and integrates under
`emath simulate` (RK4). Clamp edges conserve `sum(u)`. Mirror (Neumann)
edges conserve trapezoidal heat, not always `sum(u)`. See
`heat-rod.emath`, `heat-rod-sim.emath`, `heat-plate.emath`,
`heat-plate-sim.emath`, `heat-volume-sim.emath`, and `gradient-field.emath`.

Not admitted yet: continuous `Field[R^d -> R]` types and Dirichlet
boundaries for 2-D/3-D Laplacians. `heat-pde.emath` remains a target sketch
for the full field-type design.

### Modular arithmetic and finite fields

Modular arithmetic builtins operate on integer (i64) values and are
admitted in `definitions:` and `equations:`:

- `factorial(n)` - exact i64 factorial. `n` must be in [0, 20] (i64
  overflow guard). Returns `Int`.
- `mod_inv(a, m)` - modular inverse of `a` modulo `m` via extended GCD.
  Errors at runtime if `gcd(a, m) != 1`. Returns `Int`.
- `congruence(a, b, m)` - congruence test: `(a - b) mod m == 0`. Returns `Bool`.
- `mod(a, m)` - floating-point remainder (already available as a general
  builtin; works on `Int` values too via i64-to-f64 coercion).
- `poly_eval_mod(coeffs, x, p)` - evaluates polynomial `c[0] + c[1]*x +
  ... + c[k-1]*x^(k-1)` at `x` modulo `p` using Horner's method. `coeffs`
  is a `Vector`, `x` and `p` are integers. Returns `Int`.
- `rs_encode(coeffs, n, p)` - constructs a Reed-Solomon codeword by
  evaluating the polynomial at points `0, 1, ..., n-1` over `GF(p)`.
  Returns a `Vector` of `n` values.

`GF<p>` and `GF<p>` are admitted as `Int` types - values are exact
integers, and modular reduction is performed by the builtins, not the
type system. This is sufficient for Reed–Solomon code construction
over small prime fields (evaluating polynomials, checking distances,
testing Wilson's theorem).

```emath
definitions:
  p = 7
  wilson = mod(factorial(p - 1), p)
  expect wilson == p - 1  (* (p-1)! ≡ -1 (mod p) for prime p *)
```

### Limits, series, and asymptotic equivalence (B04+B06+B18)

- `limit x -> 0: f(x)` - limit as a **claim** (parses, does not compute).
  One-sided: `0+` (from above), `0-` (from below). Usable in `require`/
  `invariant`.
- `sample_limit x -> 0: f(x)` - numerical limit approximation
  (**computation**). Admitted in `definitions:`/`equations:`.
- `series n in 0..inf: a[n]` - series convergence **claim** (parses,
  does not compute). Contextual keyword.
- `f(n) ~~ g(n)` - asymptotic equivalence (**claim**). Lowers to a
  limit claim. Per C7, `~` is the distribution tag; `~~` is asymptotics.

`limit`, `sample_limit`, and `series` are contextual keywords: they
activate only in their syntactic positions and remain valid user
identifiers elsewhere.
