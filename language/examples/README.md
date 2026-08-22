# Language Examples

Cross-domain example programs illustrating emath semantics, grouped by
category. Within each category, read in table order.

A file in this tree is not automatically executable. The tables below
say what the compiler does with each one today.

## intro — getting started

| Example | Today |
|---------|-------|
| [hello-square.emath](intro/hello-square.emath) | Runs. Smallest `emath function`. |
| [sum-one-to-five.emath](intro/sum-one-to-five.emath) | Runs. Finite `sum i in 1..6: i` and `sum([1, 2, 3, 4, 5])`. |
| [tensor-face.emath](intro/tensor-face.emath) | Runs. Rank-3 tensor, a `:` slice, and a matrix `expect`. |
| [stateful-affine-scorer.emath](intro/stateful-affine-scorer.emath) | Runs. Stateful `emath policy` with a constructor. |
| [vector-given.emath](intro/vector-given.emath) | Runs. A `Vector[3]` input bound by `given v = [1, 2, 3]`; index, scale, and `dot` it. |
| [vec-stats.emath](intro/vec-stats.emath) | Runs. `mean(v)` and elementwise `abs(v)` on a known-size vector. |
| [factorial.emath](intro/factorial.emath) | Runs. Inclusive `product i in 1..=5: i` fold. |
| [range-sum.emath](intro/range-sum.emath) | Runs. Variable-bound `sum i in 0..n: v[i]` with `n = length(v)`, a runtime fold (not compile-time unrolling). |
| [forall-exists.emath](intro/forall-exists.emath) | Runs. `forall i in 0..n: v[i] > 0` and `exists i in 0..n: v[i] == 0`, quantifier binders over a vector. |
| [integral.emath](intro/integral.emath) | Runs. `integral x in a..b: x * x` with composite Simpson's rule (1000 steps, exact for degree ≤ 3). |
| [autodiff.emath](intro/autodiff.emath) | Runs. `derivative(y) wrt x` in a definition, forward-mode autodiff via dual numbers. At x=3, dy/dx of x^2 = 6. |
| [solve.emath](intro/solve.emath) | Runs. `solve(x^2-4) wrt x` with Newton's method. From x=1, converges to root x=2. |
| [optimize.emath](intro/optimize.emath) | Runs. `minimize((x-3)^2) wrt x` and `maximize(-(x-2)^2) wrt x` with gradient descent/ascent. |
| [algebraic-dae.emath](intro/algebraic-dae.emath) | Runs. Semi-explicit DAE: algebraic variable `I` in `equations:`, `der(q) = I` references it. RC circuit with `emath simulate`. |
| [implicit-dae.emath](intro/implicit-dae.emath) | Runs. Implicit DAE: `solve(V - R*I - q/C) wrt I` in equations. Newton's method finds current at each step. |
| [showcase.emath](intro/showcase.emath) | Runs. `solve` + `minimize` + `derivative` in one function: finds root of x^2-4, minimizes (x-3)^2+1, computes derivative 2x. |
| [constrained-opt.emath](intro/constrained-opt.emath) | Runs. Constrained optimization: minimize x^2+y^2 subject to x+y=1. Substitutes y=1-x to eliminate the constraint, then minimizes. |

## numerical — numerics and dynamics

| Example | Today |
|---------|-------|
| [explicit-mass-spring.emath](numerical/explicit-mass-spring.emath) | Runs. `emath model` you can `emath simulate`. The `m * der(v) = rhs` spelling also admits when `m` is a named scalar. |
| [dynamic-mass-spring.emath](numerical/dynamic-mass-spring.emath) | Target sketch for a later DAE / rumoca path. Not an admitted `emath model`. |
| [tensor-program.emath](numerical/tensor-program.emath) | Target sketch (`einsum`, binders, autodiff). Rank-3 tensors and slices run in smaller functions; this file does not. |
| [graph-router.emath](numerical/graph-router.emath) | Target sketch. Graphs and `solve` are not admitted. |
| [heat-pde.emath](numerical/heat-pde.emath) | Target sketch. Fields and PDEs are not admitted. |

## search — conjecture and optimization

| Example | Focus |
|---------|-------|
| [conjecture-search.emath](search/conjecture-search.emath) | Conjecture-driven search |
| [bounded-conjecture-search.emath](search/bounded-conjecture-search.emath) | Bounded conjecture search |
| [certificate-checked-optimization.emath](search/certificate-checked-optimization.emath) | Certificate-verified optimization |

## integration — providers, plugins, genesis, and caching

| Example | Focus |
|---------|-------|
| [arbitrary-glyphs.emath](integration/arbitrary-glyphs.emath) | Reference genesis source; glyph identifier flexibility |
| [cache-policy.emath](integration/cache-policy.emath) | Compile-time caching policy |
| [custom-kind.emath](integration/custom-kind.emath) | Custom kinds and schema |
| [custom-provider.emath](integration/custom-provider.emath) | Provider integration |
| [host-cache-policy.emath](integration/host-cache-policy.emath) | Host-side caching |
| [parametric-unknown-operator.emath](integration/parametric-unknown-operator.emath) | Unknown operator parameterization |
| [token-compression-system.emath](integration/token-compression-system.emath) | Token compression workflow |

Examples illustrate intended semantics; the phase documents state when each
becomes executable.
