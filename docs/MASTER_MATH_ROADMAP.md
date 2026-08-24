# Master Math Roadmap

> Consolidated from Grok 4 synthesis (2026-08-24). Living document —
> update as capabilities land. Aligns with AGENTS.md Stage 1 priority:
> mathematical engine first.

## Implemented (as of Aug 2026)

Authoritative source: `language/CAPABILITY.md`.

- **Declarations:** function, policy, model (all compute); kind (schema
  only); custom (function or refuses)
- **Types:** Float64, Bool, Nat, Int (i64), Complex, GF<p>,
  Vector[n], Matrix[r,c], Tensor[...], Interval<F>, NonNegative<R>,
  Positive<R>, Probability<R>, units, generics at use sites
- **Expressions:** arithmetic, comparisons, logic (and/or/not/==>/<==>,
  binders (sum, product, integral/Simpson, forall, exists with guards),
  einsum, complex literals + arithmetic, unit/dimension queries
- **Autodiff:** forward-mode (derivative, partial, total, Unicode ∂)
- **Solving:** Newton root-finding (solve), gradient descent/ascent
  (minimize/maximize) with constraint penalty method
- **Simulation:** Euler, RK4, RK45 adaptive, event detection,
  DAE causalization, vector/matrix state
- **PDE:** 1D/2D laplacian stencils (Clamp, Dirichlet, Neumann),
  gradient fields
- **Modular arithmetic:** factorial, mod_inv, congruence, poly_eval_mod
  (Horner GF(p)), rs_encode (Reed-Solomon), hamming_distance
- **Goals:** evaluate → Rust library; differentiate; optimize
- **Pipeline:** source → sema → EMIR → interp or Rust codegen →
  verified Cargo artifact. WASM playground, CLI, evidence model.

## Future Master List — Domains to Implement

### 1. Foundations
Propositional/predicate/modal/temporal/intuitionistic/linear logics.
Full quantifiers. Set theory, Boolean algebras, relations, orders,
lattices. Type theory (simple → dependent → HoTT). Category theory
(categories, functors, adjunctions, limits/colimits, monads, toposes,
higher categories). Synthetic differential geometry.

### 2. Numbers & Arithmetic
Rat, Real (exact/approx), p-adics, hyperreals, surreals, dual numbers,
quaternions, octonions, Clifford/geometric algebras. Arbitrary
precision. Interval/affine/Taylor-model validated arithmetic. Advanced
modular/CRT/finite-field extensions.

### 3. Algebra
Full BLAS-like linear algebra (eigenvalues, SVD, QR/LU/Cholesky, sparse,
structured, Kronecker). Multilinear/tensor algebra (CP/Tucker
decompositions, tensor networks). Abstract algebra (groups, rings,
fields, modules, Lie/Hopf algebras). Commutative algebra (ideals,
Gröbner bases, primary decomposition). Representation theory, Galois
theory. Algebraic geometry (varieties, schemes, sheaves). Coding theory
expansion. Lattice-based crypto primitives.

### 4. Analysis & Calculus
Limits, continuity, series (power, Laurent, asymptotic, Fourier,
generating). Reverse-mode AD, higher-order, mixed derivatives. Symbolic
+ advanced numeric integration (adaptive, Monte-Carlo, sparse grids,
quasi-MC). Special functions (gamma, beta, Bessel, hypergeometric,
elliptic, zeta, Airy). Complex analysis (contour integrals, residues).
Functional analysis, operators, spectra. Measure theory & distributions.
Variational calculus.

### 5. Differential Equations & Dynamical Systems
Stiff/adaptive/symplectic/high-order/delay ODE/DAE. PDE
(elliptic/parabolic/hyperbolic, nonlinear; FDM/FVM/FEM/spectral/
method-of-lines/PINNs). SDE/SPDE. Dynamical systems (stability,
bifurcation, chaos, Lyapunov, attractors, ergodic theory). Control
(LQR, MPC, optimal control, hybrid systems, Lyapunov functions).

### 6. Optimization & Search
Newton, quasi-Newton, interior-point, SQP. Global (Bayesian, genetic,
branch-and-bound). Convex (LP/QP/SDP/SOCP/geometric). Multi-objective/
Pareto. Integer/Mixed-integer/combinatorial. Certificates, duality,
sensitivity. Robust/stochastic/bilevel. Manifold optimization.

### 7. Probability, Statistics & Information
Measure-theoretic probability, distributions, sampling, densities.
Stochastic processes (Markov, Poisson, Brownian, Lévy). Bayesian
inference, MCMC, variational inference. Statistical estimation,
hypothesis testing, causal inference/SCMs. Information theory (entropy,
KL, channels, coding theorems). Random matrices, concentration
inequalities, UQ/polynomial chaos.

### 8. Geometry, Topology & Manifolds
Computational geometry (hulls, Voronoi, meshes, discrete differential
geometry). Differential geometry (manifolds, Riemannian/Lorentzian/
symplectic/contact, geodesics, curvature, connections). Algebraic
topology (homology/cohomology, persistent homology/TDA, homotopy groups,
K-theory). Knot theory, geometric group theory. Projective/affine/
conformal/non-Euclidean geometries.

### 9. Discrete Math & Combinatorics
Graph theory (paths, flows, matching, spectral, random graphs,
generation). Enumerative/extremal combinatorics, generating functions,
combinatorial species, designs, matroids, hypergraphs. Additive
combinatorics, Ramsey theory. Formal languages, automata, term-rewriting.

### 10. Number Theory & Cryptography
Elementary/analytic (L-functions, modular forms, zeta zeros)/algebraic
(class field, elliptic curves). Computational NT (primality, factoring,
discrete log, LLL). Crypto math (RSA/ECC, post-quantum, ZK, MPC).
Advanced coding theory.

### 11. Numerical Analysis & Scientific Computing
Approximation theory, interpolation, quadrature. Large-scale
linear/nonlinear solvers, eigensolvers, sparse iterative. FFT and
integral transforms. Monte-Carlo/quasi-MC/multilevel. Parallel/GPU/
distributed (via providers). Certified/rigorous numerics with proofs.

### 12. Applied & Domain-Specific
Classical/continuum/quantum mechanics, relativity, field theories,
fluids, EM, thermodynamics. Chemistry (reaction networks, quantum
chemistry). Biology/systems biology/population dynamics/neuroscience.
Finance (stochastic calculus, risk, derivatives). Engineering
(structural, circuits, control, signal processing). ML/AI math (kernels,
neural nets as operators, geometric deep learning, equivariance,
attention-as-algebra). Quantum information/computation.

### 13. Meta, Frontier & Invented Mathematics
Full symbolic CAS (simplify, rewrite, pattern matching, Gröbner, CAD).
Deep formal verification/theorem proving (FrankenLean). Experimental
mathematics (conjecture generation, counter-example search, OEIS-style).
Generative/AI-assisted discovery loops. Higher category theory, derived
geometry, non-classical foundations. Multi-physics coupling, digital-twin
mathematics. Evidence hierarchies from "tested" to "proved".

## Laws, Theorems & Compositional Invention

### Design: `emath law` declaration kind

Sugar over existing function/policy/model machinery plus rich metadata
(assumptions, domain, evidence level, provenance, citations). Allows:

```emath
emath law NewtonSecond:
    about:
        statement: "F = m a"
        domain: classical_mechanics
        assumptions: [inertial_frame, non_relativistic, constant_mass]
        evidence: certified
        provenance: "Isaac Newton, Principia 1687"
    inputs:
        m: Positive<Mass>
        a: Acceleration
    outputs:
        F: Force
    definitions:
        F = m * a
    goals: evaluate, differentiate
```

### Package system for known mathematics

Core language provides mechanisms only (declare, import, specialize,
compose, check units/domains/evidence). Content lives in packages
(`core::physics`, `core::cs`, `core::stats`, etc.) — ordinary .emath
source or pre-compiled artifacts, versioned, community-contributable.

```emath
use physics::NewtonSecond
use cs::Amdahl
F = NewtonSecond(m = 2.0 kg, a = 3.0 m/s^2)
```

### Composition / mash-up

Free recombination of loaded laws under evidence + domain checks.
Provenance tracked; evidence level = weakest of parts. Dimensional or
assumption inconsistency → named refusal, not silent NaN.

```emath
emath combine NewtonSecond with IdealGas under isothermal constraint
emath specialize EinsteinField to weak-field + slow-motion limit
emath mutate FourierHeat by replacing laplacian with fractional laplacian
```

### Starter set of named laws

Classical mechanics: Newton's laws, gravitation, conservation laws,
Hooke's law, ideal gas, Navier-Stokes, Fourier heat, Bernoulli, Archimedes.
Relativity: E=mc², Lorentz, Einstein field equations, equivalence
principle, Schwarzschild metric. CS: Amdahl, Gustafson, Little's law,
CAP, Master theorem, P vs NP. Probability/stats: CLT, LLN, Bayes,
Shannon coding theorems. Analysis: Fundamental theorem of calculus, IVT,
MVT, Taylor, Fourier inversion, Stokes/Green/Divergence, Banach
fixed-point, Picard-Lindelöf. Algebra/NT: FTA, FTA, Fermat's little,
CRT, quadratic reciprocity, Fermat's last, Riemann hypothesis, twin
prime/Goldbach conjectures. Optimization/control: Noether, Pontryagin,
Lyapunov, KKT, Bellman. Other: Pythagorean, binomial, Cayley-Hamilton,
spectral theorem, Maxwell, Schrödinger, Black-Scholes.

## Mathematical Object Arena (MOA)

### Problem
Invoke known laws in <1ms. General archive + decompressor + parser on
every call is too slow.

### Design
Purpose-built, emath-native arena: content-addressed, arena-allocated,
interned, bit-packed IR forest already in lowered form. No general
decompression or parsing on the invocation path.

**Source of truth** (cold, versioned, compressed .emath law files)
→ **Offline builder** (emath in catalog mode)
→ **Pre-lowered binary IR + metadata index** (memory-mappable)
→ **In-process/WASM resident cache**
→ **Ready-to-use semantic object** (units, evidence, assumptions attached)

### Arena layout
- **Header:** magic, version, perfect-hash seed, object count, arena base
- **Name index:** perfect-hash/open-addressed table → (offset, length,
  evidence-bits, domain-bits, assumption-bitset, unit-signature)
- **Arena body:** interned strings/types/units/constants, pre-lowered IR
  nodes, shared sub-DAGs, bit-packed metadata

### Performance targets
- Hot path (cached/mapped): name → IR pointer < 50μs, argument binding +
  unit/domain check < 200μs, total < 500μs native
- Warm path (on disk, mapped): < 800μs
- Cold path (source only, first time): few ms acceptable (once per process)
- WASM: hottest 30-50 laws baked into data section; larger arena as
  companion blob in linear memory

### Key properties
Arena-relative offsets → relocatable, mmap-friendly, WASM-friendly.
Common patterns (Positive, SI units, "inertial_frame") interned once.
Cache invalidation content-addressed (hash of source → IR). Every loaded
law carries evidence level, assumptions, domain, provenance.

## Prioritization Heuristic

Impact on (a) production Rust software, (b) teaching/exploration loop,
(c) open-problem/generative research, weighted by provider availability
and current CAPABILITY surface.

### Near-term (Stage 1 aligned)
1. Reverse-mode AD + higher-order derivatives
2. Special functions library (gamma, beta, Bessel, etc.)
3. Richer linear algebra (eigenvalues, SVD, QR/LU/Cholesky)
4. Stiff/adaptive/symplectic ODE solvers
5. PDE methods beyond laplacians (FEM, spectral, method-of-lines)
6. Probability distributions + sampling
7. Graph algorithms
8. Advanced optimization (Newton, interior-point, SQP)

### Mid-term
9. `emath law` declaration kind + package system
10. Provider adapters (Wrenfold, FrankenLean)
11. Events/transitions in models
12. Rat/arbitrary-precision/validated intervals
13. Named laws library (curated starter set)

### Horizon
14. Mathematical Object Arena (MOA)
15. Composition/mash-up with provenance
16. Full symbolic CAS
17. Generative/AI-assisted discovery loops
18. Multi-backend execution (JAX, SciPy, GPU)
19. Visualization (plots, manifolds, phase portraits)
20. Community package ecosystem
