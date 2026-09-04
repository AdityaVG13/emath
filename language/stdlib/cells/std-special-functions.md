# `core::special_functions`; package contract (05 section 3.3 #1, Phase 11)

Status: **contracts, strict-f64 reference implementations, and `.emath`
reference-VM execution landed**.
Each function has a `<name>_error_bound` companion. Generated
`rust.library` artifacts embed the same strict-f64 evaluator and its
declared bounds, so the reference VM and generated code share one
implementation.

## Contract

| Function | Arguments | Domain (real carrier) | Branch behavior | Declared error bound |
|---|---|---|---|---|
| `Γ(z)` | `gamma(z)` | finite real; **poles at 0, −1, −2, …** (refused, named); reflection branch admits `z < 0.5` with `\|sin(πz)\| ≥ 1e-3` | reflection branch for `z < 0.5`; near-pole points refuse rather than return an exploding value | `1e-14` relative (direct and reflection, the reflection additionally amplified by `1/\|sin(πz)\|`) |
| `B(a, b)` | `beta(a, b)` | `a > 0, b > 0` |; | composed from Γ bounds (first-order) |
| `erf(x)` | `erf(x)` | all finite reals (entire) | odd: `erf(−x) = −erf(x)` | alternating-series remainder `\|t_{N+1}\|·2/√π` for `\|x\| < 4`; `1.55e-8` absolute for `\|x\| ≥ 4` (constant-1 tail certificate) |
| `ζ(s)` | `zeta(s)` | real `s > 1`; **simple pole at `s = 1`** (refused, named) |; | alternating η-tail `(N+1)^{−s}/\|1−2^{1−s}\|`; grows honestly as `s → 1⁺` |
| `W₀(z)` | `lambert_w0(z)` | real `z ≥ −1/e`; **principal branch named**; `z < −1/e` refuses naming the branch cut; boundary `W₀(−1/e) = −1` exact | principal (`W₀`), never implicit | residual certificate `\|we^w − z\|/(e^w·\|1+w\|)` |
| `K(m)` | `elliptic_k(m)` | parameter `m ∈ [0, 1)`; `K(1)` diverges (refused) |; | AGM bracket `b_N ≤ a_∞ ≤ a_N` propagated through `K = π/(2a)` |
| `E(m)` | `elliptic_e(m)` | parameter `m ∈ [0, 1)` |; | hypergeometric all-same-sign tail bound at the stopping index |
| `Π(n, m)` | `elliptic_pi(n, m)` |; |; | **no reference implementation yet**; refuses `NotImplemented` |

## Reference implementations

`crates/emath-core/src/special.rs`; std-only (no external crates):

- Γ: recurrence up to `w ≥ 12`, then the asymptotic Stirling expansion
  in the EXPONENT (`log Γ` Bernoulli series, truncated after the B14
  term; ≤1.9e-21 relative contribution at `w ≥ 12`), `exp`-corrected;
  reflection for `z < 0.5`. Chosen over Lanczos for derivable
  rational coefficients (no memorized magic constants).
- B: Γ ratio with first-order relative-bound composition.
- erf: Maclaurin series with the alternating-tail remainder as the
  bound; `|x| ≥ 4` uses the constant-1 certificate.
- ζ: Dirichlet-η acceleration (`ζ = η/(1 − 2^{1−s})`), alternating
  remainder `(N+1)^{−s}` as the bound.
- W₀: Halley iteration from a piecewise initial guess; the residual
  bound is computed from the final iterate.
- K: AGM (`K = π/(2·AGM(1, √(1−m)))`), bracket certified.
- E: hypergeometric series `₂F₁(−1/2, 1/2; 1; m)`, same-sign tail
  bound.

## Provider contract

`SpecialFunctionEvaluator::evaluate(function, args) ->
Result<Evaluated, DomainRefusal>` with `Evaluated { value,
error_bound }`. `StrictF64Reference` is the core implementation;
high-precision and interval-certified backends implement the same
trait behind this contract and never change core semantics.

## No-claim boundary

- Reference values are **labeled-error-bound**, not claimed
  correctly-rounded anywhere that is not proven so. The contract
  tests enforce the discipline mechanically: each declared bound must
  COVER the true deviation from independently-known reference
  constants; an under-stated bound is treated as the same lie as no
  bound.
- `Π(n, m)` is contract-only (refuses `NotImplemented`).
- Numeric profiles: this slice is strict-f64 only; `f16`…`f128`
  profile behavior is the provider contract's extension surface, not
  a core claim.
- The real-argument carriers above are the declared Phase 11 slice;
  complex arguments (e.g. `ζ(s)` for `Re(s) ≤ 1`, `W` on other
  branches, Γ near its poles) refuse with the carrier named rather
  than silently extending.
- `.emath` calls admit and execute in the strict-f64 reference VM and
  generated Rust. Neither path silently substitutes a different
  implementation.
