# `std.geometry.spherical`; chord/dot slice of the unit sphere

Status: language-layer capability cell (wave-16 math lane, Tier A
geometry/topology foundations).  HONEST THIN SLICE: the chord/dot
vocabulary of the unit sphere computes today as pure `.emath`
arithmetic; the ANGULAR forms (central angle, arc length, solid
angle) are NAMED FENCES because their `atan2`/`asin`/`acos`/`sqrt`
CALL lowerings are outside the Phase 1 strict-f64 execution subset.

## What computes (exact-first)

- `chord_squared(a, b) = Σ (a_i - b_i)²` over Vector[3] (componentwise
  squares; exact for integer/rational-valued coordinates — no `sqrt`,
  hence preferred over the chord length itself).
- Unit-sphere chord law: for unit vectors,
  `chord²(a, b) = 2 - 2·dot(a, b)`.  Pinned exactly at the cardinal
  points: chord²(i, j) = 2, chord²(i, -i) = 4, chord²(i, i) = 0, each
  agreeing with the dot-relation chain bit-for-bit; the witness also
  pins the 3-4-5 Pythagorean unit vector
  `[3/5, 4/5, 0]` with `norm² == 1.0` exactly and `dot(i, b) == 0.6`
  (single correctly-rounded literals, verified independently).
  Off the representable points the component-chain and the
  dot-relation differ by 1 ULP (0.8000000000000002 vs 0.8 for the
  3/5-4/5 witness) — that drift is pinned as data, not hidden: strict
  f64 has no tolerance band, and the cell does not claim the two forms
  are bit-identical away from exactly-representable points.

## NAMED FENCES (formulas of record, execution refused today)

- Central angle `theta = 2·atan2(c, sqrt(4 - c²))` with
  `c = norm(a - b)` (the haversine-equivalent stable form, acos-free);
  the atan2/asin-based spherical law of cosines; great-circle arc
  length `R·theta`; spherical excess (solid angle) via
  Van Oosterom-Strackee `Ω = 2·atan2(|det(a,b,c)|, 1 + a·b + b·c + c·a)`
  with the determinant as the scalar triple product; l'Huilier.
  All of these wait on the same emitter admission as the Lp fences in
  `std.geometry.metric`; the formulas stay here as the pinned
  semantics so the slice can widen without re-deriving.

## Test shape

Each test-bearing declaration observes exactly one evaluate target;
the two computation chains (componentwise chord² and the dot
relation) are pinned separately so their 1-ULP divergence off
representable points is visible, not hidden.

## Contracts

- Pure, deterministic, strict-f64; zero core delta.
- Runnable artifact:
  `language/examples/geometry/spherical-fundamentals.emath` (check +
  generated-crate tests pass; the 2.1-coefficient mutation of the
  antipodal chord law is killed by the relation witness).
