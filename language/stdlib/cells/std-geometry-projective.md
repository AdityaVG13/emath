# `std.geometry.projective`; cross-ratio and homogeneous incidence

Status: language-layer capability cell (wave-16 math lane, Tier A
geometry/topology foundations). Projective geometry is EXPRESSED IN
`.emath` over the generic arithmetic/index surface; zero
geometry-specific core enums, crates, or parser branches.

## What computes (exact-first)

- `cross_ratio(xa, xb, xc, xd) = ((xa-xc)(xb-xd)) / ((xa-xd)(xb-xc))`
  for four collinear points on the affine chart.  Exact-first: every
  pinned witness uses coordinates whose divisions are exact in strict
  f64 (witness 1.5 = 6/4, second witness 1.125 = 9/8, both
  correctly-rounded exact rationals).
- `homogeneous_apply(m, p)`: 2-D homogeneous point `[x, w]` mapped by
  the row-major matrix `[m0, m1; m2, m3]`; the affine chart
  `x -> alpha*x + 1` (w-preserving) is the pinned instance.
- `cross-ratio invariance`: the pinned equality
  `CR(x -> alpha*x+1 applied) == CR(0,1,2,4) == 1.5` holds EXACTLY
  because every intermediate quotient of the witness is
  exactly-representable; this is the projective-invariance law as an
  executable test, not prose.
- `det3` incidence: three homogeneous points `[x, y, 1]` are collinear
  iff the 3x3 determinant is exactly 0; on-line (0) and off-line (+1)
  witnesses pinned exactly.  The determinant is the pure-builtin
  expansion of the triple product (no `cross` call needed at 3x3).

## Refusals and named fences

- Cross-ratio with coincident points (`xb == xc`): zero denominator
  divides by zero and refuses at execution (strict-f64 non-finite
  boundary); never clamped or patched.
- Points at infinity (`w = 0`), the projective completion of the
  affine chart, conic duality, and P^2 collineation groups: named
  fences (the homogeneous_apply surface already carries the [x, w]
  carrier shape they need).

## Contracts

- Zero core delta; pure; strict-f64; all witness divisions exact.
- Runnable artifact:
  `language/examples/geometry/projective-cross-ratio.emath` (check +
  generated-crate tests pass; the `+1.0` mutation of the cross-ratio
  formula is killed by the 1.5/1.5 invariance witness).
