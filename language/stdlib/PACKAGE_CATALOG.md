# Planned Standard Package Catalog

Phase 1 does not ship these as importable `.emath` packages. The
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
