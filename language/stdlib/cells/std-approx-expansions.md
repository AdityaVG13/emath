# `std.approximation.laws`; stdlib slice contract

Status: std-layer laws package (bead `emath-r3-approx-std-lib-lzu1`).
The approximation machinery itself is language surface: the `≈`/`~=`
operator, `within rtol/atol` tolerance clauses, and claim-context
admission are compiler features (bead `emath-r3-approx-operator-depc`,
CLOSED). This package is authored mathematics on top of that surface;
no core IR enum variants, no parser keywords.

## Scope

Three executable approximation laws, each pairing a computable expansion
with a DECLARED regime that the compiler enforces as admission-constraint
IR (`require` lines in `assumptions:` lower via `lower_requirement` into
`declaration.invariants`; the same seam as `invariant:`).

| Law | Expansion | Declared regime (enforced) |
|---|---|---|
| `TaylorQuadraticRegime` | `f + f'·δ + f''·δ²/2` | `abs(delta) < convergence_radius` |
| `ChebyshevThreeTerm` | `c0 + c1·x + c2·(2x²−1)` | `abs(x) <= 1` |
| `PadeTwoOne` | `(a0 + a1·x + a2·x²)/(1 + b1·x)` | `b1·x != −1` |

## Honesty surface (E1)

Every law's evidence claim is level `E1` and states what is computed
versus what is only declared: the polynomial/rational evaluation is real,
the regime (convergence radius, equioscillation domain, pole avoidance)
is DECLARED by the author, and no remainder bound is fabricated. This is
the approximation-theory face of the honesty standard: an approximation
without a declared regime is exactly the lie the surface forbids.

## Regime enforcement is real, not prose

- The `require` lines are constraint IR: stripping them removes exactly
  one invariant per law (tested), and mutating the constraint (`<` →
  `<=`) changes the canonical package `ContentId` (identity law).
- Regime VALUES in an example (`given convergence_radius = 1`) are
  runtime demo data, not identity; the same law with a different demo
  input is the same admitted meaning.
- Violations refuse at RUN with a typed refusal verdict (Chebyshev at
  `x = 2` refuses; the other two examples still pass), never silently
  compute.

## Reference semantics (strict-f64)

- Taylor: the exact quadratic polynomial; honest because the regime is
  declared, not because the remainder is bounded by the compiler.
- Chebyshev: three-term basis evaluation using `T2(x) = 2x² − 1`
  (recursion unrolled, no trig); domain `[-1, 1]` is the equioscillation
  regime from Chebyshev's minimax theory.
- Padé (2,1): the rational form `(a0 + a1·x + a2·x²)/(1 + b1·x)`; the
  denominator constraint is the declared pole-avoidance assumption.

## No-claim boundaries

- No coefficient computation (fitting/interpolation); the laws evaluate
  SUPPLIED coefficients. Computing coefficients from data is a follow-up
  slice.
- No world-parameterized coefficient types (units-carried expansions);
  coefficients are plain `Float64`.
- No remainder bounds, error estimators, or adaptive order selection.
- No `≈`-operator integration inside the laws; the operator surface is
  exercised by `depc` tests (`tests/emath-sema/tests/approx_operator.rs`);
  these laws use explicit formulas so the regime enforcement, not the
  tolerance machinery, is what the tests pin.

## Verification

`cargo test -p emath-sema-tests --test approx_stdlib` (6 tests): package
admits and runs (3/3 examples), expansion values (2.25, −0.5,
2.1666666666666665), E1 honesty labels, identity mutation, constraint
stripping, runtime domain refusal. Mutation-checked: lowering the
`assumptions:` `require` lines away kills 3 tests.
