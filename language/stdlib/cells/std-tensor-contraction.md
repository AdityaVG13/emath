# `std.tensor.contraction`; Einstein summation over the admitted surface

Status: **admitted end to end** — `einsum` admits at sema (explicit and
implicit free-index spellings), evaluates in the interpreter, and
compiles to the generated crate (`emath test` green on the runnable
proof). Runnable proof:
`language/examples/physics-engineering/tensor-contraction.emath`.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.tensor.contraction` |
| Class | `pure` (function class over Vector/Matrix values) |
| Version | `0.1.0` |
| Migration | `stable` (Phase 1 surface; Float64 carrier) |
| Catalog item | tensor algebra / representation theory (Wave 16, algebra + physics sections) |

## Carrier

Real `Vector[N]` and `Matrix[R, C]` values (Float64 entries). Indices
are named by single letters in the einsum spec string; repeated indices
in one argument are traces/contractions, commas separate operands, `->`
names the free indices of the result. Implicit mode (`"ik,kj"`) infers
alphabetical free indices. `a * b` on matrices is the same contraction
as `einsum("ik,kj->ij", a, b)` — the example asserts they agree.

## Surface (all witnessed in the runnable example)

| Pattern | Spelling | Notes |
|---|---|---|
| Matrix product | `einsum("ik,kj->ij", a, b)` | equals `a * b`; [[19,22],[43,50]] witness at a=[[1,2],[3,4]], b=[[5,6],[7,8]]. |
| Trace | `einsum("ii->", a)` | tr(A) = 5 and 13 for the witnesses; linear and cyclic (tr(AB) − tr(BA) = 0 asserted via the −51 gap witness). |
| Quadratic form | `einsum("i,ij,j->", v, A, v)` | vᵀAv; 18 at A=diag(2,4), v=(1,2). Antisymmetric part vanishes: vᵀWv = 0 for W antisymmetric (asserted 0). |
| Matrix-vector | `einsum("ij,j->i", A, v)` | (Av)·v reaches the same quadratic form through the intermediate vector (asserted equal at 18). |
| Outer product | `einsum("i,j->ij", d, d)` | d dᵀ; used by the parallel-axis witness in `std.physics.inertia`. |
| Gram matrix | `einsum("ki,kj->ij", R, R)` | RᵀR; the O(2)-law carrier in `std.physics.rotation`. |

## Laws (asserted in the runnable example)

1. **Consistency**; explicit, implicit, and `*` spellings of the same
   contraction agree exactly.
2. **Trace linearity + cyclicity**; tr(A) + tr(B) = tr(A+B) and
   tr(AB) = tr(BA) on the 2×2 witnesses.
3. **Symmetric projection**; a quadratic form sees only the symmetric
   part of its matrix: vᵀWv = 0 for antisymmetric W, any v.
4. **Factorization**; (Av)·v = vᵀAv — contraction order along the
   middle index does not change the scalar.

## Refusals (typed, never silent)

- einsum spec/index errors are refused at sema with the standard shape
  diagnostics (E-SHAPE/E-TYPE family); a spec whose free indices do not
  match the operand ranks never evaluates.
- The carrier is Float64: no exactness is claimed beyond exact small
  integer witnesses (all example values are exact in f64).

## No-claim boundaries

- No symbolic index algebra, no einsum optimization, and no exact
  (Int/BigInt) matrix carrier is claimed; that is backend/core lane
  territory.
- Plain shape-matched dense tensor addition is capsule-active as
  `std.capability.tensor.add` (kernel `dense-tensor-add`) in
  `language/spec/capabilities/linear-algebra.emath`; einsum
  contraction itself stays on the legacy path until the variadic
  carrier hole below is closed.
- einsum in the strict-f64 backend subset does not lower `cos`/`sin`
  inside matrix literals (found while building `std.physics.rotation`);
  trig-bearing contractions wait on backend trig lowering.
- emath verifies contraction identities on given values; it does not
  prove them for symbolic inputs.
