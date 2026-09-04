# `std.finite_field.group`; the multiplicative group F_p^*

Status: **admitted end to end** — every group fact below is expressed
as capability data over the admitted `pow_mod` / `sqrt_mod` /
`mod_inv` primitives (square-and-multiply over i128 intermediates,
exact for `p <= 2^63`; the stage-2 big lane extends to moduli below
2^256). Runnable proof:
`language/examples/algebra/fp-multiplicative-group.emath` (all tests
green through `emath test`, interpreter and generated crate).

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.finite_field.group` |
| Class | `world` (carrier + laws over a value domain) |
| Version | `0.1.0` |
| Migration | `stable` (Phase 1 builtins; no compiler growth) |
| Catalog item | finite group theory (Wave 16 gaps, algebra and number theory) |

## Carrier

The multiplicative group `F_p^*` for a prime `p`: elements are the
field residues `1..p-1`, the law is multiplication mod `p`, and the
group order is `n = p - 1`. There is NO field-named compiler branch and
NO core IR enum growth: every operation is user capability data
composing the universal modular builtins (`pow_mod`, `sqrt_mod`,
`mod_inv`, `int_rem`, `congruence`).

## Surface (capability-data patterns, all runnable)

| Pattern | Spelling | Notes |
|---|---|---|
| Group exponentiation | `pow_mod(x, e, p)` | square-and-multiply; `e >= 0`, `p > 0`, typed refusals outside. |
| Element order | `pow_mod(x, d, p) == 1` tests | `ord(x)` divides `n`; witness test for the exact order `d` checks `x^d = 1` and `x^(d/q) != 1` for prime `q` dividing `d`. |
| Roots of unity census | `sum x in 1..p if pow_mod(x, d, p) == 1: 1` | `mu_d = {x : x^d = 1}` has exactly `gcd(d, p-1)` elements. |
| Generator (primitive root) test | `x^(n/q) != 1` for all prime `q` dividing `n` | for `n = 12`: check `x^6 != 1 and x^4 != 1`; the census counts `phi(n)` generators. |
| Euler's criterion | `pow_mod(a, (p-1)/2, p)` | `1` iff `a` is a quadratic residue, `p-1` iff a non-residue; the residue class is the index-2 subgroup. |
| Square roots | `sqrt_mod(a, p)` | Tonelli–Shanks; deterministic `min(x, p-x)` tie-break; typed refusal on non-residues (the criterion's `p-1` answer is the signal). |

## Laws (test targets, all asserted at p = 13, n = 12)

1. **Lagrange**; `x^(p-1) = 1` for every `x in F_p^*` — the census hits
   all `p-1 = 12` elements (`lagrange_census`).
2. **mu_d cardinality**; `mu_d` has exactly `gcd(d, n)` elements:
   `d = 4 -> 4`, `d = 6 -> 6`, `d = 5 -> 1` (coprime: only the
   identity), `d = 12 -> 12` (`mu_census`).
3. **Cyclicity**; exactly `phi(n)` elements generate: the maximal-
   proper-divisor test counts `phi(12) = 4` generators `{2, 6, 7, 11}`
   (`generator_census`).
4. **Order divides n**; `3` has order `3` in `F_13^*` (`3^3 = 27 = 1`
   while `3, 9 != 1`) (`order_witness`).
5. **Euler vs Tonelli–Shanks**; `euler_of_3 = 1` with
   `sqrt_mod(3, 13) = 4` (the residue round-trips through the gate),
   while `euler_of_2 = 12 = -1` and `sqrt_mod(2, 13)` refuses typed
   (`euler_of_3` / `euler_of_2` / `sqrt_of_3`).

## Refusals (typed, never silent)

- `pow_mod` with `p <= 0` or `e < 0`: typed refusal, never a wrap.
- `sqrt_mod` on a non-residue (or non-prime odd `p`): typed refusal —
  the exactness backstop re-checks `x^2 = a` before any root escapes.
- `mod_inv(a, m)` with `gcd(a, m) != 1`: typed refusal (no inverse).

## Fiber-census synergy (why this cell exists)

The zero-sum fiber law counts subsets of multiplicative subdomain
`mu_d` (mu_12 = F_13^* at the campaign's (17, 8, 9, 4) point; mu_32 and
mu_64 domains at big widths). Before counting subsets of a declared
`mu_d`, the census needs machine proof that the domain really has `d`
elements — exactly the `mu_census` law above. The generator census and
Euler criterion likewise verify subgroup structure the fiber arguments
lean on. Everything is plain admitted surface; no compiler change.

## No-claim boundaries

- No discrete logarithm, no primitive-root SEARCH beyond the witness
  test, and no factorization of `p - 1` is claimed; the maximal-divisor
  exponents arrive as inputs (`e2`, `e3` in `generator_census`).
- `p` must be prime by caller contract; the kernel's only prime gate is
  the Legendre/exactness backstop inside `sqrt_mod`.
- emath verifies group facts for given `p`; it does not prove the
  theorems (Lagrange, cyclicity, Euler's criterion) in general.
