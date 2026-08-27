# Language Examples

A small set of runnable programs. Syntax, refusals, and “what computes”
live in [`../reference/`](../reference/overview.md), [`../QUICKSTART.md`](../QUICKSTART.md),
and [`../CAPABILITY.md`](../CAPABILITY.md) — not a new `.emath` file per feature.

## intro

Declaration syntax is in [`../QUICKSTART.md`](../QUICKSTART.md). These three
show what is not a homework algebra drill: an unsolved object, calculus,
and a PDE. Other `.emath` files under `intro/` are the WASM/regression
corpus, not the reading list.

| Example | Concept |
|---------|---------|
| [scratch.emath](intro/scratch.emath) | Typed hole: `f(x)=?` with `f'=f`, `f(0)=1`. emath does not invent `f`. |
| [v9_06_2rdq_10.emath](intro/v9_06_2rdq_10.emath) | `emath capability` admitted through `use std.kinds.capability`, not a parser or stable-IR branch. |
| [v9_06_2rdq_11.emath](intro/v9_06_2rdq_11.emath) | Imported `theory`, finite `model`, and `morphism`; Mod17 laws and the `Power` scale map are checked exhaustively. |
| [v9_06_2rdq_12.emath](intro/v9_06_2rdq_12.emath) | `ElementwiseUnary<Op>` family generates `Sin`, `Exp`, and `Sqrt` capability cells through one imported schema. |
| [autodiff.emath](intro/autodiff.emath) | Forward-mode `derivative(y) wrt x` (dual-number tangent). |
| [heat-rod-sim.emath](numerical/heat-rod-sim.emath) | 1D heat equation: `der(u) = alpha * laplacian(u, dx)`, `emath simulate`. |

## physics - executable laws

| Example | Concept |
|---------|---------|
| [newton-second.emath](physics/newton-second.emath) | `emath law` metadata plus unit-checked, executable `force = mass * acceleration`. |

## science - measured values and provenance

| Example | Concept |
|---------|---------|
| [measured-provenance.emath](science/measured-provenance.emath) | Identity-bearing `Citation` and visible `Assumed` provenance, rendered by `emath explain --provenance`. |

## algebra - symbolic computation

| Example | Concept |
|---------|---------|
| [symbolic-cas.emath](algebra/symbolic-cas.emath) | `simplify <value>` computes a native structural simplification plan for an exact scalar expression. |

## numerical - dynamics and PDEs

| Example | Concept |
|---------|---------|
| [explicit-mass-spring.emath](numerical/explicit-mass-spring.emath) | Coupled mass-spring as one vector-state ODE. Classic RK4 via `emath simulate --set s=[x,v]`; undamped (c=0) tracks x=cos(t). |
| [heat-rod-sim.emath](numerical/heat-rod-sim.emath) | 1D heat equation: `laplacian` + `emath simulate`. |
| [heat-plate-sim.emath](numerical/heat-plate-sim.emath) | 2D heat equation: `laplacian_2d` + `emath simulate`. |
| [heat-volume-sim.emath](numerical/heat-volume-sim.emath) | 3D anisotropic heat equation: rank-3 Tensor, `laplacian_3d`, and RK4 simulation. |
| [gradient-field.emath](numerical/gradient-field.emath) | `gradient` / `gradient_2d_x` / `gradient_2d_y`. |
| [spatial-3d.emath](numerical/spatial-3d.emath) | 3D gradients, anisotropic Laplacian, and 1D/2D/3D divergence. |
