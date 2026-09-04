# `std.physics.rotation`; coordinate transforms as the SO(2) group action

Status: **admitted end to end** — rotation matrices as capability data
over the admitted `einsum`/`transpose` surface; every group law below
is witnessed exactly at integer-entry rotations (f64 carries small
integers exactly). Runnable proof:
`language/examples/physics-engineering/rotation-coordinate.emath`
(all `emath test` asserts green).

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.physics.rotation` |
| Class | `world` (carrier + laws over a value domain) |
| Version | `0.1.0` |
| Migration | `stable` (Phase 1 surface; Float64 carrier) |
| Catalog item | coordinate transforms / analytical mechanics (Wave 16, physics section) |

## Carrier

2-D rotation matrices `R ∈ SO(2)` carried as `Matrix[2, 2]` literals.
The quarter turn `R90 = [[0, -1], [1, 0]]` is the exact generator: all
witnesses use integer entries, where f64 arithmetic is exact. Group
composition is `einsum("ik,kj->ij", ...)`, the Gram map is
`einsum("ki,kj->ij", ...)`, and transpose is the admitted builtin.

## Laws (asserted in the runnable example)

1. **Length preservation**; `|Rv|² = vᵀ(RᵀR)v = |v|²` — asserted 13 at
   v = (2, 3). The Gram carrier RᵀR is exactly I; note vᵀRv itself is 0
   for every v (R antisymmetric), so the metric must ride RᵀR — the
   O(2) defining law, made explicit by the witness.
2. **Involution / C4 structure**; R90² has trace −2 (the half turn
   −I), R90 composed with its inverse has trace 2 (identity), giving
   the exact C4 group table: 2, 0 (third power), −2, 2 (fourth).
3. **Orthogonality**; tr(R Rᵀ) = 2, the dimension — the defining
   orthogonal-group law at the exact witness.

## Refusals / limitations (typed, never silent)

- `cos`/`sin` admit at sema (`emath check` passes on a general-angle
  `R(th) = [[cos th, -sin th], [sin th, cos th]]` spelling) but the
  backend strict-f64 EMIR subset used by `emath test` refuses them
  ("unknown function `cos` in strict-f64 subset"). General-angle
  witnesses therefore wait on backend trig lowering; the exact
  integer-entry witnesses carry all group laws here. This is a
  backend/core limitation, not a cell extension point.
- π is not exactly representable in f64: even after trig lowering
  lands, general-angle `==` tests would be dishonest. The honest
  general-angle discipline is eval/build probes with tolerance, not
  exact asserts.

## No-claim boundaries

- No 3-D rotation surface (SO(3), Euler angles, quaternions) is
  claimed; the quaternion algebra cell is the existing quaternion
  carrier, and a 3-D rotation cell is a follow-up, not an extension.
- No determinant/belongs-to-SO(2) builtin: group membership is witnessed
  through the Gram and trace laws, not type-enforced.
- emath verifies group laws on given matrices; it does not prove them
  symbolically.
