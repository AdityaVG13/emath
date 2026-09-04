# `std.analysis.finite.fourier`; exact finite Fourier (NTT) over `F_p`

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`pow_mod`, `int_rem`,
`mod_inv`, `congruence`): no core IR/op-enum change, no new binder
kind. The continuous Fourier transform is a limit object; this cell is
its EXACT discrete finite analogue, where every claim is a finite
integer identity, not a tolerance.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.fourier` |
| Class | `pure` (length-N NTT over `F_p`) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

Fix a prime `p` and `N >= 1` with `N | p - 1`. Let `g` be an element of
order exactly `N` in `F_p^*` (a principal N-th root of unity: `g^N == 1`
and no smaller positive power is 1). The number-theoretic transform of
`x : F_p^N` is

```text
X[k] = sum_{n=0}^{N-1} x[n] * g^(n*k mod N)   in F_p
```

with every exponent reduced through `int_rem(n*k, N)` and every power
through `pow_mod`. The inverse transform is the same sum with `g` and
`N^-1 = mod_inv(N, p)`:

```text
x[n] = N^-1 * sum_{k=0}^{N-1} X[k] * g^(-(n*k) mod N)   in F_p
```

The reference transform is the DIRECT O(N^2) double sum, unrolled over
the fixed N of the example; an FFT-style provider behind a contract is
a later slice (same fence as `std.signal`), never a semantic change.

## Laws (exact, finite)

1. **Round trip.** Applying the inverse transform to the transform
   recovers `x` exactly: `y[n] == x[n] (mod p)` for every `n`. This is
   the finite analogue of Fourier inversion; there is no truncation and
   no tolerance.
2. **Orthogonality / Parseval (conjugate-paired).** Because `x` has
   entries in the base field (no extension tower), the exact identity is

   ```text
   sum_n x[n]^2  ==  N^-1 * sum_k X[k] * X[(-k) mod N]   (mod p)
   ```

   The conjugate of `X[k]` is `X[(-k) mod N]`, NOT `X[k]`: the naive
   real Parseval `sum x^2 == N^-1 sum X^2` is generally FALSE in
   `F_p` and the cell refuses to claim it.
3. **Cyclic convolution theorem.** For `c = x (*_N) y` (cyclic
   convolution, indices mod N), `NTT(c)[k] == X[k] * Y[k]` in `F_p`.
   Equivalently `c = INTT(X . Y)`. Linear (acyclic) convolution is the
   length-`(2N-1)` padding variant and is a separate claim.
4. **Linearity.** `NTT(a*x + b*y) == a*NTT(x) + b*NTT(y)` in `F_p`.

## Refusals (typed, never silent)

- `g` of order other than exactly `N`: the transform is not invertible;
  the round-trip law is the admission test (a wrong `g` fails it, and
  the failure is the typed negative, mirroring the `std.signal`
  exact-pair contract test).
- `N` not dividing `p - 1`: no element of order `N` exists; the world
  does not apply.
- `p` composite: `F_p` is not a field; laws are unclaimed.
- Zero divisor arithmetic never appears because every intermediate is
  reduced mod the prime `p`.

## Surface spelling

```emath
emath function ntt4_roundtrip:
    inputs:
        p: Int
    outputs:
        X0: Int
        y1: Int
    definitions:
        # N = 4, principal root g = 2 (2^4 = 16 == 1 mod 5), x = [1,2,3,4]
        g = 2
        X0 = int_rem(1 + 2 + 3 + 4, p)
        X1 = int_rem(1 + 2 * pow_mod(g, 1, p) + 3 * pow_mod(g, 2, p)
                     + 4 * pow_mod(g, 3, p), p)
        # X2, X3 are the same unrolled sum at k = 2, 3 (see the example).
        # inverse: X_k paired with exponent (4 - n*k) mod 4, scaled by N^-1
        ninv = mod_inv(4, p)
        y1 = int_rem(ninv * (X0 * pow_mod(g, int_rem(4 - 0, 4), p)
                             + X1 * pow_mod(g, int_rem(4 - 1, 4), p)
                             + X2 * pow_mod(g, int_rem(4 - 2, 4), p)
                             + X3 * pow_mod(g, int_rem(4 - 3, 4), p)), p)
```

Runnable demonstration with exact round-trip and conjugate-paired
Parseval tests: `language/examples/analysis/finite-fourier-ntt.emath`.

## Named fences (deliberately open)

- No `.emath` DFT/FFT call surface (the `std.signal` fence stands:
  signals keep the declared-sampling world; this cell is index-world
  arithmetic over `F_p`, a different carrier by design).
- No general-N unrolling machinery: examples fix N and unroll; a macro
  or loop-carried admission path is compiler work, not stdlib data.
- No extension-field NTT (Galois rings), no negative-packed
  convolution, no NTT-based multiplication wiring into `poly_mul`.
