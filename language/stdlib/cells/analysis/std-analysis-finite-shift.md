# `std.analysis.finite.shift`; the cyclic shift operator on exact finite sequence spaces

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`pow_mod`, `int_rem`,
`congruence`): no core IR/op-enum change, no new binder kind. This is
the finite-dimensional OPERATOR-theory slice of the advanced-analysis
pack: a concrete linear operator whose spectrum is computed exactly,
closing the loop with the spectra and Fourier cells.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.shift` |
| Class | `pure` (cyclic shift on `F_p^N`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

Fix a prime `p` and length `N` with `N | p - 1`. The cyclic shift on
the sequence space `F_p^N` is the linear operator

```text
S(x0, x1, ..., x[N-1]) = (x1, x2, ..., x[N-1], x0)
```

(indices mod N). As a matrix it is the permutation with ones on the
superdiagonal and one in the bottom-left corner. Every claim below is
an exact finite identity checked through unrolled component arithmetic
and `congruence`.

## Laws (exact, finite)

1. **Order.** `S^N == I` and no smaller positive power is `I` (when the
   components are free): the shift has multiplicative order exactly
   `N`, the operator-level mirror of the principal root `g` in the
   Fourier cell.
2. **Spectrum.** `spec(S) = {lambda in F_p : lambda^N == 1}`; when
   `N | p - 1` this is exactly `N` distinct values `g^0, ..., g^{N-1}`
   for any principal root `g`. The eigenvalue test is
   `congruence(pow_mod(lambda, N, p), 1, p)` — complete and finite.
3. **Fourier eigenvectors.** The vector `v_j = (g^{0·j}, g^{1·j}, ...,
   g^{(N-1)·j})` is an eigenvector of `S` with eigenvalue `g^{-j}`:
   `S v_j == g^{N-j} v_j` componentwise. The Fourier modes diagonalize
   the shift; this is the exact finite analogue of "the DFT
   diagonalizes the cyclic shift operator".
4. **Charpoly (N = 3 shape).** For `N = 3` the characteristic
   polynomial is `t^3 - 1` (permutation matrix, direct Sarrus
   expansion), so the spectrum is the cube roots of unity in `F_p`.

## Refusals (typed, never silent)

- `N` not dividing `p - 1`: fewer than `N` eigenvalues exist in `F_p`;
  the "N distinct eigenvalues" claim is NOT made (the ring statement
  `lambda^N == 1` still decides membership exactly).
- `p` composite: field laws unclaimed.
- General unrolled N is fixed by the example (N = 3 here); a loop-carried
  admission path is compiler work, not this cell's claim.

## Surface spelling

```emath
emath function shift3_spectrum_fp7:
    inputs:
        p: Int
    outputs:
        all_ok: Bool
    definitions:
        # S = [[0,1,0],[0,0,1],[1,0,0]] on F_p^3
        lam_is_root = congruence(pow_mod(2, 3, p), 1, p)
        # eigenvector of lambda = 2 is v = (1, 2, 4) = (g^0, g^1, g^2)
        Sv0 = int_rem(0 * 1 + 1 * 2 + 0 * 4, p)
        lam2v0 = int_rem(2 * 1, p)
        all_ok = lam_is_root and congruence(Sv0, lam2v0, p)
```

Runnable demonstration with exact spectrum and eigenvector tests:
`language/examples/analysis/finite-shift-operator.emath`.

## Named fences (deliberately open)

- No general-N unrolling machinery (same fence as the Fourier cell).
- No invariant-subspace or Jordan-form computation (needs general-n
  exact linear algebra; the nullspace seam is future work).
- No continuous shift/operator-algebra claims: this cell is exactly the
  finite cyclic world and refuses nothing silently beyond it.
