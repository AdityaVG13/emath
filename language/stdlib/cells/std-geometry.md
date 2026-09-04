# `std.geometry`; geometry types over declared fields (thin slice)

Status: std-layer package (Phase 12),
implemented in `crates/emath-core/src/geometry.rs`. HONEST THIN SLICE:
the type vocabulary and the exact-rational reference ops are landed;
the language surface, notation routing, and algorithms are NAMED
FENCES, not silent gaps.

## Declared fields

`Field` is the scalar vocabulary: `Rational` (exact, gcd-reduced
i64/i64, overflow and zero-division refuse typed) and `f64` (IEEE,
refuses when a computation leaves the finite domain). The field is
part of the type: `Point<Rational>` and `Point<f64>` are different
geometries. Nothing wraps, saturates, or fabricates.

## Coordinate-bound vs coordinate-free (structural, not prose)

- `Point<F>` is coordinate-BOUND: transforms apply the full affine map.
- `FreeVector<F>` is coordinate-FREE: transforms apply ONLY the linear
  part; a pure translation moves points and never moves free vectors
  (mutant-tested).
- `dot`/`cross` exist only on free vectors. Point + point addition has
  NO impl anywhere: adding two points is a compile-time type error by
  construction. The admissible boundary is explicit:
  `a.displacement(&b)` yields the free vector, `p.translate(&v)` moves
  the point.
- The compile-time guarantee is documented here rather than runtime-
  tested; enforcing it at runtime would need a trybuild harness (a
  harness for a harness, declined).

## Exact-rational reference ops

`Rational` arithmetic is exact until the i64 envelope is exhausted,
then it refuses typed (`E-GEOMETRY-2`). Exactness witnesses pinned by
tests: `1/3 + 1/6 = 1/2`, `1/3 × 3 = 1` exactly, quarter-turn rotation
of `(1/2, 0)` lands on `(0, 1/2)` exactly, and the 3-4-5 triangle is
an exact containment witness on the unit circle
(`9/25 + 16/25 = 25/25 = 1`), with `(7/5, 4/5)` exactly off it; no
epsilon anywhere.

## Types

- `Point<F>`, `FreeVector<F>` (dot, 2-D scalar cross).
- `Line<F>` through two distinct points; coincident points refuse
  `E-GEOMETRY-1`; containment via the exact implicit form.
- `Conic<F>` general coefficients; all-zero refuses (the whole plane
  is not a conic); `unit_circle()`; exact `evaluate`/`contains`.
- `Transform<F>` identity/translation/scaling/quarter-turn rotations
  (entries in {−1, 0, 1}, exact for any i64 turn count via mod 4).

## Named fences (follow-up slices, deliberately open)

- **No `.emath` surface**: geometry types are not yet admitted
  language types; the reference still refuses `Rat` as a compute type.
  The exact field lives inside the Rust std layer until a language
  decision lands (BLOCKED on that decision).
- **G4 routing**: `×`/`·` overloads route through notation packs,
  never parser defaults; the notation-pack integration is BLOCKED on
  the notation lanes' machinery; this slice exposes named methods
  only.
- No computational-geometry algorithms (hulls, meshes), no CAD interop,
  no N-dimensional geometry, no normalization (needs sqrt), no conic
  classification beyond construction/evaluation.

## 3D slice; `std.geometry.cartesian3`

The 3D geometry pack lives in
`language/stdlib/cells/std-geometry-cartesian3.md` and is expressed as
inline `.emath` formulas over the generic Vector[3] surface (cross,
length/normalize/distance, sphere, plane, bounding box, mesh area and
signed volume), with semantics, laws, refusals and the call-seam fence
specified in `language/reference/geometry-and-topology.md`.
