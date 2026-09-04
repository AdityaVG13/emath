# `std.geometry.metric`; distances, Cauchy-Schwarz, metric axioms over Vector[3]

Status: language-layer capability cell (wave-16 math lane, Tier A
geometry/topology foundations). Expressed IN `.emath` over the generic
Vector surface and the `norm`/`dot` builtins; zero core delta.

## What computes (semantics of record)

- `l2_3(p, q) = norm(p - q)`; `l2_squared_3(p, q) = Σ (p_i - q_i)²`
  componentwise (exact for integer-valued coordinates; this is the
  exact-first carrier of the metric).
- Cauchy–Schwarz slack: `(a·a)(b·b) - (a·b)² ≥ 0`, pinned exactly
  (witness 381 = 14·53 - 19²) with the equality case on parallel
  vectors pinned at exactly 0.
- Triangle inequality pinned as exact signed slack
  `d(p,r) - (d(p,q) + d(q,r))` (negative = strict); the collinear
  equality case `||s0-s2|| = ||s0-s1|| + ||s1-s2||` is pinned exactly.
- Scale homogeneity `d(αp, αq) = α·d(p, q)`: holds bit-for-bit for the
  sqrt(116) vs 2·sqrt(29) pair (both correctly-rounded IEEE results of
  the same real number); identity of indiscernibles pinned at 0.
- The only irrational literals are single correctly-rounded pins
  (sqrt(29)) verified independently; no epsilon anywhere.

## NAMED FENCES (deliberately open, not silent gaps)

- **L1/Manhattan and L∞/Chebyshev**: need componentwise `abs`/`max`.
  Those calls ADMIT end-to-end in the strict-f64 emitter since
  emath-s9w1m + fpl60 (`abs`/`min`/`max` piecewise; `sqrt` carries the
  `SqrtNonNegative` domain obligation, NaN value on a negative
  operand; `atan2` IEEE; `sign` = sgn, 0 at zero — probed 2026-09-01
  on `language/examples/intro/clamp-distance-builtins.emath`).  The
  Lp definitions are specified in the cell contract; the instances
  wait only on a cell revision, and the `L1 ≥ L∞ ≤ dim·L∞` span
  witnesses travel with them.
- General Vector[N] genericity, adaptive/exact interval metrics,
  geodesic metrics on curved carriers: named no-claims.

## Test shape

Each test-bearing declaration observes exactly one evaluate target
(Phase 1 generated-crate surface); inequalities are pinned as exact
signed slacks (negative = strict, zero = equality case), never
booleans; `dz`-drop mutation of the squared distance is killed by the
29.0 witness.

## Contracts

- Pure, deterministic, strict-f64; zero core delta (`emath-ir` zero
  delta, no domain-named variant, no parser branch).
- Runnable artifact:
  `language/examples/geometry/metric-fundamentals.emath` (check +
  generated-crate tests pass; mutation-killed).
