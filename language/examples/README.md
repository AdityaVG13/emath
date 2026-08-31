# Examples

This directory is a small teaching set, not a feature inventory. Start with the first four files, then choose a complete workflow close to your problem. The normative language reference documents the full surface.

## Start here

1. [`hello-square.emath`](intro/hello-square.emath) - a named function with typed input and output.
2. [`scratch.emath`](intro/scratch.emath) - declaration-free calculation and inspection.
3. [`l1_guided.emath`](intro/l1_guided.emath) - a relationship, worked value, expansion, and exactness budget.
4. [`units.emath`](intro/units.emath) - quantities and dimensional checking.

## Mathematical workflows

5. [`autodiff.emath`](intro/autodiff.emath) - derivatives through executable definitions. **Run currently refused** (generated `auto_diff_parabola` test fails; forward-mode dual-number backend defect — divergence ledger).
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
25. [`dae-rc-circuit.emath`](numerical/dae-rc-circuit.emath) - index-1 implicit DAE (RC circuit) with a GENERIC event-triggered switch through the `transitions:` channel: algebraic unknown (`current`), differential state (`charge`), square KVL residual; the declared event names the crossing (`charge` reaches `capacitance * threshold_voltage`) and the firing `on ThresholdCrossed:` rule dispatches the switch (`voltage = 0`), switching the trajectory from charging to discharging (r3-dynamical-03lh ch7, transitions slice). Check record: [`dae-rc-circuit-check.md`](numerical/dae-rc-circuit-check.md); execute oracle (identical flags in the file header and here):

```sh
emath simulate language/examples/numerical/dae-rc-circuit.emath \
  --model RCCircuit \
  --set voltage=10 --set resistance=1 --set capacitance=1 \
  --set threshold_voltage=5 --set current=10 --set charge=0 \
  --method backward-euler --dt 0.1 --t1 1.0
```
26. [`combustion-mass-balance.emath`](chemistry/combustion-mass-balance.emath) - stoichiometric mass-balance certificate (`std.chem.mass_balance`): balanced `2 H2 + O2 -> 2 H2O` admits with an exact zero residual; unbalanced forms refuse typed `MassImbalance` naming the violating element and residual.
27. [`pk-two-compartment-fit.emath`](science/pk-two-compartment-fit.emath) - generic fit goal (04 §5.3): `fit k_el, V_central to conc_time` with model, prediction, residual method, explicit weights, optimizer method, initial seeds, and the identifiability honesty gate as plain program data. The PK model lives in `.emath`, not Rust.
28. [`molecular-graph-valence.emath`](chemistry/molecular-graph-valence.emath) - reaction-mechanism rewrite as (L, K, R) graph data: the allyl shift `C1=C2-C3 -> C1-C2=C3` preserves every context atom's valence, certified by `std.chem.graph_rewrite_preserve`; a broken bond refuses typed `ValenceImbalance`.
29. [`thermo-equilibrium.emath`](chemistry/thermo-equilibrium.emath) - Wegscheider cycle consistency (`std.chem.cycle_consistent`) plus ideal-mixture Gibbs equilibrium along a reaction extent through the existing goal path, with the stoichiometry certified by `std.chem.mass_balance`.
30. [`01_softmax_cell.emath`](intro/01_softmax_cell.emath) - a biform capability cell (bead `emath-biform-cells-jswu6`): one cell, two authorities. The `spec:` side (laws, types, units) and the `algorithm:` side (reference semantics) each bind their own quoted evidence object, so a green algorithm test never stamps the spec proved. `emath check` admits the example; authority laundering (one evidence object on both sides), a missing side, and provider receipts on the spec side refuse typed `E-CELL-011`, `E-CELL-009`, and `E-CELL-010`. Softmax here is fixture data — the cell name and evidence tokens, never a domain-named Rust branch.
31. [`3d-primitives.emath`](geometry/3d-primitives.emath) - 3D geometry primitives (bead `emath-talo`) over the generic Vector[3] surface: cross (basis axes, anti-symmetry), norm/length, distance, sphere containment/volume/surface, plane signed distance, per-axis bounding-box membership, flat triangle-soup area with exact-zero signed volume. Every operation is the inline formula `std.geometry.cartesian3` pins — the named `cross`/`normalize`/`distance` builtins are gated on the generic declared-function/capability call seam (`language/reference/geometry-and-topology.md` §4). `emath check` admits it and all six example blocks evaluate (see `emath-ir-tests --test geometry3d`).
32. [`eval-function.emath`](intro/eval-function.emath) - `emath eval` over an ordinary admitted function spec (generic EMIR/reference-VM lane): `--set` binds declared inputs, `--function NAME` selects among several declarations, and plain `eval` runs the spec's own worked example as the input oracle; the deterministic `emath.eval-function` receipt carries `meaning_id` provenance (typed `E-EVAL-*` refusals on the closure).
33. [`parametric-surfaces.emath`](geometry/parametric-surfaces.emath) - parametric curves and surfaces (bead `emath-0e68`) through the user-defined-function call path: space curve `r(t) -> Vector[3]`, surfaces `paraboloid`/`sphere`/`torus` `(u,v) -> Vector[3]`, and the implicit field `f(p) = x²+y²+z²` over `Vector[3]`, all ordinary `emath function` declarations called from one acceptance function. Calls resolve through the generic declared-call seam by pure-inline substitution at sema (no new IR node, no registry entry; recursion refuses `E-TYPE-013`, arity/type mismatches refuse `E-TYPE-012`; see `language/reference/geometry-and-topology.md` §4). Values pinned at exactly-representable points (`r(0.5) = [0.5, 0.25, 1.0]`, north pole `(0,0) → (0,0,1)`, torus outer equator `(3,0,0)`, `f([1,2,2]) = 9`). This file is the pinned artifact of `emath-ir-tests --test parametric_surfaces` (curve/surface/implicit tests `include_str!` it).
34. [`sde-control.emath`](numerical/sde-control.emath) - deterministic scalar SDE execution (bead `emath-r3-sde-control-zxkl`): Itô (Euler–Maruyama) and Stratonovich (Euler–Heun) as PURE capability cells (`std.stochastic.euler_maruyama` / `std.stochastic.stratonovich`) called through the generic declared-capability path — no `sde` keyword and no mode switch; the rules are cell data. The seed is required (same seed ⟹ bit-identical trajectory; the Z draws are the same Normal(0,1) stream the `std.prob` sampler uses); state-dependent noise makes the two rules differ (the `spread` definition is strictly positive), additive noise makes them agree bit-for-bit. Control cells (`transfer_eval`, `poles_stable`) reuse the existing control surface unchanged.

## Composite compute types (bead emath-option-result-graph-field-aj8d)

35. [`option-result-ops.emath`](intro/option-result-ops.emath) - `Option<T>` / `Result<T,E>` EXPRESSION ops from ordinary `.emath` text: `option_some`/`option_none`/`option_is_some`/`option_unwrap_or` and `result_ok`/`result_err`/`result_is_ok`/`result_unwrap_or`/`result_error_of`, nesting (`Some(None)` kept distinct), total `unwrap_or` (no panic). Nullary → `emath eval --function carrier_ops` runs it (`os=true, ou=9.0, proj=7.0`-style receipt); scalar/carrier misuse refuses `E-TYPE-012`. Proven by `aj8d_text_*` sema tests.
36. [`field-mod-arithmetic.emath`](intro/field-mod-arithmetic.emath) - `Field<p>`/`GF<p>` exact-integer prime-field arithmetic as capability-cell data over the universal `int_rem` (`a.rem_euclid(m)` on i64) and the exact inverse `field_inv(a,p)`: `field7_add`/`field7_mul`/`field7_inv`/`field7_rem` (user-named data, no field-named compiler branch). Nullary `field7_ops` runs via `emath eval --function field7_ops` (`add=0, mul=5, inv=5, rem=6`); float defs in a `Field` output refuse `E-TYPE-012`; non-positive `int_rem` modulus is a typed runtime fault, never a panic. Proven by `aj8d_field*` sema/ir/backend tests + `aj8d_meta_field7_distribution_law`.
37. [`option-result-graph-field.emath`](intro/option-result-graph-field.emath) - the composite-type admission surface plus the Graph compute: a `g: Graph` output with `graph { <nodes> ; <edges> }`, driven through `reachability`/`shortest_distances`/`out_degrees` (dense `Matrix<Float64>` adjacency alias; `Graph` and `Matrix<Float64>` interchange bidirectionally; bare `Graph` only, `Graph<T>` → `E-TYPE-010`). **Runnable** via `emath eval --function AdjacencyFieldsFromSource`. `CompositeTypesDeclare` is check-only admission for the composite INPUT spellings. Proven by `aj8d_graph_*`, `aj8d_matrix_field_*`, and `aj8d_meta_graph_relabel_*` tests.

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

An example may intentionally demonstrate a typed refusal. Its header states the expected command and result.
