# emath Standard Library Plan

The standard library is split into semantic contract packages and implementation/provider packages. See `language/reference/standard-library-constitution.md`.

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
  `is_finite` `lerp` `clamp`;
- linear algebra: `dot` `norm` `transpose` `length` `mean` `einsum`;
  `length` is the only size query — the `len` alias is removed;
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
