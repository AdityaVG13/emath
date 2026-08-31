# Chapter: Geometry and Topology (over the generic compute surface)

Normative status of the geometry pack (bead `emath-talo`). Geometry is
DATA over the existing generic Vector/Matrix/Tensor surface; the
compiler core (parse, sema, EMIR, reference VM, emit) gains no
geometry-named enums, variants, or branches. What computes below is
pinned by executable tests in
`tests/emath-sema/tests/geometry3d.rs` and
`tests/emath-ir/tests/geometry3d.rs`.

## 1. Policy principle

New mathematics enters as `.emath` capability cells / families;
never as new domain-named core IR/op enum variants. The 3D geometry
cell `std.geometry.cartesian3` (see
`language/stdlib/cells/std-geometry-cartesian3.md`, and the 2D
thin slice in `language/stdlib/cells/std-geometry.md`) follows this
rule exactly: types are semantic aliases over `Vector[3]` (or small
carrier matrices), and every operation is an inline formula over the
generic expression surface.

## 2. What computes today (inline formulas, executed)

All of the following are verified by targeted tests that parse,
admit, and execute `.emath` programs; no core branch implements them.

- `cross(a, b)` for `Vector[3]`:
  `(a.y·b.z − a.z·b.y, a.z·b.x − a.x·b.z, a.x·b.y − a.y·b.x)`.
  Pinned laws: right-hand axis rule (`i × j == k`, and cyclic
  coordinate permutations), anti-symmetry `cross(b, a) == −cross(a, b)`,
  scale homogeneity `cross(s·a, b) == s·cross(a, b)`,
  `cross(u, u) == 0` (componentwise), sampled determinism.
- `length(v) = norm(v)`; `normalize(v) = v / length(v)`; the zero
  vector never normalizes into a fabricated unit vector (non-finite
  refusal at execution).
- `distance(a, b) = norm(a − b)`.
- Sphere: `contains(p) = norm(p − center) ≤ radius`; `volume =
  4/3·π·r³`; `surface_area = 4·π·r²`; parameterization
  `surface_point(θ, φ) = center + r·(sin φ·cos θ, sin φ·sin θ, cos φ)`
  with the containment law sampled on a grid of (θ, φ) pairs.
- Plane (unit-normal contract): `signed_distance(p) = dot(p − point,
  normal)`; `contains(p) = signed_distance == 0`.
- BoundingBox3D (per-axis): `contains(p)` iff
  `min.x ≤ p.x ≤ max.x` and per-axis lower/upper for y and z;
  `overlap` iff per-axis intervals intersect. Phase-1 constraint
  groupers admit one comparison per binding, so conjunctions are
  expressed per axis (documented phrasing, not a semantic limit).
- Mesh (triangle soup over an `N×3×3` carrier):
  `area = Σ ½·|cross(b − a, c − a)|`; signed
  `volume = Σ 1/6·dot(cross(b − a, c − a), origin − a)`.
  A flat square soup's signed volume is exactly 0 and is asserted as
  such. TOPOLOGY NO-CLAIM: closedness/consistency certificates are not
  claimed; "volume" here is the signed accumulation, not an enclosed
  solid's volume.

## 3. Refusals and named fences

- `cross` over a `Vector[2]` (or any out-of-bounds component read):
  refused typed at execution (index fault), never a fabricated third
  component. Today the admission surface does not shape-check the
  index; the honest contract is refusal/fault at execution and the
  admission-time shape refusal is part of the call-seam work below.
- `normalize(0-vector)`: refuses (non-finite at execution), never
  returns a unit vector.
- Negative radius spheres: the FORMULA evaluates deterministically
  (volume/area stay positive by construction); validating "radius is
  data" is a type-contract responsibility, not silent clamping.
- `Option[Point3D]` ray results: refused `E-TYPE-010` until the
  generic `Option<T>` surface lands (SilverMaple lane). The
  discriminant math for ray–plane / ray–sphere is pinned by the
  formulas above.
- Named builtins `cross`, `normalize`, `distance` do not exist;
  and still do not: the names are DECLARED PURE CAPABILITY CELLS
  (`std.geometry.cross` / `std.geometry.normalize` /
  `std.geometry.distance`) reached through the generic call seam
  (§4). An unknown name still refuses `E-TYPE-003`; nothing became a
  builtin.

## 4. The call seam (the one minimal core change)

The architecture decision for invoking declared functions/capability
cells is GENERIC declared-function/capability invocation:

- sema: `ExprKind::Call` dispatch falls through to a resolution of
  declared function/cell names in the package (and mounted cells),
  lowering to the existing `ApplyCapability` term family;
- term/emit: compile the referenced cell/function body through the
  existing `compile_reference` spine and emit the call;
- exec: reuse the existing `ApplyCapability` evaluation.

This is deliberately NOT: geometry-specific builtins, new EMIR
variants, or a parallel call path.

**Landed status:** the seam is live. The three primitives are declared
in `language/examples/geometry/3d-primitives.emath` with the standard
capability surface (`class: pure`, `inputs:`/`outputs:` typed
`Vector[Float64]`/`Float64`) and called by name (bare and qualified
spellings both resolve). A call lowers to `ExprNode::Apply` →
`ApplyCapability`; the cells execute as compiled reference cell data
over the closed vector vocabulary (`cross` composes bit-exact
dot-with-basis component extraction; no index operator, no kernel,
no new op; `normalize` is the generic vector-scalar divide;
`distance` is `norm(a − b)`). Unknown names still refuse typed, and
the strict Rust codegen backend does not lower capability
applications yet (`emath run` refuses typed; the reference VM / interp
world executes them). §2's inline-formula laws remain the semantic
pins; the named-call surface is pinned by
`tests/emath-sema/tests/geometry3d.rs` (admission + `ExprNode::Apply`
targets) and `tests/emath-ir/tests/geometry3d.rs` (exact values
through the call path, mutation-killed).

**User-defined functions are callable too (emath-0e68):** a call whose
name matches a sibling `emath function` declaration in the same source
resolves through the same seam by pure-inline substitution at sema;
the callee's parameters bind as `#`-suffixed renamed definitions over
the caller-side argument subtrees, the callee's `definitions:` body
lowers in a swapped environment, and the output binding folds in
through the existing inliner. No new IR node, no registry entry, no
runtime callee frame; recursion refuses typed (`E-TYPE-013`, inline
depth cap 32), arity/type mismatches refuse `E-TYPE-012`, and a
callee-local name can never collide with a caller name (the `#`
separator is not a valid identifier character). This is the path the
parametric surfaces travel: `r(t) -> Vector[3]`,
`r(u,v) -> Vector[3]` (paraboloid, sphere, torus), and the implicit
field `f(p) -> Float64` are ordinary user functions called from an
acceptance function; see
`language/examples/geometry/parametric-surfaces.emath` (the runnable
example) and `tests/emath-ir/tests/parametric_surfaces.rs` (exact
values, arity refusal, reparameterization invariance, determinism,
mutation-killed).

## 5. Determinism and no-claim boundaries

- Pure, deterministic evaluation: same inputs, same outputs, verified
  across sessions (`run_definitions_value` re-run stability).
- No-claims: topology certificates, CAD interop, solid classification,
  mesh repair, and any geometry whose truth depends on floating-point
  tolerance beyond IEEE strict semantics.
