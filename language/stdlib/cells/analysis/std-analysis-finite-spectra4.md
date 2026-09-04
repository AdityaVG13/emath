# `std.analysis.finite.spectra4`; exact 4x4 block-triangular spectra over `F_p`

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`int_rem`, `congruence`,
`pow_mod`): no core IR/op-enum change, no new binder kind. Composes
the 2x2/3x3 cofactor cells into the 4x4 block-triangular world; still
no general-n determinant primitive, so the claim stops exactly at the
unrolled 4x4 shape shown.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.spectra4` |
| Class | `pure` (4x4 block upper-triangular matrices over `F_p`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

For a 4x4 matrix `A` in 2x2 block upper-triangular form
`A = [[B, C], [0, D]]` (bottom-left 2x2 block zero), the full
characteristic polynomial is computed by the unrolled quartic
identities

```text
chi(t) = t^4 - tr t^3 + M2 t^2 - M3 t + det
```

- `tr` = trace;
- `M2` = sum of the SIX principal 2x2 minors;
- `M3` = sum of the FOUR principal 3x3 determinants;
- `det` = 4x4 determinant (row-0 cofactor expansion into 3x3 minors).

Every claim below is an exact integer identity reduced mod the prime
`p`; the eigenvalue decision is a finite sweep over the p candidates.

## Laws (exact, finite)

1. **Block factorization.** `chi_A(t) == chi_B(t) * chi_D(t)` in
   `F_p[t]`, coefficientwise: a block upper-triangular determinant is
   the product of the diagonal-block determinants. The cell verifies
   the factorization explicitly (multiply the two quadratic
   characteristic polynomials out and match all five coefficients).
2. **Spectrum union.** `spec(A) = spec(B) ∪ spec(D)` with multiplicities;
   decided by the sweep against `chi_A`.
3. **Vieta invariants (quartic).** `tr == sum(spec)`,
   `M2 == sum of pairwise products`, `M3 == sum of triple products`,
   `det == product`, all in `F_p` counted with multiplicity.
4. **Eigenvector certificate.** As in the 2x2/3x3 cells:
   `(A - lambda I) v == 0 (mod p)` componentwise, checked exactly per
   vector claim.

## Refusals (typed, never silent)

- Non-integer entries refuse at admission (type error, no rounding).
- `p` composite or `< 2`: field laws unclaimed (ring arithmetic only).
- General 4x4 NON-block matrices: the same quartic identities hold,
  but the factorization law does not; the cell states which law is
  being exercised rather than pretending universality.
- `n > 4` or non-block general-n: refused by contract; no float
  fallback.

## Surface spelling

```emath
emath function spectra4_fp7:
    inputs:
        p: Int
    outputs:
        tr: Int
        m2: Int
        m3: Int
        det: Int
        all_ok: Bool
    definitions:
        # A = [[2,1,5,0],[1,2,0,5],[0,0,3,1],[0,0,0,4]] over F_7
        tr = int_rem(2 + 2 + 3 + 4, p)
        # principal 2x2 minors, pairs (01)(02)(03)(12)(13)(23)
        m2 = int_rem(3 + 6 + 8 + 6 + 8 + 12, p)
        # principal 3x3 dets (Sarrus each)
        m3 = int_rem(24 + 24 + 12 + 9, p)
        det = int_rem(3 * 12, p)
        all_ok = (congruence(tr, 4, p) and congruence(m2, 1, p)
                  and congruence(m3, 6, p) and congruence(det, 1, p))
```

Runnable demonstration with the full F_7 sweep, block factorization
check, and an eigenvector certificate:
`language/examples/analysis/finite-spectra-4x4.emath`.

## Named fences (deliberately open)

- No general-n determinant/charpoly; no 5x5 (Sarrus expansion stops
  being honest by hand).
- No Jordan form / invariant subspace computation.
- No continuous spectral claims (the float `eigvals`/`singular_values`
  world is a different carrier with different honesty).
