# Standard Library

The standard library defines portable mathematical contracts and small reference implementations. Large solver portfolios and hardware-specific implementations belong in provider packages.

A stable standard item declares its domain, partiality, numeric behavior, deterministic identity, refusal behavior, and provider extension points.

## Packages

```text
core::prelude          core::logic             core::numbers
core::units            core::shapes            core::collections
core::linear_algebra   core::calculus          core::optimization
core::graphs           core::probability       core::state
core::evidence         core::artifact          core::host
```

Imported declaration schemas live under `std.kinds.*`; notation packs and domain packages use their own namespaces.

## Linear algebra

```emath
norm(v)
norm1(v)
norminf(v)
inner_product(u, v)
dot(u, v)
matvec(A, x)
solve_linear(A, b)
lu(A)
qr(A)
outer_product(u, v)
eigvals(A)
singular_values(A)
svd_factors(A)
solve_iterative(A, b)
```

Norm and inner-product arguments must be finite and shape-compatible. `matvec(A, x)` is the dense matrix-times-vector product (the generic carrier for linear residuals of every kind). `solve_linear` and `lu` require square matrices; singular systems refuse. `qr` accepts matrices with at least as many rows as columns.

`eigvals` accepts real symmetric square matrices and returns ascending eigenvalues. SVD accepts rectangular matrices and returns descending singular values. `solve_iterative` uses conjugate gradients and refuses when the SPD convergence contract is not met.

## Graphs

Graph algorithms use a square `Matrix<Float64>` adjacency carrier. `0.0` means no edge; nonzero values are weights. Vertex scans and tie breaks use ascending index order.

```emath
reachability(adj, source)
bfs_order(adj, source)
shortest_distances(adj, source)
bellman_ford(adj, source)
out_degrees(adj)
graph_laplacian(adj)
graph_symmetrize(adj)
sparse_triplets(adj)
sparse_from_triplets(n, triplets)
```

Dijkstra requires nonnegative weights. Bellman-Ford accepts negative edges and refuses a reachable negative cycle. Unreachable distance is positive infinity. Non-square carriers, invalid vertices, and non-finite weights refuse.

Sparse triplets are `[from, to, weight, ...]` in ascending edge order. Duplicate edges sum. `graph_symmetrize(A)` is `(A + A^T)/2`; spectral operations never symmetrize silently.

## Polynomials

Coefficient vectors are stored in ascending power order:

```emath
poly_add(p, q)
poly_mul(p, q)
poly_eval(p, x)
```

The empty vector is the zero polynomial. Non-finite coefficients or evaluation points refuse.

## Control

```emath
transfer_eval(numerator, denominator, x)
dc_gain(A, b, c)
poles_stable(denominator)
```

`transfer_eval` refuses at a pole. `poles_stable` applies the strict Routh-Hurwitz predicate and refuses degenerate marginal cases. `dc_gain` requires a stable square state matrix and compatible vectors; feedthrough is zero.

## Optimization

`lp_minimize(A, b, c)` solves the standard-form class `Ax <= b`, `x >= 0`, `b >= 0` using deterministic Bland tie-breaking. Unbounded or malformed problems refuse. `pareto_front(points)` returns the strict non-dominated mask and does not treat identical points as dominating each other.

The goal surface also provides nonlinear `solve`, `minimize`, and `maximize`; see the expressions chapter.

## ODE and PDE methods

```emath
ode_backward_euler_step(rate, y0, h)
ode_velocity_verlet_step(acceleration, position, velocity, h)
poisson_sine(load)
```

Backward Euler performs a bounded implicit Newton solve. Velocity Verlet preserves the separable `q' = v`, `v' = a(q)` structure. Invalid time steps, non-finite values, or non-convergence refuse.

`poisson_sine` solves the one-dimensional unit-interval Dirichlet problem using a deterministic discrete sine transform. Empty or non-finite loads refuse. Other boundary classes and dimensions require different methods.

## Probability

```emath
normal_sample([mean, sigma], seed, draws)
normal_sample([mean, sigma], seed, draws, "campaign.chain-a")
uniform_sample([low, high], seed, draws)
bernoulli_sample([p], seed, draws)
normal_density([mean, sigma], x)
uniform_density([low, high], x)
bernoulli_pmf([p], x)
```

Sampling uses the declared counter-based stochastic contract. Equal seed and stream path produce bit-identical draws; stream paths split deterministically and do not depend on call order. Invalid parameters, non-finite values, wrong arity, and excessive draw counts refuse.

Information-theory functions include discrete entropy in bits or nats, KL divergence, and mutual information. Probability carriers must be finite, nonnegative, and sum to one; they are never normalized silently. Differential entropy is a separate measure-world contract.

## Finite categories

```emath
category_check(dom, cod, composition)
diagram_commutative(dom, cod, composition, faces)
```

The checker exhaustively validates composition alignment, identities, and associativity on bounded finite carriers. A face is commutative only when its two paths compose to the same morphism. Malformed indices, shapes, or carriers beyond the configured bound refuse.

## Chemistry

```emath
mass_balance(S, s)
balance(S)
```

`std.chem.mass_balance` is the stoichiometric mass-balance certificate cell: the result is the per-element residual `S·s`, where `S` is the signed composition matrix (row per element, column per species, entry = atoms of the element in the species) and `s` is the signed coefficient vector (reactants positive, products negative). Balanced systems admit with an EXACT all-zero residual, which is the mass-balance evidence: small-integer stoichiometry is exact in the f64 carrier, so no tolerance is applied. A nonzero residual refuses typed `MassImbalance(element i, residual r)` naming the first violating element and its exact residual. Non-finite carriers refuse, and unbalanced forms never evaluate silently.

`std.chem.balance` derives the balanced equation: given the SIGN-BLIND species composition matrix (nonnegative integer entries), it returns the canonical primitive integer coefficient vector through the generic exact-integer nullspace primitive. "Primitive" means the entries are coprime and the first nonzero entry is positive, so the same reaction always reports the same vector up to species-column permutation (which permutes the vector identically) and element-row permutation or integer row scaling (which leave it unchanged). Certified chemical equations are validated by chaining `mass_balance(S, balance(S))`; the certificate refuses any derivation defect.

`balance` refuses typed when the system is not a valid chemical reaction: a non-integer entry (`E-NULLSPACE-001`), no nontrivial balance exists because each element appears in a single species, or the equation is underdetermined; several independent conservation equations (for example a species with zero atoms, `E-NULLSPACE-002`). It never guesses a basis vector for a higher-dimensional nullspace.

The generic exact-integer nullspace primitive (`int_nullspace(A)`) is also available directly: exact rational Gauss-Jordan elimination over i128 intermediates, no floating point; one-dimensional nullspaces yield the canonical primitive generator, anything else refuses `E-NULLSPACE-001/002`.

## Molecular graphs (reaction mechanisms)

```emath
graph_rewrite_preserve(L, K, R, u)
```

A reaction mechanism step is modeled as a rewrite rule over graphs carried by the generic dense `Matrix` carrier: rows are the CONTEXT atoms (the rule's interface), columns are the union of atoms across the left and right graphs, and each entry is the bond order (1 single, 2 double, 3 triple; 0 = no bond). A rule is the triple `(L, K, R)`; left-hand graph, shared context, right-hand graph; all with identical context-row × union-column dimensions; `K` has zero columns beyond the context.

`std.chem.graph_rewrite_preserve` checks the valence-conservation law across the rule span. The per-atom valence is the row's bond-order sum, computed by the generic `matvec(A, 1s)` op; the certificate is

```text
sum(abs(matvec(L,u) − matvec(K,u))) + sum(abs(matvec(K,u) − matvec(R,u)))
```

with `u` the all-ones vector. A rule whose context atoms all keep their valence admits with the EXACT zero certificate (atom-permutation invariant, disjoint-rule-composition additive, bond-order-scaling invariant). Any nonzero certificate refuses typed `ValenceImbalance(residual r)`; a bond-order break is never silent, and cancellation between atoms (one gains what another loses) is still a violation because the law is per-atom absolute.

Boundaries and no-claims: the carrier stores bond orders only. Element identities and formal charges live above this cell (property tables are later-slice work), so charge-only changes without a bond-order change are NOT detected by design; the typed negative here is the bond-order break that the carrier can express. `u` must be a finite vector whose length equals the column count.

## Thermo-equilibrium (Wegscheider consistency, Gibbs minimization)

```emath
cycle_consistent(P, Q)
exact_product_delta(P, Q)
```

A closed reaction cycle is thermodynamically consistent exactly when the product of the forward equilibrium constants equals the product of the reverse constants around the cycle (Wegscheider's law), computed over the rationals. Write each `K_i = p_i/q_i` in lowest terms; consistency is the exact integer equality `∏ p_i == ∏ q_i`.

`std.chem.cycle_consistent` certifies that equality: it is registry data over the generic `exact_product_delta(P, Q)` primitive, which computes `∏P − ∏Q` over u128/i128 intermediates with NO floating point. A consistent cycle admits with the exact zero delta; an inconsistent cycle refuses typed `CycleInconsistency(residual d)` where `d` is the exact witness difference. The certificate is invariant under cycle rotation, factor permutation, reversal, common unit scaling, and composition of consistent cycles (the products multiply); a zero factor degenerates a cycle and its mismatch refuses. Non-integral or overflow-prone entries refuse `E-EXACT-001/002`.

Gibbs free-energy minimization is NOT a new cell: the ideal-mixture free energy `G(ξ) = Σ n_i(ξ)·(μ0_i + RT·ln(n_i/N))` along a reaction extent `ξ`, with `n_i = n0_i + ν_i·ξ`, minimizes through the EXISTING single-variable goal path (`minimize(expr) wrt ξ`, Newton with line search) from the `core::optimization` methods library. Conservation is automatic along `ξ` because the stoichiometry vector `ν` is certified by `std.chem.mass_balance` (a null vector of the composition matrix); an UNconserved extent direction refuses at that cell before any minimization. Boundaries: `G` is convex only on the interior where every `n_i > 0`; the extent bounds and composition-dependent convexity are the caller's contract, and boundary minima are not claimed.

## Sets, number theory, and algebra

Finite sets provide extensional membership, comprehensions, and deterministic enumeration. Exact integer helpers include primality, factorization, GCD, LCM, factorial, binomial coefficients, permutations, modular inverse, and congruence. Overflow and invalid moduli refuse.

Quaternion, dual-number, and Clifford operations are exposed through named constructors and functions rather than new numeric suffixes. Their carrier and algebraic laws are documented in the corresponding cell contracts.

## Measurement and statistics

`core::measure` defines measured values, datasets, uncertainty, and the closed provenance variants. `core::statistics` returns labeled estimates that state method and sample size. Sample and population variance are distinct names. Empty or non-finite samples refuse, and significance requires an explicit classification call.

## Laws and imported kinds

Named mathematics is source, not a compiler builtin. Law packages execute through ordinary function lowering and retain assumptions, provenance, citations, and evidence.

The following schemas require explicit imports:

```text
std.kinds.capability   std.kinds.family      std.kinds.method
std.kinds.experiment   std.kinds.theory      std.kinds.model
std.kinds.morphism     std.kinds.migration   std.kinds.field_pack
```

Theory declarations do not self-certify. Finite models and morphisms gain evidence only after exhaustive checking. Methods and experiments remain proposals and cannot grant themselves authority.

## Capability cells (biform authority)

A capability declaration using the biform surface (`class: biform`) carries ONE cell with TWO authorities (bead `emath-biform-cells-jswu6`), reached from ordinary `.emath` source:

```emath
package std.math
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: "1.0.0"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        # what the cell claims: laws, types, units
        evidence: "evidence:std.math.softmax:spec:v1"
    algorithm:
        # how the claim is computed: reference semantics / bytecode
        evidence: "evidence:std.math.softmax:algorithm:v2"
```

The `spec:` and `algorithm:` sides each bind an INDEPENDENT quoted evidence object, with an optional `authority: authored | verified | provider` row (defaults: spec `authored`, algorithm `verified`). The bounded descriptor rows `class:` / `version:` / `migration:` and the side evidence parse into the capability layer's schema, and the closure planner assesses the sides at admission:

- a missing side refuses `E-CELL-009`; a missing spec is never "proved by the algorithm";
- an authority that cannot attest a side refuses `E-CELL-010`; a provider receipt may attest the algorithm by delegation but can never raise spec authority;
- one evidence object claimed for both sides refuses `E-CELL-011`; a green algorithm test never stamps the spec proved.

The cell name is namespaced by the declared `package <path>` (identity needs a stable namespace; a package-less biform declaration refuses `E-CELL-005`). Bounded admission reuses the capability layer's typed codes (`E-CELL-001` unknown class, `E-CELL-002` missing version, `E-CELL-004` arity bound). Legacy capability declarations without a `class:` row (inputs/outputs/definitions shape) keep the generic kind-application path unchanged.

## Provider contracts

Provider interfaces include root solvers, linear solvers, integrators, optimizers, differentiators, proof checkers, tensor backends, renderers, and simulation backends. A provider contract specifies result, certificate, error, budget, cancellation, and determinism behavior, not only a function signature.

Detailed operation signatures and refusal codes live in `language/stdlib/cells/`.
