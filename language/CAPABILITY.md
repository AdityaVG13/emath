# emath Capability Matrix

> Single source of truth for what parses, what evaluates, and what is
> refused. Updated with every language change. When this file and a
> reference chapter disagree, the reference is normative.

## Declaration kinds

| Kind | Parses | Admits | Runs |
|------|--------|--------|------|
| `emath function` | yes | yes | yes (definitions evaluate) |
| `emath policy` | yes | yes | yes (stateful objects) |
| `emath model` | yes | yes | yes (`emath simulate` integrates ODEs) |
| `emath kind` | yes | partial (schema validation) | no |
| `emath custom` | yes | treats as function or refuses | no |
| other kinds | yes | refuses with named error | no |

## Sections

| Section | Status |
|---------|--------|
| `inputs` `outputs` `state` | admitted |
| `definitions` `equations` `equation` | admitted |
| `algebraic` | admitted (implicit DAE unknowns) |
| `constructors` | admitted |
| `constraints` | admitted (auto penalty method) |
| `invariants` | admitted |
| `goals` | admitted (`evaluate`, `differentiate`, `optimize`) |
| `exports` `tests` `compile` | admitted |
| `about` `evidence` `host` | admitted |
| `transitions` `events` | parses, not admitted |
| other | `E-SEC-101` |

## Types

| Type | Parses | Admits | Computes |
|------|--------|--------|----------|
| `Float64` `Real` `f64` | yes | yes | yes |
| `Bool` | yes | yes | yes |
| `Nat` `Int` | yes | yes | yes (Int → exact i64 output) |
| `Complex` | yes | yes | type-checks (eval pending) |
| `Mod<p>` `GF<p>` | yes | yes (as Int) | yes (via builtins) |
| `Vector[n]` `Matrix[r,c]` `Tensor[...]` | yes | yes | yes |
| `NonNegative<R>` `Positive<R>` `Probability<R>` | yes | yes | yes |
| `Interval<F>` | yes | yes | yes |
| `T in unit` | yes | yes | yes |
| `Rat` bare `Real` | yes | no | — |
| `Option` `Result` `Graph` `Field` | yes | no | — |

## Generic arguments at use sites

| Form | Example | Parses |
|------|---------|--------|
| Type only | `Vector<Float64>` | yes |
| Integer literal | `Mod<7>` | yes |
| Bracket-list extent | `Tensor<Float64, [N, N]>` | yes |
| Named argument | `GF<2, 3, modulus = x + 1>` | yes |

## Expressions

| Form | Example | Parses | Computes |
|------|---------|--------|----------|
| Arithmetic | `a + b * c` | yes | yes |
| Comparison | `x >= 1` | yes | yes |
| Logic connectives | `a ==> b`, `a <==> b` | yes | yes |
| Binders (sum/product/integral/forall/exists) | `sum i in 0..n: f(i)` | yes | yes |
| Filtered binders (`if` guard) | `sum i in 0..n if i > 0: f(i)` | yes | yes |
| Derivative (autodiff) | `derivative(y) wrt x` | yes | yes |
| Partial derivative | `partial(H) wrt T holding p` | yes | parse only |
| Total derivative | `total(t) wrt t` / `d(t) wrt t` | yes | parse only |
| Unicode partial | `∂(T) wrt x` | yes | parse only |
| Solve (Newton) | `solve(f) wrt x` | yes | yes |
| Optimize | `minimize(loss) wrt x` | yes | yes |
| einsum | `einsum("ik,kj->ij", A, B)` | yes | yes |
| Complex literal | `2i`, `3.5i`, `1 + 2i` | yes | type-checks |
| Unit query | `unit of E` / `dimension of E` | yes | parse only |
| Notation declarations | `notation infix 10 "⊕" => core::algebra::add` | yes | parse only |

## Builtins

| Function | Arity | Computes |
|----------|-------|----------|
| `exp` `ln` `log` `sqrt` `sin` `cos` `tan` `tanh` | 1 | yes |
| `abs` `floor` `ceil` `round` `sign` `log2` `log10` | 1 | yes |
| `sinh` `cosh` `atan` `cbrt` `recip` `fract` `is_finite` | 1 | yes |
| `norm` `transpose` `length` `len` `mean` | 1 | yes |
| `min` `max` `atan2` `pow` `mod` `hypot` `dot` | 2 | yes |
| `lerp` `clamp` | 3 | yes |
| `laplacian` `laplacian_neumann` `laplacian_2d` `laplacian_2d_neumann` | 2 | yes |
| `laplacian_dirichlet` | 4 | yes |
| `gradient` `gradient_2d_x` `gradient_2d_y` | 2 | yes |
| `sum` `product` | 1 (reduction) | yes |
| `einsum` | variable (≥2) | yes |
| `factorial` | 1 | yes (i64, n ∈ [0,20]) |
| `mod_inv` | 2 | yes (i64, extended GCD) |
| `cong` | 3 | yes (Bool) |
