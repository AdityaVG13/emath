# `std.analysis.finite.spectra`; matrix spectra over exact finite fields

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`int_rem`, `congruence`,
`pow_mod`, `mod_inv`): no core IR/op-enum change, no new binder kind.
This page is the human contract; nothing here claims a Rust
implementation beyond the integer kernels it composes.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.spectra` |
| Class | `pure` (fixed small-n square matrices over `F_p`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## Discrete/exact finite analogue, honestly scoped

Spectral theory over `C` needs limits and roots of arbitrary
polynomials; that is the numeric `eigvals` world (real symmetric,
Jacobi, `Float64`). This cell is the EXACT finite analogue: a square
matrix with integer entries viewed over the field `F_p` (p prime),
whose spectrum is a finite set that can be enumerated completely.

The carrier is deliberately the admitted scalar `Int` entries plus the
closed 2x2/3x3 determinant formulas; there is no general-n exact
determinant primitive in the surface yet (that is a named fence), so
the cell claims n <= 3 only and refuses to generalize silently.

## Carrier

- Matrix `A` given by its entries `a[i][j] : Int`, interpreted mod `p`.
- `p : Int` prime, `p >= 2`; a composite or non-positive `p` is
  outside the field world and every law below is vacuous there (callers
  must gate on a primality proof; the cell does not silently accept).

## Laws (exact, finite)

1. **Characteristic polynomial (2x2).** For
   `A = [[a, b], [c, d]]`, `chi(t) = t^2 - tr(A) t + det(A)` with
   `tr = (a + d) mod p`, `det = (ad - bc) mod p`, all arithmetic exact
   over the integers before reduction.
2. **Eigenvalue membership.** `lambda in F_p` is an eigenvalue of `A`
   iff `congruence(chi(lambda), 0, p)` holds. The spectrum is exactly
   `{lambda in F_p : chi(lambda) == 0 (mod p)}`; a complete sweep over
   the p candidates is finite and decisive.
3. **Eigenvector certificate.** `v = (v0, v1)` is a `lambda`-eigenvector
   iff `((A - lambda I) v) == 0 (mod p)` componentwise. The certificate
   is checked exactly; a nonzero residual refuses the claim.
4. **Trace/determinant invariants.** `tr(A) == sum(eigenvalues)` and
   `det(A) == product(eigenvalues)` in `F_p` (Vieta, counted with
   algebraic multiplicity from the factored `chi`).
5. **Reflection spectrum.** The swap matrix `S = [[0,1],[1,0]]` has
   `chi(t) = t^2 - 1`, so `spec(S) = {+1, -1}` in every `F_p` with
   `p > 2` (odd `p`: two distinct eigenvalues; `p = 2`: `+1 == -1`, a
   single eigenvalue of multiplicity 2).

## Refusals (typed, never silent)

- Entry arithmetic is exact integer arithmetic; an entry that is not an
  integer literal/expr is a type error at admission, not a rounding.
- `p` composite or `< 2`: no law is claimed; results computed anyway
  are ring arithmetic, NOT spectra (no field, no Vieta guarantee). The
  cell's contract names this boundary rather than hiding it.
- General `n > 3`: refused by the contract (no determinant primitive
  claimed); no silent fallback to float `eigvals`.

## Surface spelling

```emath
emath function swap_spectrum_fp5:
    inputs:
        p: Int
    outputs:
        chi_plus: Bool
        chi_minus: Bool
    definitions:
        # S = [[0,1],[1,0]] over F_p; chi(t) = t^2 - 1
        chi_plus  = congruence(1 * 1 - 1, 0, p)
        chi_minus = congruence((-1) * (-1) - 1, 0, p)
```

Runnable demonstration with explicit invariant checks:
`language/examples/analysis/finite-spectra-fp.emath`.

## Named fences (deliberately open)

- No general-n exact determinant/charpoly primitive in the stdlib
  surface; the generic exact-integer nullspace primitive
  (`int_nullspace`) is the right seam for a LeVerrier-style charpoly
  later, but that is new admit work, not this cell.
- No complex/real exact spectra (requires algebraic-number carriers).
- No eigenvector BASIS computation (one certificate vector per claim,
  not a full diagonalization).
