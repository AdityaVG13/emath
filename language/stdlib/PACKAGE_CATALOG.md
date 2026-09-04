# Planned Standard Package Catalog

Most Phase 1 entries below are compiler contracts rather than importable
`.emath` packages. The
compiler binds `core::math::` / `math::` / bare names for the functions
listed in `README.md`. `core::prelude` and `core::numbers` are not
callable namespaces yet.

| Package | Responsibility | First phase |
|---|---|---:|
| `std.core` (object pack) | executable standard-library pack: theory + cell + evidence objects with MeaningIDs, mounted by `emath library mount std` | 13 |
| `core::math` | elementary arithmetic and libm builtins (implemented as compiler builtins, not a package source) | 1 |
| [`core::special_functions`](cells/std-special-functions.md) | Γ, B, erf, ζ, W₀, K, E, Π contracts with named branches, certified strict-f64 reference impls, and the SpecialFunctionEvaluator provider seam (e2e admission follow-up) | 11 |
| [`core::lp_milp`](cells/std-lp-milp.md) | deterministic LP simplex (Bland) + MILP branch-and-bound + Pareto-front nucleus; `.emath` goal surface (`objectives(pareto):`) lowers into this contract (admission follow-up) | 6/7 |
| [`core::number_theory`](cells/std-numtheory-comb.md) | B16: deterministic Miller–Rabin `is_prime` (u64), trial-division `factorize`, `gcd`/`lcm` (typed overflow refusal), normalized `congruence` predicate; `factorial`+`congruence` admit today, rest admission follow-up | 1 |
| [`core::algebra`](cells/std-quaternions.md) | B44 nucleus: Hamilton `Quaternion` (`quat(w,x,y,z)`; non-commutative law pinned; zero normalize/inverse refuse), `Dual` (ε²=0 exact first-order derivatives), `CliffordBasis(p,q)` + sparse `MultiVector` (derived multiplication table); C18 collision avoided by having NO new literal suffix; admission-table follow-up | 1 |
| [`core::combinatorics`](cells/std-numtheory-comb.md) | B17: exact-i128 `factorial`/`binomial` (typed overflow refusal), `Permutation` finite carrier (C10 value-ctor workaround), budgeted lexicographic enumeration with resumable continuation | 1 |
| [`core::game_theory`](cells/std-game-theory.md) | B41 finite-carrier claims: `BimatrixGame` + `is_nash_equilibrium` (claim CHECKER, never a search oracle), mixed-profile support condition, best responses as tie SETS, validated `MixedStrategy` (mass never renormalized); infinite/continuous games out by construction; admission-table follow-up | 1 |
| `core::prelude` | common stable imports | 1 |
| `core::logic` | propositions, predicates, quantifiers | 4 |
| `core::numbers` | numeric towers and profiles | 1/5 |
| `core::units` | dimensions, scales, quantities | 5 |
| `core::shapes` | ranks, extents, layout contracts | 5 |
| `core::domains` | intervals, sets, measures, boundaries | 5 |
| `core::collections` | sequences/maps/sets | 4 |
| `core::linear_algebra` | vectors/matrices/operators | 5/7 |
| `core::calculus` | derivative/integral goal contracts | 6 |
| `core::optimization` | constraints/objectives/certificates | 6/7 |
| [`optimization::methods`](cells/std-optimization-methods.md) | continuous nucleus: Newton (Armijo line search) + BFGS engines, KKT residual helper, typed refusals (singular Hessian / budget with achieved gradient / stalled line search); interior-point + SQP refuse by name (phased landing); goal-surface method selection is the wiring follow-up | 6 |
| `core::graphs` | graph values and algorithms contracts | 5/7 |
| `core::probability` | distributions/sampling/evidence | 7 |
| [`probability::information`](cells/std-probability.md) | B22 slice: entropy/KL/mutual information over discrete carriers (bits, declared base variants, support-violation refusals); differential entropy refuses by name (measure-world contract); B10 random variables world-gated | 7 |
| `core::state` | state/events/clocks/transitions | 4/5 |
| `core::evidence` | claims, assumptions, evidence kinds | 8 |
| `core::artifact` | manifests/source maps/continuations | 9 |
| `core::host` | host bindings and fallback contracts | 9/10 |
| [`physics::classical`](laws/physics-classical.emath) | executable classical-mechanics laws | 1 |
| [`physics::relativity`](laws/physics-relativity.emath) | executable special-relativity slice and GR deferrals | 1 |
| [`cs::laws`](laws/computer-science.emath) | executable systems laws and open-problem deferrals | 1 |
| [`probability::laws`](laws/probability-statistics.emath) | finite Bayes, CLT scaling, and information slices | 1 |
| [`analysis::laws`](laws/analysis.emath) | constructive endpoint, Taylor, and contraction slices | 1 |
| [`number_theory::laws`](laws/algebra-number-theory.emath) | exact modular laws and conjecture no-claims | 1 |
| [`optimization_control::laws`](laws/optimization-control.emath) | finite KKT, Bellman, and Lyapunov slices | 1 |
| [`approximation::laws`](laws/approximation.emath) | executable Taylor/Chebyshev/Padé laws with declared regimes ([cell contract](cells/std-approx-expansions.md)) | 1 |
