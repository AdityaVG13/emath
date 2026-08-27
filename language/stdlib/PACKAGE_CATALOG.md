# Planned Standard Package Catalog

Most Phase 1 entries below are compiler contracts rather than importable
`.emath` packages. The
compiler binds `core::math::` / `math::` / bare names for the functions
listed in `README.md`. `core::prelude` and `core::numbers` are not
callable namespaces yet.

| Package | Responsibility | First phase |
|---|---|---:|
| `core::math` | elementary arithmetic and libm builtins (implemented as compiler builtins, not a package source) | 1 |
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
| `core::graphs` | graph values and algorithms contracts | 5/7 |
| `core::probability` | distributions/sampling/evidence | 7 |
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
