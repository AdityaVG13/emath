# `std.finite_field.polynomial`; polynomial algebra over F_p

Status: **admitted end to end** — `poly_eval_mod` (Horner over i128
intermediates) and `rs_encode` (the full evaluation map) admit through
sema call admission, evaluate in the interpreter, and compile to the
shared `emath-rt` kernels (`crates/emath-rt/src/body/numeric.rs`).
Runnable proof: `language/examples/algebra/fp-polynomial-census.emath`.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.finite_field.polynomial` |
| Class | `world` (carrier + laws over a value domain) |
| Version | `0.1.0` |
| Migration | `stable` (Phase 1 builtins; stage-2 big lane via emath-t63iz) |
| Catalog item | polynomial algebra (Wave 16 gaps, algebra and number theory) |

## Carrier

Univariate polynomials over the prime field `F_p`, carried as dense
ASCENDING coefficient vectors (`coeffs[i]` = coefficient of `x^i`; the
empty vector is the zero polynomial, matching the B28 compute-layer
convention). Coefficients ride the `Vector<Float64>` surface but must be
exact integers: the kernel's `exact_i64` gate refuses a fractional
coefficient typed instead of rounding. The point and modulus carry the
stage-2 promotion: any `BigInt` operand moves the whole op to the big
lane (exact for any modulus below 2^256, pinned at the Curve25519 prime).

## Surface

| Function | Signature | Notes |
|---|---|---|
| `poly_eval_mod(coeffs, x, p)` | `(Vector, Int, Int) -> Int` | Horner's method; result in `0..p-1`. `p <= 0` refuses typed. i128 intermediates keep every step exact for `p <= 2^63` (the same width contract as `pow_mod`/`sqrt_mod`); the big lane extends to 2^256. |
| `rs_encode(coeffs, n, p)` | `(Vector, Int, Int) -> Vector` | the evaluation map `x = 0..n-1` in one call; `n` must be in `(0, p]` (typed refusal outside). This is the encoding side of Reed–Solomon over `GF(p)`. |

## Laws (test targets)

1. **Fermat whole-field census**; `f(x) = x^p - x` vanishes on all of
   `F_p`: the fiber `f^{-1}(0)` has cardinality exactly `p`
   (`fermat_census` in the example asserts 7 at `p = 7`).
2. **Fiber census pattern**; for any coefficient vector and target
   `t`, `#{x in F_p : f(x) = t} = sum x in 0..p if poly_eval_mod(coeffs, x, p) == t: 1`
   (half-open `0..p` walks exactly the p field elements).
3. **Cube-fiber counting**; the fiber of `x^k` over `F_p^*` has
   `gcd(k, p-1)` preimages for `t != 0` and exactly one (`{0}`) for
   `t = 0` (`cube_fiber_census` asserts 3 and 1 at `p = 7, k = 3`).
4. **Identity by evaluation**; for `deg f < p`, `f = g` as polynomials
   iff `f(x) = g(x)` for all `x in F_p` — the census sum discriminates
   polynomials below the degree bound. (Beyond `deg >= p` evaluation
   only sees the function, never the coefficients.)

## Refusals (typed, never silent)

- `p <= 0`: `poly_eval_mod: modulus must be positive`.
- Fractional coefficient: the `exact_i64` gate refuses (no silent
  rounding of `2.5` to a field element).
- `rs_encode` with `n <= 0` or `n > p`: codeword length refused.
- Non-prime `p` is NOT refused by the kernel (Horner is well-defined
  mod any positive modulus); primality is the caller's contract, same
  as the `sqrt_mod` Legendre gate being the only prime check downstream.

## Fiber-census synergy (why this cell exists)

The zero-sum fiber law (proximity-prize campaign, `N1 = N2`) counts
subsets of a domain whose sum lands in a given residue class mod `p` —
a fiber cardinality of an explicit map into `F_p`. The census pattern
above is the same shape with the polynomial map made explicit: sweep
the domain, evaluate exactly, count the fiber. Everything the census
lane needs from polynomial algebra — exact evaluation mod p at i64 and
big widths, all-points evaluation maps, fiber cardinalities — is
already admitted surface.

## No-claim boundaries

- No polynomial GCD, factorization, or interpolation is claimed here;
  `poly_add`/`poly_mul`/`poly_eval` (Float64 carrier) are the B28
  compute-layer primitives. They are capsule-active dense carriers in
  `language/spec/capabilities/linear-algebra.emath`
  (`std.capability.poly.mul` / `std.capability.poly.eval` over the
  shared `polynomial-multiply` / `polynomial-evaluate` kernels, and
  `std.capability.linear.vector-add` for `poly_add`), not in this
  field cell.
- Coefficients stay on the Float64 surface; the exactness gate lives in
  the kernel (`exact_i64`), not in the type system. A `Vector<Int>`
  coefficient carrier is a follow-up, not an extension of this cell.
- emath verifies census claims for given `p`; it does not prove the
  theorems (Fermat, the gcd fiber count) in general.
