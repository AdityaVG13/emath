# `std.geometry.affine`; affine combinations, orientation, barycentrics, coplanarity

Status: language-layer capability cell (wave-16 math lane, Tier A
geometry/topology foundations). Geometry is EXPRESSED IN `.emath`
over the generic Vector surface and the existing builtins; zero
geometry-specific core enums, crates, or parser branches (the cell
composes only user functions, builtins, and the generic
index/expression surface; the declared capability cells
`std.geometry.cross` are the documented alternative spelling, not a
dependency of this slice).

## Types (semantic aliases, no new types)

- `Point2D` = `Vector[Float64]` with 2 components (coordinate position).
- `Point3D` = `Vector[Float64]` with 3 components.
- `Scalar` = `Float64`.

## Operations (semantics of record)

- `affine_combination2(t, p0, p1) = p0 + t*(p1 - p0)` componentwise;
  `t = 0` returns `p0`, `t = 1` returns `p1`, `t = 0.5` is the exact
  midpoint at representable coordinates; affine equivariance under
  coordinate scaling pinned exactly at power-of-two `t`.
- `orient2(a, b, p) = (b-a) x (p-a)` scalar orientation:
  positive = counterclockwise, zero = collinear, negative = clockwise;
  `0.5 * orient2(a, b, c)` is the signed triangle area.
- `barycentric2(a, b, c, p)`: λ = (λa, λb, λc) with
  `λb = orient2(a,c,p)/orient2(a,c,b)`,
  `λc = orient2(a,b,p)/orient2(a,b,c)`,
  `λa = 1 - λb - λc`.  Pinned exact witness: triangle (0,0),(4,0),(0,4)
  at p=(1,1) gives λ = (0.5, 0.25, 0.25) and the reconstruction
  λa·A + λb·B + λc·C lands exactly on (1,1).
- `coplanarity3(a,b,c,d)`: scalar triple product as the 3x3
  determinant expansion (pure-builtin spelling of
  `dot(b-a, cross(c-a, d-a))`); zero ⟺ coplanar; in-plane and
  off-plane witnesses pinned exactly.

## Exactness class

Every pinned witness uses integer-valued coordinates and power-of-two
scales, so every value is exactly-representable and no epsilon appears
anywhere.  Degenerate barycentric input (collinear witness triangle,
total = 0) divides by zero and refuses at the strict-f64 non-finite
boundary; it is never clamped or faked.

## Executable artifact

`language/examples/geometry/affine-barycentric.emath` — `emath check`
clean, `emath test` green (generated-crate tests), mutation-killed
(the orientation sign mutant is caught by the diagonal-sign witness;
it survived the first axis-aligned-only suite and the suite was
strengthened, not the assertion loosened).

## Refusals and no-claims

- Barycentric coordinates of a point against a degenerate (collinear)
  triangle divide by zero and refuse at execution (strict-f64
  non-finite boundary); they are never clamped.
- No N-dimensional genericity (2-D/3-D fixed arities), no tolerance
  predicates, no adaptive precision; determinism class: pure,
  identical inputs → identical outputs.
