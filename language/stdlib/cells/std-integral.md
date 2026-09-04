# `std.integral`; declared measure worlds and explicit-measure integration

Status: std-layer slice of the measures-transforms work
(B20 measure worlds + the B25 declared-kernel core), implemented in
`crates/emath-core/src/integral.rs`. The measure is a WORLD PARAMETER,
never ambient; Lebesgue and discrete are different worlds; exactness
only where declared.

## The measure is never ambient

A measure exists only through its constructor, and every
integrate/transform call takes it as an explicit argument; the std
mirror of the language's `wrt mu` rule (and of the MeaningHole refusal
for an ambient integral). Nothing infers, defaults, or remembers a
measure between calls.

## Two world types, no silent conversion

- `DiscreteMeasure`: masses on declared atoms; every atom finite,
  every mass finite and non-negative (`E-INTEGRAL-1` otherwise).
  Total mass is the declared sum.
- `LebesgueOn`: length measure on a declared non-degenerate finite
  interval (`E-INTEGRAL-2` otherwise).
- There is NO conversion impl between the two types: Lebesgue and
  discrete are different worlds. Riemann is deliberately absent;
  finite Riemann sums belong to the numeric-solver lanes, not to the
  measure vocabulary.

## Exactness where declared

- `integrate_discrete(f, &mu)`: exact atom sum `Σ f(x_j)·m_j`. A
  non-finite integrand value refuses, naming the atom. The zero
  measure is a legal world and integrates to zero.
- `integrate_step(&f, &mu)`: exact `Σ value·length` for a declared
  `StepFunction` over a declared Lebesgue domain.
- General measurable functions are NOT claimed: no quadrature, no
  silent numeric approximation.

## Coverage is part of the world contract

`StepFunction` cells must be finite, non-degenerate, and
non-overlapping (an overlap makes "the value at x" ambiguous;
refusal at construction). Gaps are legal on the OBJECT (it may be a
partial function) but `integrate_step` requires the cells to cover
the declared domain exactly: first cell starts at `lo`, last ends at
`hi`, no interior gaps. Short, gappy, or overhanging coverage refuses
typed (`E-INTEGRAL-3`); never a silent clip, never a silently-zero
gap.

## Declared kernels (B25 core, discrete world)

- Fourier: `(Fμ)(t) = Σ m_j·e^{-i t x_j}`; exact atom sum. The kernel
  SIGN is pinned by exact witnesses (mass at x=1, t=π/2 ⇒ exactly −i;
  symmetric pair at t=π ⇒ exactly −1). The Fourier kernel-sign mutant
  is killed by these witnesses.
- Laplace: `(Lμ)(s) = Σ m_j·e^{-s x_j}` for complex `s` (built on the
  signal layer's `Complex`). Witnesses: e^{-1/2}, e^{-(1+iπ)} = −e^{-1},
  and the sign witness e^{-(1+iπ/2)} = −i·e^{-1}.
- Finite-domain honesty: any kernel evaluation leaving the f64
  envelope refuses typed (`E-INTEGRAL-4`). Enforcement is single-point
  at the sum level; the mutation check proved a kernel-level guard
  redundant (inf/NaN poison the total identically), so the dead guard
  was removed rather than kept as untestable defense.

## Named fences (follow-up slices, deliberately open)

- **No `.emath` `wrt mu` surface and no transform binder kinds**;
  both are language design work (B25 NEEDS-DESIGN-WORK; binder kinds
  and kernel/domain world-carrying), gated on the notation-core lane
  (m5mt). This slice ships the substrate those binders will lower to.
- No continuous-kernel transforms over Lebesgue worlds (needs the
  integral design, not atom sums).
- No measure algebra (product measures, pushforward, density).
- No symbolic transform inversion.
- No Riemann measure (solver-lane concern, by design).
