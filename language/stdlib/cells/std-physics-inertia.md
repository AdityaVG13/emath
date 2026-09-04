# `std.physics.inertia`; the inertia tensor as a quadratic form

Status: **admitted end to end** — rigid-body rotational energy, angular
momentum consistency, and the parallel-axis theorem expressed as
capability data over the admitted `einsum` surface; every witness is
exact in f64 (integer inertia entries and integer angular velocities).
Runnable proof:
`language/examples/physics-engineering/inertia-tensor.emath`.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.physics.inertia` |
| Class | `world` (carrier + laws over a value domain) |
| Version | `0.1.0` |
| Migration | `stable` (Phase 1 surface; Float64 carrier) |
| Catalog item | classical mechanics / moment of inertia (Wave 16, physics section) |

## Carrier

The inertia tensor `I` as a symmetric `Matrix[3, 3]` (Float64); the
diagonal carrier `diag(2, 4, 6)` keeps all witnesses exact. Angular
velocity `ω` is `Vector[3]`. The physics reads through two einsum
contractions: `L = einsum("ij,j->i", I, w)` (angular momentum) and
`T = (1/2)·einsum("i,ij,j->", w, I, w)` (rotational kinetic energy).

## Laws (asserted in the runnable example)

1. **Quadratic-form energy**; `T = (1/2) ωᵀIω` — asserted 12 at
   ω = (1, 2, 1) with I = diag(2,4,6): (1/2)(2·1 + 4·4 + 6·1).
2. **Angular-momentum consistency**; `ω·(Iω) = 2T` — the L = Iω vector
   dotted back with ω doubles the energy exactly (asserted 24). This is
   the Euler-texture identity tying L to T through the same tensor.
3. **Parallel-axis theorem**; moving the rotation axis by d adds
   `m(|d|²E − ddᵀ)` to the tensor: `I' = I + m·(E − ddᵀ)` at
   d = (1,0,0), m = 1 gives diag(2, 5, 7) — trace grows 12 → 14, i.e.
   by exactly `2m|d|²` in 3-D (3|d|² on the diagonal from m|d|²E minus
   |d|² from ddᵀ). The outer product ddᵀ is
   `einsum("i,j->ij", d, d)`.

## Refusals / limitations (typed, never silent)

- The carrier is Float64: witnesses are exact only for small integer
  or exactly-representable inputs; general real tensors carry f64
  rounding, and no exactness is claimed beyond the witnesses.
- Off-diagonal (fully general) inertia tensors ride the same einsum
  surface; the diagonal carrier is chosen so `==` test witnesses stay
  honest. No principal-axis (eigen) solver is claimed — diagonal
  tensors are the admitted scope, and diagonalization is backend lane.

## No-claim boundaries

- No rigid-body dynamics integration (Euler equations of motion with
  the ω × Iω coupling), no angular-momentum conservation proofs, and no
  3-D orientation kinematics are claimed here; this cell is the static
  tensor geometry (energy, L, parallel-axis), not the time evolution.
- emath verifies the quadratic-form identities on given values; it does
  not prove the theorems (work-energy, parallel-axis) in general.
