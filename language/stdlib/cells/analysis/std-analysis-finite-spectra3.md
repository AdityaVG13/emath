# `std.analysis.finite.spectra3`; exact 3x3 matrix spectra over `F_p` by cofactor expansion

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`int_rem`, `congruence`,
`pow_mod`): no core IR/op-enum change, no new binder kind. Extends
`std.analysis.finite.spectra` (2x2) to 3x3 by hand-expanded cofactor
identities; still no general-n determinant primitive, so the claim
stops exactly at n = 3.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.spectra3` |
| Class | `pure` (3x3 matrices over `F_p`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

For `A` a 3x3 integer matrix read in `F_p`, write

```text
chi(t) = t^3 - tr t^2 + M t - det
```

with the three invariants computed by hand-expanded 2x2 cofactors:

- `tr = a00 + a11 + a22`;
- `M = det A[0,1 | 0,1] + det A[0,2 | 0,2] + det A[1,2 | 1,2]`
  (the sum of the three PRINCIPAL 2x2 minors);
- `det = a00 (a11 a22 - a12 a21) - a01 (a10 a22 - a12 a20)
         + a02 (a10 a21 - a11 a20)` (cofactor expansion along row 0).

Every claim below is an exact integer identity reduced mod the prime
`p`; the eigenvalue decision is a finite sweep over the p candidates.

## Laws (exact, finite)

1. **Eigenvalue membership.** `lambda in F_p` is an eigenvalue iff
   `congruence(lambda^3 - tr lambda^2 + M lambda - det, 0, p)`; the
   spectrum is the full root set of `chi` in `F_p`, decided by the
   finite sweep.
2. **Vieta invariants.** With eigenvalues counted by multiplicity:
   `tr == sum(spec)`, `M == sum of pairwise products`, `det == product`
   in `F_p`. These are checked exactly; a mismatch refuses the cell's
   own arithmetic.
3. **Eigenvector certificate.** `v` is a `lambda`-eigenvector iff
   `(A - lambda I) v == 0 (mod p)` componentwise (equivalently
   `A v == lambda v`); checked exactly, per vector claim.
4. **Triangular shortcut.** A triangular `A` has `spec =` its diagonal;
   the sweep must confirm exactly the diagonal values (a useful
   mutation check on the cofactor arithmetic).

## Refusals (typed, never silent)

- Non-integer entries refuse at admission (type error, no rounding).
- `p` composite or `< 2`: field laws unclaimed (ring arithmetic only).
- `n > 3`: refused by contract; no silent fallback to float `eigvals`
  and no LeVerrier claim without the general-n determinant primitive.

## Surface spelling

```emath
emath function spectra3_fp7:
    inputs:
        p: Int
    outputs:
        tr: Int
        mm: Int
        det: Int
        all_ok: Bool
    definitions:
        # A = [[2,1,0],[1,2,1],[0,1,2]] over F_7:
        # chi(t) = t^3 + t^2 + 3t + 3, spec = {2, 5, 6}
        tr = int_rem(2 + 2 + 2, p)
        mm = int_rem((2 * 2 - 1 * 1) + (2 * 2 - 0 * 0) + (2 * 2 - 1 * 1), p)
        det = int_rem(2 * (2 * 2 - 1 * 1) - 1 * (1 * 2 - 1 * 0)
                      + 0 * (1 * 1 - 2 * 0), p)
        root2 = congruence(2 * 2 * 2 - 6 * 2 * 2 + 10 * 2 - 4, 0, p)
        all_ok = (root2 and congruence(tr, 6, p) and congruence(mm, 3, p)
                  and congruence(det, 4, p))
```

Runnable demonstration with the full 7-point sweep, Vieta checks, and
an eigenvector certificate:
`language/examples/analysis/finite-spectra-3x3.emath`.

## Named fences (deliberately open)

- No general-n exact determinant/charpoly (`int_nullspace`-based
  LeVerrier is the future seam; new admit work, not this cell).
- No eigenvector BASIS (one certificate vector per claim).
- No complex/real exact spectra (algebraic-number carrier fence from
  the 2x2 cell stands).
