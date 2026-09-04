# `std.analysis.finite.delta`; the forward difference operator on exact finite sequence spaces

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`int_rem`, `congruence`): no
core IR/op-enum change, no new binder kind. Discrete calculus slice of
the advanced-analysis pack: the difference operator is the exact
finite analogue of differentiation, and telescoping is its exact
finite analogue of the fundamental theorem.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.delta` |
| Class | `pure` (cyclic forward difference on `F_p^N`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

Fix a prime `p` and length `N`. The cyclic forward difference on the
sequence space `F_p^N` is the linear operator

```text
(Dx)[k] = x[(k+1) mod N] - x[k]   in F_p
```

In operator terms `D == S - I` with `S` the cyclic shift cell
(`std.analysis.finite.shift`); every law below is an exact finite
identity checked through unrolled component arithmetic and
`congruence`. The NON-cyclic restriction (differences only for
`k < N-1`, no wrap) is the same formulas without the last term; the
cell states both but the operator laws use the cyclic convention where
the shift connection is exact.

## Laws (exact, finite)

1. **Linearity.** `D(a x + b y) == a D(x) + b D(y)` in `F_p` — the
   operator is a difference of two linear operators.
2. **Telescoping (cyclic).** `sum_k (Dx)[k] == 0`: every component
   appears once with `+1` and once with `-1`. The finite analogue of
   "the integral of a derivative vanishes on closed loops". The
   partial (non-cyclic) telescope `sum_{k<m} (Dx)[k] == x[m] - x[0]`
   holds for the non-cyclic reading.
3. **Summation by parts (cyclic Abel).** For all `f, g`:

   ```text
   sum_k f[k] * (Dg)[k]  ==  sum_k g[k] * (f[k-1] - f[k])   in F_p
   ```

   (indices mod `N`). Derivation: reindex `sum_k f[k] g[k+1]` to
   `sum_j f[j-1] g[j]` and collect. This is the exact discrete
   integration-by-parts; no boundary terms survive the cyclic wrap.
4. **Spectrum via the shift.** `D == S - I` and `S` commutes with `I`,
   so `spec(D) = {g^j - 1 : j = 0..N-1}` where `g` is any principal
   N-th root of unity in `F_p` (Fourier cell). The kernel of `D` is
   exactly the constant sequences (eigenvalue `0`); the Fourier mode
   `v_j = (g^{0 j}, ..., g^{(N-1) j})` is an eigenvector with
   eigenvalue `g^j - 1`, checked componentwise.

## Refusals (typed, never silent)

- `p` composite: field laws unclaimed; the identities become ring
  statements without the spectral reading.
- The spectral law needs the shift cell's world (`N` arbitrary for
  telescoping/Abel; `N | p - 1` for the `N` distinct eigenvalues claim).
- General-N loops are fixed by unrolling in the example; a loop-carried
  admission path is compiler work, not this cell's claim.

## Surface spelling

```emath
emath function delta3_telescope:
    inputs:
        p: Int
    outputs:
        d0: Int
        d1: Int
        d2: Int
        telescope_ok: Bool
    definitions:
        # x = (1, 3, 2); cyclic Dx = (x1-x0, x2-x1, x0-x2)
        d0 = int_rem(3 - 1, p)
        d1 = int_rem(2 - 3, p)
        d2 = int_rem(1 - 2, p)
        telescope_ok = congruence(d0 + d1 + d2, 0, p)
```

Runnable demonstration with exact telescoping, Abel, and spectral
tests: `language/examples/analysis/finite-delta-operator.emath`.

## Named fences (deliberately open)

- No symbolic difference-equation SOLVER (recurrence solution in
  closed form is the generating-function lane's seam, not this cell).
- No discrete Stieltjes integration or finite-element assembly.
- No continuous limit claim (h → 0): the cell is exactly the finite
  cyclic world; convergence statements are refused by scope.
