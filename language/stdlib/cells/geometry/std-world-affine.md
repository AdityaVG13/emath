# `std.geometry.world.affine`; affine-geometry world over R^2

Status: language-layer capability cell (wave-16 worlds lane, bead
emath-wave16-catalog-epic-fassw.26, catalog item **affine-geometry
world** — "affine combinations and transformations").  The world is
realized as ordinary user functions over the admitted index/arithmetic
surface: orientation, barycentric coordinates, an explicit affine map,
and the affine-map preservation law as an executable witness.  ZERO
CORE DELTA.

## World contract (what the cell fixes)

- Carrier: affine 2-space as `Vector[Float64]` point pairs.
- Orientation form `orient2(a, b, p)`: signed doubled area; 0 iff
  collinear (exact for integer coordinates).
- Barycentric coordinates `barycentric2(a, b, c, p)`:
  `lambda_b = orient2(a, c, p) / orient2(a, c, b)`,
  `lambda_c = orient2(a, b, p) / orient2(a, b, c)`,
  `lambda_a = 1 - lambda_b - lambda_c`.  The lambda_b denominator is
  `orient2(a, c, b)` — NOT the total; that sign subtlety is load
  bearing and is pinned by the witness (the flipped-denominator form
  is the historical bug this cell's probes kill).
- Affine map instance `T(x) = (2x + y + 1, 3y + 1)` (integer matrix,
  integer offset — exactness preserved).
- The defining affine law, pinned exactly:
  `T(sum lambda_i P_i) == sum lambda_i T(P_i)` (0.0 slack) and
  barycentric invariance `barycentric(T(A), T(B), T(C), T(P)) ==
  barycentric(A, B, C, P)` bit-for-bit at dyadic probes.

## LAW ENCODING FENCE

No boolean evaluate targets and no comparison operators in the
strict-f64 subset: incidence/collinearity are witnessed through
orient2 values (0 for degenerate, pinned signs otherwise) and affine
laws through exact 0.0 slacks.

## NAMED FENCES (world machinery, execution refused today)

- Native world declaration: genesis grammar design-only.
- Universal quantification ("for every affine map T") is world
  machinery; the cell pins the law on the declared map instance and
  two probe points.
- d-dimensional generalization (barycentric over simplices of
  dimension > 2), projective completions, and the discrete-geometry
  world (lattice point censuses) are catalogued fences in the
  wave-16 worlds slice — integer-valued exactness is already
  available, the remaining items need world machinery or integer
  chain structures, not this cell.

## Test shape

Each test-bearing declaration observes exactly one evaluate target;
barycentric weights are pinned as exact dyadic rationals (0.5, 0.25);
the map law and the barycentric-invariance law are pinned as 0.0
slacks.  Mutation-checked: a pin flip kills one probe, and the
historically-wrong denominator form (`total` in place of
`orient2(a, c, b)`) kills both probes.

## Contracts

- Pure, deterministic, strict-f64; zero core delta.
- Runnable artifact:
  `language/examples/geometry/worlds/affine-geometry-world.emath`
  (check + generated-crate tests pass).
- No-claim boundary: witnesses cover the pinned triangle, the pinned
  map, and dyadic-exact probes; no claim is made about non-dyadic
  barycentric weights (rounding there is f64 division, unpinned).
