# `std.geometry.cartesian3`; 3D geometry over the generic surface

Status: language-layer package (slice 2 of the
geometry pack). Geometry is EXPRESSED IN `.emath` over the existing
generic Vector/Matrix/Tensor operations; zero geometry-specific core
enums, crates, or parser branches. The named primitives `cross`,
`normalize`, `distance` are DECLARED PURE CAPABILITY CELLS
(`std.geometry.*`) reached through the generic declared-capability
call seam (`ExprNode::Apply` → `ApplyCapability`; they execute as
compiled reference cell data over the closed vector vocabulary;
`cross` composes bit-exact dot-with-basis component extraction, no
index operator, no kernel, no new op). They are still NOT builtins:
an unknown name refuses `E-TYPE-003`. The same semantics remain
pinned as inline formulas over `VectorIndex`/`VectorCreate` and the
existing `dot`/`norm` builtins (the inline laws are the semantic
pins; the named-call surface is pinned in
`tests/emath-sema/tests/geometry3d.rs` and
`tests/emath-ir/tests/geometry3d.rs`).

## Declared types (semantic, alias/data over Vector[3])

- `Point3D` = `Vector[3]` (coordinate position).
- `Vector3D` = `Vector[3]` (direction, unit-preserving when normalized).
- `Sphere` = center `Point3D` + radius `Float64`.
- `Plane` = unit normal `Vector3D` + `point` on the plane.
- `Ray3D` = origin `Point3D` + direction `Vector3D`.
- `BoundingBox3D` = `min`/`max` `Point3D`.
- `Mesh` = triangle soup over a matrix carrier (`N×3×3`), area and
  signed-volume accumulation.

## Operations (semantics of record)

- `cross(a, b) = (a.y*b.z - a.z*b.y, a.z*b.x - a.x*b.z, a.x*b.y - a.y*b.x)`.
  Laws pinned as executable tests: axis permutations (right-hand rule),
  anti-symmetry `cross(b,a) = -cross(a,b)`, scale homogeneity
  `cross(sa, b) = s*cross(a,b)`, `cross(u,u) = 0`, cyclic coordinate
  permutation equivariance. A `Vector[2]` cross refuses/faults typed
  (out-of-bounds index, never a fabricated third component).
- `length(v) = norm(v)`; `normalize(v) = v / length(v)`; the zero
  vector's normalization divides by zero and is never faked into a
  unit vector (strict-f64 non-finite boundary).
- `distance(a, b) = norm(a - b)`.
- Sphere: `contains(p) = norm(p - center) <= radius`;
  `volume = 4/3·π·r³`; `surface_area = 4·π·r²`;
  `surface_point(θ, φ) = center + r·(sin φ cos θ, sin φ sin θ, cos φ)`,
  every sample lies on the sphere (`norm(p - center) == r`, sampled law).
  A negative radius is data (the formula evaluates it); validation is
  the named type's job once surfaced; never silently normalized.
- Plane: `signed_distance(p) = dot(p - point, normal)` (unit normal
  contract); `contains(p) = signed_distance == 0`.
- Ray3D: `intersect_plane` / `intersect_sphere` return an Option; the
  real `Option<T>` surface is future work; until then the
  discriminant/nearest-positive-root semantics are the pinned formulas
  with the `Option` no-claim (E-TYPE-010 today).
- BoundingBox3D: `contains(p)` per axis `min <= p <= max`; `overlap`
  per axis `min_a <= max_b AND min_b <= max_a`. Conjunction is
  expressed per-axis (Phase 1 admits one comparison per binding).
- Mesh: `area = Σ 0.5·|cross(b-a, c-a)|`;
  `signed_volume = Σ 1/6·dot(cross(b-a, c-a), origin - a)`.
  CLOSEDNESS NO-CLAIM: triangle-soup topology (closedness/consistency
  certificates) is not claimed; a flat soup's signed volume is exactly
  0 and is asserted as such, not as an enclosed solid's volume.

## Refusals and no-claims

- `Vector[2]` cross → typed refusal/fault (E-SHAPE/E-TYPE path).
- zero-direction normalize → never a fabricated unit vector.
- `Option[Point3D]` ray results: `E-TYPE-010` until the Option surface
  lands; the discriminant math is pinned by the formulas.
- Mesh closedness, CAD interop, topology: named no-claim.

## Contracts

- The policy/type vocabulary is data: adding geometry never grows the
  core IR op/expr enums (`emath-ir` zero delta), never adds a
  domain-named variant, never forks the parser.
- Determinism class: pure; identical inputs → identical outputs
  (re-asserted across sessions in the targeted tests).
