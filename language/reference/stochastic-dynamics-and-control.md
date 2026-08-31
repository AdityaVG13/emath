# Stochastic dynamics and control

## Deterministic stochastic executions (SDE)

The scalar SDE surface executes `dX = μ(X) dt + σ(X) dW` with
ASCENDING polynomial carriers (the B28 law): `μ` and `σ` are
coefficient vectors, `μ(x) = Σ aᵢxⁱ`, `σ(x) = Σ bᵢxⁱ`; the empty
carrier is the zero polynomial (σ ≡ 0 ⟹ the ODE reduction).

### Rules

The two rules are PURE capability cells declared with the standard
capability surface and called by name; there is no `sde` keyword and
no `ito`/`stratonovich` mode switch anywhere in the language: the
rules are cell data, and the call resolves through the generic
declared-capability call path (`ExprNode::Apply` → `ApplyCapability`;
kernel-backed cells execute via the immutable native-kernel registry;
the shared builtin-miss seam other domains reuse).

| Cell call | Rule | Step |
|---|---|---|
| `euler_maruyama(μ, σ, x0, h, n, seed, stream)` | Itô (Euler–Maruyama) | `X' = X + μ(X)·h + σ(X)·√h·Z` |
| `stratonovich(μ, σ, x0, h, n, seed, stream)` | Stratonovich (Euler–Heun) | `X' = X + μ(X)·h + σ(X)·√h·Z + ½·σ(X)·σ'(X)·h·Z²` |

The cells are declared once per package (canonical names
`std.stochastic.euler_maruyama` / `std.stochastic.stratonovich` when
declared under `package std.stochastic`):

```
package std.stochastic
use std.kinds.capability

emath capability euler_maruyama:
    class: pure
    version: "1.0.0"
    migration: frozen
    inputs:
        drift: Vector[Float64]
        diffusion: Vector[Float64]
        x0: Float64
        h: Float64
        steps: Float64
        seed: Float64
        stream: Vector[Float64]
    outputs:
        trajectory: Vector[Float64]
```

- `h` is the step size (positive finite), `n` the step count (positive
  integer), `seed` an explicit finite value in `[0, 2^64)`, `stream`
  the label carrier (the root stream executes today).
- The two rules are MATHEMATICALLY DISTINCT for state-dependent noise
  (σ' ≠ 0): both execute; the correction term is never dropped or
  merged. For additive noise (σ' = 0) they agree bit-for-bit.
- Both return a trajectory `[x0, x1, …, xN]` of length `n + 1` (the
  declared `Vector[Float64]` output types the call result, so indexing
  `trajectory[n]` is admitted).
- A call to a name that is neither a builtin nor a declared capability
  cell refuses with the typed unknown-function diagnostic (`E-TYPE-003`)
 ; the capability path never silently admits unknown names. A wrong
  argument count admits at the call site and refuses at evaluation with
  the cell contract's typed message (one arity discipline across the
  compiled-cell and native-kernel paths).

### Determinism and the seed

The seed is identity. All randomness enters through the declared
counter-based stream contract (Philox-class, `stochastic.rs`): the
seed maps to a local stream seed, and each step's standard Normal draw
Z comes from the SAME Box–Muller pair the `Normal(0,1)` sampler uses.
There is no ambient entropy and no hidden seed. Same seed ⟹
bit-identical trajectory; a different seed ⟹ a different trajectory.
A run without a legal seed refuses.

### Typed refusals

| Code | Meaning |
|---|---|
| `E-SIM-SEED` | seed missing, non-finite, negative, or ≥ 2⁶⁴ |
| `E-SIM-001` | non-finite drift/diffusion/state/step |
| `E-SIM-002` | non-positive step, or zero steps |
| `E-SIM-003` | step count exceeds the compute budget |

### Metamorphic laws (the language's honest checks)

- **Seed replay**: same seed ⟹ bit-identical trajectory.
- **Refinement (distributional)**: for constant μ, σ, halving h at
  fixed T doubles the trajectory length; the terminal VARIANCE
  approaches σ²T as h → 0 (strong-order closure), pinned in the
  kernel tests.
- **Zero noise**: σ ≡ 0 reduces both rules to the explicit Euler ODE
  `X' = μ(X)` (bit-identical between rules).
- **Itô vs Stratonovich**: for σ' ≠ 0 the rules differ under the same
  seed; for σ' = 0 they agree bit-for-bit.

## Control (reused unchanged)

The transfer-function, state-space DC-gain, and Routh–Hurwitz stability
cells execute through the existing control paths (`control.rs`), with
the same typed refusals `E-CONTROL-001..005`. The SDE drift and control
carriers share the ascending-polynomial law; no SDE-specific control
code exists.

## No-claim boundaries

Only scalar SDEs with polynomial drift/diffusion. Multi-dimensional
systems, general non-polynomial coefficients, adaptive stepping, and
strong/weak error estimation are named deferrals. Kernel-backed cells
are interpreted (local reference semantics); Rust-backend codegen for
kernel-backed cells is an explicit refusal today; a documented
no-claim boundary, never a silent fallback.

## Example

`language/examples/numerical/sde-control.emath` runs the Itô and
Stratonovich trajectories under one fixed seed and reuses the control
cells.
