# Curated Law Packages

Every executable entry is an `emath law` with assumptions, domain,
provenance, citations, evidence, typed inputs/outputs, and a canonical example.
“Deferred” means no numeric answer is fabricated.

## `physics::classical`

Source: [`physics-classical.emath`](physics-classical.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Newton's laws | `NewtonSecond` | Computes, algebraic constant-mass slice |
| Gravitation | `UniversalGravitation` | Computes, Newtonian point-mass slice |
| Conservation laws | `LinearMomentum`, `KineticEnergy` | Compute scalar definitions |
| Hooke's law | `Hooke` | Computes, linear elastic slice |
| Ideal gas | — | Deferred until thermodynamic state constraints are executable |
| Navier–Stokes | — | Deferred until vector differential operators and pressure constraints compose |
| Fourier heat | — | Deferred here; executable PDE examples live under `language/examples/numerical/` |
| Bernoulli | — | Deferred until fluid streamline assumptions are executable |
| Archimedes | — | Deferred until displaced-volume geometry is typed |

## `physics::relativity`

Source: [`physics-relativity.emath`](physics-relativity.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Mass-energy equivalence | `MassEnergyEquivalence` | Computes with typed SI quantities |
| Lorentz transformations | — | Deferred until bounded velocity assumptions are enforced at invocation |
| Einstein field equations | — | Refused by the current absence of metric/curvature types |
| Equivalence principle | — | Statement-only until frame/world mappings are executable |
| Schwarzschild metric | — | Deferred until manifold coordinate singularities are typed |

## `cs::laws`

Source: [`computer-science.emath`](computer-science.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Amdahl's law | `AmdahlSpeedup` | Computes fixed-workload speedup |
| Gustafson's law | `GustafsonSpeedup` | Computes scaled-workload speedup |
| Little's law | `Little` | Computes stable-queue average relation |
| CAP theorem | — | Check-only statement; no fake scalar answer |
| Master theorem | — | Deferred until recurrence-shape predicates are executable |
| P versus NP | — | Open problem; evaluation is not exported |

## `probability::laws`

Source: [`probability-statistics.emath`](probability-statistics.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Bayes' theorem | `BayesPosterior` | Computes a finite-event update |
| Central limit theorem | `CltStandardError` | Computes the declared iid finite-variance scaling, not a convergence certificate |
| Shannon coding theorems | `BinarySelfInformationContribution` | Computes one finite binary entropy term; no channel-capacity theorem claimed |
| Law of large numbers | — | Deferred until independence and convergence evidence are executable |

## `analysis::laws`

Source: [`analysis.emath`](analysis.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Fundamental theorem of calculus | `FundamentalTheoremEvaluation` | Computes endpoint evaluation for a supplied antiderivative |
| Taylor's theorem | `TaylorQuadratic` | Computes the second-order polynomial, no remainder bound claimed |
| Banach fixed-point theorem | `BanachContractionStep` | Computes one declared contraction step |
| IVT / MVT | — | Deferred until continuity certificates are executable |
| Fourier inversion | — | Deferred until measure and transform normalization are typed |
| Stokes / Green / Divergence | — | Deferred until differential forms and oriented domains land |
| Picard–Lindelöf | — | Deferred until Lipschitz certificates connect to ODE construction |

## `number_theory::laws`

Source: [`algebra-number-theory.emath`](algebra-number-theory.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| Fermat's little theorem | `FermatLittleModThree` | Exact p=3 specialization |
| Chinese remainder theorem | `ChineseRemainderWitness` | Checks a two-modulus witness |
| Modular construction | `ModularInverse` | Constructs an inverse for coprime inputs |
| Fundamental theorem of arithmetic | — | Deferred until factorization certificates are first-class |
| Fundamental theorem of algebra | — | Deferred until complex polynomial root certificates land |
| Quadratic reciprocity | — | Deferred until primality assumptions are executable |
| Fermat's last theorem | — | Proved statement, not exported as a numeric solver |
| Riemann hypothesis | — | Open conjecture; no decision export |
| Twin-prime / Goldbach | — | Open conjectures; no witness fabrication |

## `optimization_control::laws`

Source: [`optimization-control.emath`](optimization-control.emath)

| Roadmap name | Package symbol | Status |
|---|---|---|
| KKT conditions | `KktStationarityResidual` | Computes a scalar stationarity residual |
| Bellman optimality | `BellmanTwoActionBackup` | Computes a finite two-action backup |
| Lyapunov stability | `LyapunovLinearDerivative` | Computes a scalar linear-system derivative |
| Noether's theorem | — | Deferred until Lagrangian symmetry certificates land |
| Pontryagin maximum principle | — | Deferred until control trajectories and costates compose |

## Other roadmap starters

These names remain visible rather than being silently dropped:

| Name | Status |
|---|---|
| Pythagorean theorem, binomial theorem | Deferred to a geometry/algebra follow-on pack |
| Cayley–Hamilton, spectral theorem | Deferred to certified linear-algebra packs |
| Maxwell equations, Schrödinger equation | Deferred to field/complex PDE packages |
| Black–Scholes | Deferred to stochastic-calculus and finance-domain packages |
