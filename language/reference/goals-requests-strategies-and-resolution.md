# Chapter 9: Goals, Requests, Strategies and Resolution

## Goal structure

The `goals:` section names the work the compiler must perform. It is
optional: when a declaration omits it, every definition is an
`evaluate` goal producing `rust.library` (the compiler asks for the
whole surface). Declaring `goals:` selects just the work you want:

```emath
goals:
    evaluate <score>:
        produce rust.library
```

Advanced goals carry policy; a goal names the work and policy, not a
hard-coded algorithm:

```emath
goals:
    differentiate <score>:
        wrt [state.alpha, state.gamma]
        order 1
        require evidence >= E2
        produce rust.library
```

## Core goals

```text
evaluate, transform, simplify, differentiate
integrate, solve, optimize, simulate
search, synthesize, prove, verify
compile, benchmark
```

## Optimization methods

The admitted optimization surface today is `minimize(expr) wrt var` /
`maximize(expr) wrt var` (one variable per goal; coordinate blocks):
pure Newton on `∇f = 0` with a dual-number gradient and a
finite-difference Hessian, plus the quadratic exterior penalty for
`constraints:` sections. A returned point is a STATIONARY point of the
declared kind (a min claimed as a max returns a labeled `fault`);
exhausting iterations, a vanished Hessian, or the wrong curvature
yields a labeled `fault` rather than a non-optimum. The inequality in
the penalty approach is
approached, not a hard feasible-set projection; labeled in
`constrained-opt.emath`.

The `core::optimization` methods library
([`std-optimization-methods`](../stdlib/cells/std-optimization-methods.md))
provides Newton with Armijo backtracking line search,
BFGS (quasi-Newton; no Hessian required), the `kkt_residual`
certificate helper, and typed fault labels (`SingularHessian`,
`BudgetExhausted` carrying the achieved `‖∇f‖`, `LineSearchStalled`).
Interior-point and SQP diagnose by name. Global, Bayesian, manifold,
bilevel, SDP, and SOCP optimization are not claimed.

### Program-carrier optimize and sample-limit capabilities

The scalar optimization and limit-sampling laws are also carried as pure
`Value::Program` capability kernels, for authored programs embedded
through the program literal and evaluated over an explicit Float64
environment. The capsules live in
[`language/spec/capabilities/program_optimize.emath`](../spec/capabilities/program_optimize.emath):

- `std.capability.program.optimize` (kernel `program-optimize`):
  Newton on ∇f = 0 over the program. Arguments are the program, the
  complete environment vector, the variable-index vector (exact
  non-negative integer entries), the maximize flag, the stationarity
  tolerance, and the iteration budget; a learning-rate argument is
  admitted by the ABI and unused, because Newton takes no fixed step.
  The result is a stationary point of the claimed kind or a typed
  refusal: no variable, a vanished Hessian, the wrong curvature for
  `maximize`/`minimize`, or an exhausted budget. The kernel gradient is
  a central finite difference (the dual gradient is unreachable at the
  kernel ABI); the Hessian is the forward difference of that gradient.
- `std.capability.program.sample-limit` (kernel `program-sample-limit`):
  samples the program along a geometric step schedule 1e-1 through
  1e-12 approaching the target, two-sided by default or one-sided with
  direction `1` (above) / `-1` (below). The first finite pair agreeing
  within 1 percent returns; otherwise the last finite sample returns;
  a schedule with no finite sample refuses.

Both kernels keep the expression-form law detail strings verbatim
(empty-vars, hessian-vanished, wrong-curvature, nonconvergence,
no-finite-values), so a refused claim is never a silently returned
non-optimum. The normative law text remains the interpreter reference
implementation (`interp/ops/eval_flow.rs`).

## Native symbolic computation

`simplify <target>` computes during planning for exact integer scalar
expressions. It recursively folds exact integer constants and applies neutral
element rewrites such as `x + 0`, `x - 0`, and `x * 1`; the resulting goal
expression is carried by a `native-symbolic` plan. Rewrite rules use
`RewritePattern` captures and are labeled `structural-checked`, never `proved`
without a certificate.

The native algebraic decision procedure compares univariate polynomial
coefficient vectors exactly for `+`, `-`, `*`, and non-negative integer powers,
up to degree 64. Overflow/resource failures are `E-SYM-002`; expressions
outside that fragment, including transcendental and general first-order
claims, are `E-SYM-003`. Gröbner bases, ideal membership, CAD, and arbitrary
quantified claims are unsupported without a compatible provider and checker.

## Fit goals (04 §5.3)

`fit <params> to <observable>:` is a generic estimation goal; never
bare optimization. The whole fit program is plain payload data, all of
it declared in the goal suite:

```emath
goals:
    fit k_el, V_central to conc_time:
        model PK_TwoCompartment
        prediction central
        residual: weighted_least_squares
        weights: k_el = 1.0
        weights: V_central = 1.0
        method levenberg_marquardt
        initial: k_el = 0.2
        initial: V_central = 1.0
        data: t = [0.5, 1.0, 2.0, 4.0]
        data: conc_time = [3.12, 2.43, 1.47, 0.54]
        require identifiability.structural
```

- `model <path>`; path to the model carrying the prediction;
- `prediction <label>`; prediction target;
- `residual: <method>`; explicit residual method
  (`weighted_least_squares` today); weights are never silent;
- `weights: <param> = <number>`; explicit per-parameter weights
  (strictly positive; unknown parameter names diagnose);
- `method <optimizer>`; `levenberg_marquardt` today;
- `initial: <param> = <number>`; seed values; part of provenance
  (model + data + seed + method);
- `data: <entry> = [<number>, ...]`; observed data rows, exactly two:
  one names the observable (the `y` values), one names the model's
  arity mismatch, empty rows, and unparseable literals are typed
  diagnoses;
- `require identifiability.structural`; the honesty fence: without it
  the fit is numeric only and claims no authority.

Execution is generic capability/method/provider plumbing
(`crates/emath-calibration`): residuals through a residual-method +
model seam (model faults surface as `ModelError`, never NaN-poisoned
optimization), optimization through an optimizer-method seam, and
structural identifiability through a provider seam; the executable
`NumericRankOracle` evaluates the residual Jacobian's local column rank
at the fitted point and derives covariance-based confidence intervals
(tight only when the data certifies the direction's sign). No domain
model is compiled into the Rust nucleus; the PK one-compartment model
is the runnable fixture `language/examples/science/
pk-two-compartment-fit.emath`.

`emath fit <file.emath>` (with `--json` for the deterministic envelope)
parses, admits, plans, executes the declared fit to fitted values with
linked `Fitted` provenance: every fitted parameter materializes as a
`Measured` value whose `fit_id` is the content-addressed fit hash
(model + vocabulary + data + seed + method). Where no registry
structural-identifiability provider exists, fit plans still exclude
`fit.structural-identifiability` with a typed reason. A granted fit
carries per-direction confidence intervals; a missing provider stays
honestly unresolved; a relaxed or zero-straddling direction returns
`AuthorityEscalation` naming it (fitting is estimation with uncertainty,
provenance, and identifiability; never a silent optimum).

## Goal contract

Each goal includes:

- semantic target;
- bound inputs/unknowns;
- desired outputs;
- exactness/error tolerance;
- evidence requirement;
- budget;
- target/deployment;
- determinism;
- provider allow/deny/preference;
- fallback and unresolved policy.

## Strategies

```emath
strategies:
    prefer [native, symbolic, interval]
    allow providers matching NumericSolver
    deny remote
    fallback parametric
```

Strategy statements constrain planner policy. They cannot weaken a goal's correctness/evidence requirements implicitly.

## Resolution outcomes

- selected native/provider composition;
- multiple Pareto plans retained for benchmarking;
- parametric provider requirement;
- exploration/search artifact;
- continuation;
- diagnostic unresolved artifact.

## Plan introspection

The CLI exposes eligible providers, excluded providers and reasons, decomposed subgoals, budgets, checks, estimated cost and fallback graph before execution.

Every selected plan also names the deterministic `goal:solver:provider` combination (Track A3, emath-9bj1): the goal kind's stable spelling, the solver the goal binds (`solve` → `newton-bracket` with the bisection fallback, `differentiate` → `dual-forward`, `optimize` → `newton-hessian`, `integrate` → `quadrature`, a fit goal's declared optimizer method, `interpreter` otherwise), and the retained provider id. The combination is part of plan output (`emath plan` inspection, `explain`, and the JSON envelope); `None`/empty exactly when no plan was selected.

## Custom goals

A package may define a custom goal schema. It must either lower to core goals or provide a versioned provider/checker ecosystem. Artifact consumers preserve unknown custom goals but cannot claim them satisfied without compatible checkers.
