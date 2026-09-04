# Examples

This directory is a small teaching set, not a feature inventory. Start with the first four files, then choose a complete workflow close to your problem. The normative language reference documents the full surface. Exact arithmetic examples resolve through the authored capsules in [`../spec/capabilities/exact/rational.emath`](../spec/capabilities/exact/rational.emath) and [`../spec/capabilities/exact/number-theory.emath`](../spec/capabilities/exact/number-theory.emath); GCD/LCM remain host-only capsule candidates and therefore have no misleading `.emath` example.

## Start here

1. [`hello-square.emath`](intro/hello-square.emath) - a named function with typed input and output.
2. [`scratch.emath`](intro/scratch.emath) - declaration-free calculation and inspection.
3. [`l1_guided.emath`](intro/l1_guided.emath) - a relationship, worked value, expansion, and exactness budget.
4. [`units.emath`](intro/units.emath) - quantities and dimensional checking.

## Mathematical workflows

5. [`autodiff.emath`](intro/autodiff.emath) - derivatives through executable definitions. **Interpreter world correct; compiled-Rust world disagrees today** (generated `auto_diff_parabola` test fails; forward-mode dual-number backend defect; the divergence is the detecting signal, recorded in the divergence ledger).
6. [`solve.emath`](intro/solve.emath) - root finding with an explicit unknown.
7. [`solve_x2_eq_2.emath`](intro/solve_x2_eq_2.emath) - labeled real, complex, modular, symbolic, and numeric intent completions.
8. [`optimize.emath`](intro/optimize.emath) - a declared optimization goal.
9. [`sets-records.emath`](intro/sets-records.emath) - finite sets, comprehensions, membership, and records.
10. [`match-expressions.emath`](intro/match-expressions.emath) - total value dispatch.
11. [`polynomials.emath`](algebra/polynomials.emath) - polynomial values and evaluation.
12. [`symbolic-cas.emath`](algebra/symbolic-cas.emath) - exact symbolic simplification.
13. [`eigen-svd.emath`](linear-algebra/eigen-svd.emath) - spectral decomposition and iterative solving.
14. [`bellman-ford.emath`](graphs/bellman-ford.emath) - negative-edge shortest paths.
15. [`transfer-function.emath`](control/transfer-function.emath) - transfer evaluation and stability.
16. [`fibonacci-sequence.emath`](intro/fibonacci-sequence.emath) - indexed recurrences, coefficient extraction, and generating-function convolution.

## Scientific modeling

17. [`explicit-mass-spring.emath`](numerical/explicit-mass-spring.emath) - a state model and explicit or adaptive simulation.
18. [`solver-methods.emath`](numerical/solver-methods.emath) - stiff and symplectic integration with structural safeguards.
19. [`heat-rod-sim.emath`](numerical/heat-rod-sim.emath) - a spatial heat workflow.
20. [`newton-second.emath`](physics/newton-second.emath) - an executable law with units and evidence.
21. [`seeded_sampling.emath`](probability/seeded_sampling.emath) - deterministic root, split, and replay streams.
22. [`observations.emath`](science/observations.emath) - measured evidence, provenance, and pure deterministic reports.
23. [`wind-series.emath`](science/wind-series.emath) - time-series interpolation and extrapolation policy.
24. [`special-functions.emath`](numerical/special-functions.emath) - certified special-function values with declared error bounds.
25. [`rational-cells.emath`](numerical/rational-cells.emath) - exact rational arithmetic (`Rat`): `rat`, `rat_add`, `rat_norm`; gcd-reduced, overflow- and zero-denominator faults.
25. [`dae-rc-circuit.emath`](numerical/dae-rc-circuit.emath) - index-1 implicit DAE (RC circuit) with a GENERIC event-triggered switch through the `transitions:` channel: algebraic unknown (`current`), differential state (`charge`), square KVL residual; the declared event names the crossing (`charge` reaches `capacitance * threshold_voltage`) and the firing `on ThresholdCrossed:` rule dispatches the switch (`voltage = 0`), switching the trajectory from charging to discharging (ch7, transitions slice). Check record: [`dae-rc-circuit-check.md`](numerical/dae-rc-circuit-check.md); execute oracle (identical flags in the file header and here):


```sh
emath simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit \
  --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set current=10 --set charge=0 \
  --method backward-euler --dt 0.1 --t1 1.0
```
26. [`combustion-mass-balance.emath`](chemistry/combustion-mass-balance.emath) - stoichiometric mass-balance certificate (`std.chem.mass_balance`): balanced `2 H2 + O2 -> 2 H2O` admits with an exact zero residual; unbalanced forms diagnose typed `MassImbalance` naming the violating element and residual.
27. [`pk-two-compartment-fit.emath`](science/pk-two-compartment-fit.emath) - generic fit goal (04 §5.3): `fit k_el, V_central to conc_time` with model, prediction, residual method, explicit weights, optimizer method, initial seeds, and the identifiability honesty gate as plain program data. The PK model lives in `.emath`, not Rust.
28. [`molecular-graph-valence.emath`](chemistry/molecular-graph-valence.emath) - reaction-mechanism rewrite as (L, K, R) graph data: the allyl shift `C1=C2-C3 -> C1-C2=C3` preserves every context atom's valence, certified by `std.chem.graph_rewrite_preserve`; a broken bond diagnoses typed `ValenceImbalance`.
29. [`thermo-equilibrium.emath`](chemistry/thermo-equilibrium.emath) - Wegscheider cycle consistency (`std.chem.cycle_consistent`) plus ideal-mixture Gibbs equilibrium along a reaction extent through the existing goal path, with the stoichiometry certified by `std.chem.mass_balance`.
30. [`01_softmax_cell.emath`](intro/01_softmax_cell.emath) - a biform capability cell: one cell, two authorities. The `spec:` side (laws, types, units) and the `algorithm:` side (reference semantics) each bind their own quoted evidence object, so a green algorithm test never stamps the spec proved. `emath check` admits the example; authority laundering (one evidence object on both sides), a missing side, and provider receipts on the spec side diagnose typed `E-CELL-011`, `E-CELL-009`, and `E-CELL-010`. Softmax here is fixture data; the cell name and evidence tokens, never a domain-named Rust branch.
31. [`3d-primitives.emath`](geometry/3d-primitives.emath) - 3D geometry primitives over the generic Vector[3] surface: cross (basis axes, anti-symmetry), norm/length, distance, sphere containment/volume/surface, plane signed distance, per-axis bounding-box membership, flat triangle-soup area with exact-zero signed volume. Every operation is the inline formula `std.geometry.cartesian3` pins; the named `cross`/`normalize`/`distance` builtins are gated on the generic declared-function/capability call seam (`language/reference/geometry-and-topology.md` §4). `emath check` admits it and all six example blocks evaluate (see `emath-ir-tests --test geometry3d`).
32. [`eval-function.emath`](intro/eval-function.emath) - `emath eval` over an ordinary admitted function spec (generic EMIR/reference-VM lane): `--set` binds declared inputs, `--function NAME` selects among several declarations, and plain `eval` runs the spec's own worked example as the input oracle; the deterministic `emath.eval-function` receipt carries `meaning_id` provenance (typed `E-EVAL-*` diagnoses on the closure).
33. [`parametric-surfaces.emath`](geometry/parametric-surfaces.emath) - parametric curves and surfaces through the user-defined-function call path: space curve `r(t) -> Vector[3]`, surfaces `paraboloid`/`sphere`/`torus` `(u,v) -> Vector[3]`, and the implicit field `f(p) = x²+y²+z²` over `Vector[3]`, all ordinary `emath function` declarations called from one acceptance function. Calls resolve through the generic declared-call seam by pure-inline substitution at sema (no new IR node, no registry entry; recursion diagnoses `E-TYPE-013`, arity/type mismatches diagnose `E-TYPE-012`; see `language/reference/geometry-and-topology.md` §4). Values pinned at exactly-representable points (`r(0.5) = [0.5, 0.25, 1.0]`, north pole `(0,0) → (0,0,1)`, torus outer equator `(3,0,0)`, `f([1,2,2]) = 9`). This file is the pinned artifact of `emath-ir-tests --test parametric_surfaces` (curve/surface/implicit tests `include_str!` it).
34. [`sde-control.emath`](numerical/sde-control.emath) - deterministic scalar SDE execution: Itô (Euler–Maruyama) and Stratonovich (Euler–Heun) as PURE capability cells (`std.stochastic.euler_maruyama` / `std.stochastic.stratonovich`) called through the generic declared-capability path; no `sde` keyword and no mode switch; the rules are cell data. The seed is required (same seed ⟹ bit-identical trajectory; the Z draws are the same Normal(0,1) stream the `std.prob` sampler uses); state-dependent noise makes the two rules differ (the `spread` definition is strictly positive), additive noise makes them agree bit-for-bit. Control cells (`transfer_eval`, `poles_stable`) reuse the existing control surface unchanged.

## Composite compute types

35. [`option-result-ops.emath`](intro/option-result-ops.emath) - `Option<T>` / `Result<T,E>` EXPRESSION ops from ordinary `.emath` text: `option_some`/`option_none`/`option_is_some`/`option_unwrap_or` and `result_ok`/`result_err`/`result_is_ok`/`result_unwrap_or`/`result_error_of`, nesting (`Some(None)` kept distinct), total `unwrap_or` (no panic). Nullary → `emath eval --function carrier_ops` runs it (`os=true, ou=9.0, proj=7.0`-style receipt); scalar/carrier misuse diagnoses `E-TYPE-012`. Proven by the option/result semantics tests.
36. [`field-mod-arithmetic.emath`](intro/field-mod-arithmetic.emath) - `Field<p>`/`GF<p>` exact-integer prime-field arithmetic as capability-cell data over the universal `int_rem` (`a.rem_euclid(m)` on i64) and the exact inverse `field_inv(a,p)`: `field7_add`/`field7_mul`/`field7_inv`/`field7_rem` (user-named data, no field-named compiler branch). Nullary `field7_ops` runs via `emath eval --function field7_ops` (`add=0, mul=5, inv=5, rem=6`); float defs in a `Field` output diagnose `E-TYPE-012`; non-positive `int_rem` modulus is a typed runtime fault, never a panic. Also demonstrates the exact number-theory builtins `pow_mod` (`field7_pow`, square-and-multiply over i128 intermediates) and Tonelli-Shanks `sqrt_mod` (`field7_sqrt(2) = 3`; non-residues diagnose typed), plus a multi-binder fold (`field7_grid`, `sum i in 0..n, j in 0..m`). Proven by the Field-arithmetic tests across the semantics, IR, and backend suites.
37. [`option-result-graph-field.emath`](intro/option-result-graph-field.emath) - the composite-type admission surface plus the Graph compute: a `g: Graph` output with `graph { <nodes>; <edges> }`, driven through `reachability`/`shortest_distances`/`out_degrees` (dense `Matrix<Float64>` adjacency alias; `Graph` and `Matrix<Float64>` interchange bidirectionally; bare `Graph` only, `Graph<T>` → `E-TYPE-010`). **Runnable** via `emath eval --function AdjacencyFieldsFromSource`. `CompositeTypesDeclare` is check-only admission for the composite INPUT spellings. Proven by the graph/field semantics and invariant tests.

## Research probes

38. [`euler-criterion-p-sweep.emath`](research/euler-criterion-p-sweep.emath) - the probe-lab workflow (reference chapter 18) as a runnable example: Euler's criterion (`pow_mod(a, (p-1)/2, p)`) cross-checked against the Tonelli-Shanks gate (`sqrt_mod`, typed non-residue diagnosis as the signal), with a binder-guard residue count (`(p-1)/2` — the criterion cannot call 0 a residue). **Runnable** via `emath eval --function euler_symbol --set a=3 --set p=7` (and `residue_sqrt`/`residue_count`); hand-checked at p = 7 (`euler_symbol(3,7) = 6 = p-1`, `residue_sqrt(2,7) = 3`, `residue_count(7) = 3`). The header documents the p-sweep discipline and the independent cross-engine check via `emath build --bin euler_symbol` (compiled probe, byte-identical value lines). A larger staircase exemplar is deliberately withheld pending prize progress — see the non-inclusion note in reference chapter 18.

39. [`mersenne-field-p25519.emath`](research/mersenne-field-p25519.emath) - the stage-2 big-integer lane (reference, `Mod<p>`/`GF<p>` builtins table) at the Curve25519 prime 2^255−19: five functions over the six modular builtins with hand-derived exact values — `pow_mod(2, (p-1)/2, p) = p-1` (Euler's criterion; 2 is a non-residue because p ≡ 5 mod 8), `mod_inv(2, p) = (p+1)/2`, `sqrt_mod(4, p) = 2`, `int_rem(-5, p) = p-5` (the exact-Euclidean sign law across the width boundary), and `rs_encode([1,2], 3, p) = [1, 3, 5]`. **Runnable** via `emath eval --function euler_symbol --set a=2` (etc.); the compiled probe (`emath build --bin euler_symbol`) matches the interpreter exactly — the big kernels ride the SOURCE embed into the generated crate.

## Wave-16 catalog cells and examples

Finite-field, probability, analysis, and geometry capability-cell packs (all `emath check` clean and `emath test` green; contracts indexed in [`../stdlib/README.md`](../stdlib/README.md)):

40. [`fp-polynomial-census.emath`](algebra/fp-polynomial-census.emath) - fiber census of polynomial maps over F_p via `poly_eval_mod`; Fermat whole-field census at p = 7.

41. [`fp-multiplicative-group.emath`](algebra/fp-multiplicative-group.emath) - Lagrange, mu_d, generator, and Euler censuses over F_13^* via `pow_mod`/`sqrt_mod`.

42. [`congruence-wilson.emath`](algebra/congruence-wilson.emath) - Wilson's theorem check via `congruence` + `factorial`, Euclidean negative-residue normalization.

43. [`markov_chain_evolution.emath`](probability/markov_chain_evolution.emath) - one-step, two-step, and stationary Markov distributions; einsum transition contraction.

44. [`monte_carlo_quadrature.emath`](probability/monte_carlo_quadrature.emath) - deterministic-seeded Monte Carlo mean and second-moment estimates.

45. [`bayes_grid_posterior.emath`](probability/bayes_grid_posterior.emath) - discrete-grid Bayes posterior, odds update, and law of total probability.

46. [`finite-spectra-fp.emath`](analysis/finite-spectra-fp.emath) - exact 2x2 matrix spectra over F_p: characteristic polynomial, eigenvalue membership, eigenvector certificate.

47. [`finite-fourier-ntt.emath`](analysis/finite-fourier-ntt.emath) - exact number-theoretic transform over F_p: forward/inverse kernels, Parseval, cyclic convolution theorem.

48. [`finite-opnorms.emath`](analysis/finite-opnorms.emath) - induced 1/infinity and Frobenius-squared norms exact; spectral norm honest-approximate with an explicit bracket.

49. [`finite-shift-operator.emath`](analysis/finite-shift-operator.emath) - cyclic shift on F_7^3: S^3 = I, spectrum = cube roots of unity, Fourier-mode eigenvectors.

50. [`affine-barycentric.emath`](geometry/affine-barycentric.emath) - affine combinations, orient2 signed area, barycentric coordinates, 3-D coplanarity; integer-exact witnesses.

51. [`metric-fundamentals.emath`](geometry/metric-fundamentals.emath) - squared-L2 and L2 metrics, Cauchy-Schwarz slack, triangle slacks, scale homogeneity.

52. [`spherical-fundamentals.emath`](geometry/spherical-fundamentals.emath) - chord/dot slice of the unit sphere (chord^2 = 2 - 2*dot); central-angle forms are named fences.

53. [`projective-cross-ratio.emath`](geometry/projective-cross-ratio.emath) - cross-ratio with its affine invariance, homogeneous map witness, incidence via det3.

54. [`worlds/metric-space-world.emath`](geometry/worlds/metric-space-world.emath) - metric-space world: (R^3, L2) and (unit sphere, chord) instances with metric axioms as pinned exact slacks; L1/L-infinity instances are named fences (cell-revision scope; the `abs`/`max` calls they need admit end-to-end since emath-s9w1m).

55. [`worlds/inner-product-world.emath`](geometry/worlds/inner-product-world.emath) - inner-product world over (R^3, dot): axioms as exact slacks, Cauchy-Schwarz with equality case, induced-norm homogeneity, parallelogram law, polarization identity.

56. [`affine-geometry-world.emath`](geometry/worlds/affine-geometry-world.emath) - affine-geometry world: orient2, barycentric coordinates, and the affine-map law T(sum lambda_i P_i) = sum lambda_i T(P_i) exact, with barycentric invariance under affine maps.
57. [`clamp-distance-builtins.emath`](intro/clamp-distance-builtins.emath) - the six strict-f64 builtin contracts as one pinned witness: `abs`/`min`/`max` piecewise (with a `min(max(v, lo), hi)` clamp), `sqrt` exact roots (`sqrt(4) = 2`, `sqrt(2.25) = 1.5`; a negative operand yields the IEEE NaN value under the caller's `SqrtNonNegative` obligation), IEEE `atan2` (`atan2(1, 0) = 1.5707963267948966`), `sign` = sgn with exactly 0 at zero. Admitted end-to-end since emath-s9w1m + fpl60; `emath eval --set x=-3 --set y=7 --set v=9 --set b=3.7` matches all 12 pins; mutant-killed.
58. [`feature-capsules.emath`](intro/feature-capsules.emath) - two candidate
Feature Capsules using the generic `emath feature` shell: foundational exact
addition and a nontrivial finite-field pack. The source separates stable
FeatureID, semantic hash, typed edge/projection data, maturity, conformance,
presentation aliases, and agent guidance; admission grants no live authority.
59. [`add-exact.emath`](intro/add-exact.emath) - first-cutover positive:
`2 + 1` produces exact Int `3` in `std.world.exact.int`, with source/value
artifacts compared across legacy, capsule candidate, and independent readers.
The file header pins the oracle `emath eval language/examples/intro/add-exact.emath --json`
(result = 3, label exact, world `std.world.exact.int`); that pinned dual-run
oracle is distinct from capsule reference execution. The independent reader
test (`tests/emath-exec-ir/tests/independent_language_reader.rs`) reproduces
distribution identity, authority, and the pinned result from the checked-in
bytes without running the user program through product code.
60. [`float-into-int.emath`](intro/float-into-int.emath) - first-cutover
negative: Float `1.5` into Int yields the authorized exactness-loss diagnosis;
no widening, wrapping, or silent coercion.
61. [`sum-first-n.emath`](intro/sum-first-n.emath) - capsule-first post-cutover
workflow: `std.binder.sum` defines a finite additive fold while Rust supplies
only the generic binder/fold mechanism; `n = 5` produces exact Int `10`.
62. [`wave16-space-kind.emath`](intro/wave16-space-kind.emath) - honest
catalog-only Feature Capsule for `std.kind.space`. The generic shell parses,
while a blocking semantic Spec Hole prevents any computing/live claim.

## Distribution-level acceptance

The examples above are source programs; their live capsule authority comes from
the checked-in distribution, not from an image-ID argument or an example label.
The focused capstone at
`tests/emath-exec-ir/tests/portable_language_capstone.rs` installs that verified
distribution and runs representative exact, linear, special/probability,
graph/optimization/game, control/PDE, and domain-science capabilities through
the generic `ApplyCapability`/native-kernel seam. The independent reader at
`tests/emath-exec-ir/tests/independent_language_reader.rs` decodes the real
`language.image`, lock, and source-map bytes and reproduces distribution
identity, authority, and the `add-exact.emath` result without fake pages.

These candidates intentionally have no runnable native-kernel claim yet:
`tensor.index`, `tensor.einsum`, `reduction.finite`, `special.elliptic-pi`,
`statistics.median`, `dynamics.simulation-world`, and
`pde.tensor-and-divergence`. Their capsule holes remain visible rather than
being filled by a handwritten fallback.

## Running an example

```sh
emath check language/examples/intro/hello-square.emath
emath eval language/examples/intro/eval-function.emath --set a=2 --set b=3 --json
emath expand language/examples/intro/l1_guided.emath
emath simulate language/examples/numerical/solver-methods.emath \
  --model StiffDecay --set y=1 \
  --method backward-euler --dt 0.1 --t1 0.3
emath simulate language/examples/numerical/solver-methods.emath \
  --model HarmonicOscillator --set q=1 --set v=0 \
  --method velocity-verlet --dt 0.01 --t1 1
emath simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set current=10 --set charge=0 \
  --method backward-euler --dt 0.1 --t1 1.0
```

An example may intentionally demonstrate a typed diagnosis or a labeled
`fault`. Its header states the expected command and result.
