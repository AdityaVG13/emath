# emath Standard Library Plan

The standard library is split into semantic contract packages and implementation/provider packages. See `language/reference/standard-library-constitution.md`.

## Executable object packs

Since the stdlib is stored as
executable object packs, not catalog markdown: the `std.core` census;
theory, cells, and evidence receipts; is exported as an `.emlib` pack
where every object carries its own admitted MeaningID and canonical
semantic payload. `emath library mount std` composes and mounts the
census through the store's typed mount (every object id and evidence
hash re-verified; forgery refuses `E-EVID-503` / `E-STD-002`), and any
second workspace mounts the same pack with zero source duplication.
The markdown cells below remain the human contracts; object packs are
the executable truth.

## Phase 1 core (implemented as compiler builtins)

These names compute today. They live in the compiler builtin table
(`BuiltinId` / exec emitter / `emath-rt` kernels), not as `.emath`
package sources. Call them bare or with a namespace (`sin(x)`,
`math::sin(x)`, `core::math::sin(x)`).

- scalar types that compute: `Float64`, `Bool`, `Nat`, `Int`, `Complex`;
- arithmetic operators `+ - * / ^` and function duals
  `add` `sub` `mul` `div` `neg` `pow` (same meaning as the operators);
- elementary functions: `exp` `ln` `log` `sqrt` `sin` `cos` `tan` `tanh`
  `sinh` `cosh` `atan` `atan2` `abs` `floor` `ceil` `round` `sign`
  `log2` `log10` `cbrt` `recip` `fract` `min` `max` `hypot` `mod`
  `is_finite` `lerp` `clamp` — Float64 contracts: `abs`/`min`/`max`
  piecewise; `sqrt` carries the `SqrtNonNegative` domain obligation
  (NaN value on a negative operand); `atan2` is the IEEE two-argument
  arctangent; `sign` is mathematical sgn with `0` at zero; all admit
  end-to-end in the strict-f64 subset since emath-s9w1m + fpl60;
- linear algebra: `dot` `norm` `transpose` `length` `mean` `einsum`;
  `length` is the only size query; the `len` alias is removed;
- integer kernels in `emath-rt`: `factorial` `mod_inv` `congruence`
  `poly_eval_mod` `rs_encode` `hamming_distance`.

`Option`, `Result`, records, and variants parse as types but are not
compute types (`E-TYPE-010`). See `language/CAPABILITY.md`.

## Phase 2–5 core

- exact integers/rationals and numeric profiles;
- units/dimensions;
- shapes/tensors;
- intervals and domains;
- graph and state-machine contracts;
- calculus/optimization goal contracts.

## Native symbolic slice

`core::symbolic` exposes provider-neutral expression/rewrite contracts,
structural simplification, and an exact univariate polynomial-identity
decision procedure (degree at most 64). Unsupported Gröbner, CAD, quantified,
or transcendental claims refuse by name rather than falling through to a
numeric guess. A future Wrenfold-class adapter implements the same
`SymbolicOracleContract`.

## Provider packages

- broader symbolic simplification/differentiation;
- root/integration/ODE/optimization;
- tensor/AD backends;
- Modelica/Rumoca structural simulation;
- theorem/proof checkers;
- interval/certified numerics;
- hardware and remote execution.

## Curated known mathematics

Executable named laws and honest deferrals are indexed in
[`laws/INDEX.md`](laws/INDEX.md). Import-only files can resolve selected
embedded symbols, for example:

```emath
use physics::classical::{NewtonSecond, Hooke}
```

## Capability cell contracts

Capability cells (schema `emath.capability-cell.v1`) are stdlib surface
data, not parser keywords or core IR variants. First authoring contract:
[`cells/std-tensor-softmax.md`](cells/std-tensor-softmax.md);
`std.tensor.softmax` pure cell (stable-max strict-f64, laws, provider
seam, zero-core-delta rules). World-class contract:
[`cells/std-finite-sets.md`](cells/std-finite-sets.md); `std.finite.sets`
finite-subset carrier (set literal, comprehension, membership `v in s`;
laws extensionality, comprehension membership, finite enumeration;
evaluation refused `E-TYPE-113` until emath-ir Phase B).
Contract slice:
[`cells/std-text-report.md`](cells/std-text-report.md); `std.text.report`
`core::text`/`core::report` Phase-B contracts (U8 interpolation purity
fences landed; string VALUES refuse `E_UNSUPPORTED_TYPE` until the
Phase-B carrier; deterministic evidence-grade emitters specified).

Wave-16 cell contracts (all capability-cell data, zero core delta):

- [`cells/std-fp-polynomial.md`](cells/std-fp-polynomial.md) — `std.finite_field.polynomial` (polynomial algebra over F_p);
- [`cells/std-fp-group.md`](cells/std-fp-group.md) — `std.finite_field.group` (the multiplicative group F_p^*);
- [`cells/std-probability-markov.md`](cells/std-probability-markov.md) — `std.probability.markov`;
- [`cells/std-probability-montecarlo.md`](cells/std-probability-montecarlo.md) — `std.probability.montecarlo`;
- [`cells/std-probability-bayes.md`](cells/std-probability-bayes.md) — `std.probability.bayes`;
- [`cells/analysis/std-analysis-finite-spectra.md`](cells/analysis/std-analysis-finite-spectra.md) — `std.analysis.finite.spectra`;
- [`cells/analysis/std-analysis-finite-fourier.md`](cells/analysis/std-analysis-finite-fourier.md) — `std.analysis.finite.fourier`;
- [`cells/analysis/std-analysis-finite-opnorms.md`](cells/analysis/std-analysis-finite-opnorms.md) — `std.analysis.finite.opnorms`;
- [`cells/analysis/std-analysis-finite-shift.md`](cells/analysis/std-analysis-finite-shift.md) — `std.analysis.finite.shift`;
- [`cells/std-geometry-affine.md`](cells/std-geometry-affine.md) — `std.geometry.affine`;
- [`cells/std-geometry-metric.md`](cells/std-geometry-metric.md) — `std.geometry.metric`;
- [`cells/std-geometry-spherical.md`](cells/std-geometry-spherical.md) — `std.geometry.spherical`;
- [`cells/std-geometry-projective.md`](cells/std-geometry-projective.md) — `std.geometry.projective`;
- [`cells/geometry/std-world-metric-space.md`](cells/geometry/std-world-metric-space.md) — `std.geometry.world.metric-space` (world cells: carrier + law witnesses);
- [`cells/geometry/std-world-inner-product.md`](cells/geometry/std-world-inner-product.md) — `std.geometry.world.inner-product`;
- [`cells/geometry/std-world-affine.md`](cells/geometry/std-world-affine.md) — `std.geometry.world.affine`.
