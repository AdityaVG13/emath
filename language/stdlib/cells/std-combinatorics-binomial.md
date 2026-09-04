# `std.combinatorics.binomial`; exact factorial-form binomial tables

Status: **law pack + example landed** (Wave 16, discrete mathematics
and combinatorics, selective; probability-lane leverage). The binomial
coefficient by its exact factorial form, Pascal's identity, and
symmetry, evaluated in exact i64 integer arithmetic on the admitted
`factorial` kernel. No asymptotic or overflow claim beyond the exact
fold range.

## Contract (small exact Int, i64 fold carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `BinomialFactorialForm` | law, `n, k → binomial` | `C(n,k) = n!/(k!(n−k)!)` by the exact integer fold | `0 <= k <= n <= 20`; every quotient is an exact integer at the test values |
| `PascalIdentity` | law, `n, k → pascal_check` | `C(n,k) = C(n−1,k−1) + C(n−1,k)` as an inspectable equality | small exact n; the equality is computed, not asserted |
| `BinomialSymmetry` | law, `n, k → symmetry_check` | `C(n,k) = C(n,n−k)` | same admission box |

## Executable surface

- Laws: `language/stdlib/laws/combinatorics-binomial.emath`
  (package `combinatorics.binomial.laws`): `BinomialFactorialForm`,
  `PascalIdentity`, `BinomialSymmetry`.
- Example: `language/examples/discrete/catalan_tables.emath`
  (`BinomialRowSumPascal`, `BinomialModPrime`): row five sums to
  `2^5 = 32`; the finite-field slice checks `C(10,3) ≡ 1 (mod 7)`
  through the congruence kernel (`mod` itself is fenced in strict-f64,
  tracked by emath-s9w1m).

## No-claim boundaries

- Exactness claim is bounded by the i64 fold range (n ≤ 20); no big-int
  binomial is claimed.
- No asymptotic growth statement, no floating approximation claim:
  values are exact integers.
- Generating-function and asymptotic enumeration belong to the
  analytic-combinatorics world, not this cell.
