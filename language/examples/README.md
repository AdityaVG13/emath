# Language Examples

Each example teaches one concept. Read in table order within a section.

## intro - getting started

| Example | Concept |
|---------|---------|
| [hello-square.emath](intro/hello-square.emath) | Minimal `emath function`. |
| [sum-one-to-five.emath](intro/sum-one-to-five.emath) | Finite `sum` binder. |
| [factorial.emath](intro/factorial.emath) | `product` binder with `Int` output (exact i64). |
| [range-sum.emath](intro/range-sum.emath) | Variable-bound runtime fold. |
| [forall-exists.emath](intro/forall-exists.emath) | Quantifier binders over a vector. |
| [integral.emath](intro/integral.emath) | `integral` binder (Simpson's rule). |
| [autodiff.emath](intro/autodiff.emath) | `derivative(y) wrt x` - forward-mode autodiff. |
| [solve.emath](intro/solve.emath) | `solve(f) wrt x` - Newton's method. |
| [optimize.emath](intro/optimize.emath) | `minimize` / `maximize` - gradient descent/ascent. |
| [constrained-opt.emath](intro/constrained-opt.emath) | `constraints:` section - auto penalty method. |
| [tensor-face.emath](intro/tensor-face.emath) | Rank-3 tensor, `:` slice, matrix `expect`. |
| [stateful-affine-scorer.emath](intro/stateful-affine-scorer.emath) | `emath policy` with constructor. |
| [vector-given.emath](intro/vector-given.emath) | `Vector[3]` input, indexing, `dot`. |
| [notation-ops.emath](intro/notation-ops.emath) | `notation` glyph declarations: `⊕`, `√`, postfix `inv` and `alias` spellings desugar to builtin calls and compute in generated Rust. |
| [algebraic-dae.emath](intro/algebraic-dae.emath) | Semi-explicit DAE with `emath simulate`. |
| [causalized-rc.emath](intro/causalized-rc.emath) | Fully implicit DAE — the `algebraic:` residual system is Newton-solved at each step, and `rust.library` codegen embeds the same causalized Newton solve (`step_euler`/`step_rk4` return `Result<Self, String>`). |
| [modular-arithmetic.emath](intro/modular-arithmetic.emath) | `GF<p>`, `factorial`, `mod_inv`, `congruence`, `rs_encode`, `hamming_distance`. |
| [attribute-gated.emath](intro/attribute-gated.emath) | `@capabilities(experimental-syntax)` + `@experimental` — the ELP experimental lane gate. |

## numerical - dynamics and PDEs

| Example | Concept |
|---------|---------|
| [explicit-mass-spring.emath](numerical/explicit-mass-spring.emath) | Coupled mass-spring as one vector-state ODE (`emath simulate`). |
| [heat-rod-sim.emath](numerical/heat-rod-sim.emath) | 1D heat equation: `laplacian` + `emath simulate`. |
| [heat-plate-sim.emath](numerical/heat-plate-sim.emath) | 2D heat equation: `laplacian_2d` + `emath simulate`. |
| [gradient-field.emath](numerical/gradient-field.emath) | `gradient` / `gradient_2d_x` / `gradient_2d_y`. |
