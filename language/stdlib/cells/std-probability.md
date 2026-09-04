# `core::probability`; information-theory slice (B22; B10 world-gated)

Status: **information-theory contracts + reference implementations
landed** . The names below are
contract-first: the sema call table does not admit them yet, so
`.emath` models calling them refuse with the standard unknown-function
diagnostic until the admission-table follow-up (special-functions seam
pattern). B10 (`x: Random<Real> ~ Normal(0, 1)`) is the
world-gated follow-up; see the boundary section.

## Contract (discrete carrier, f64, unit = BITS)

| Function | Signature | Semantics | Boundaries |
|---|---|---|---|
| `entropy(p)` | `&[f64] → f64` | `H = −Σ p_i log2 p_i`, **bits** (Shannon's unit) | `0·log2 0 := 0` (zero rows carry no information, never NaN); mass must total 1 within `1e-9`; **never silently renormalized** |
| `entropy_nats(p)` | `&[f64] → f64` | same in nats (`−Σ p_i ln p_i`) | the base is a DECLARED function distinction, never a parameter or an inference; `bits = nats / ln 2` |
| `kl_divergence(p, q)` | `(&[f64], &[f64]) → f64` | `D_KL(P‖Q) = Σ p_i log2(p_i/q_i)`, bits | row-wise pairing (`|P| = |Q|` enforced); `p_i = 0` rows contribute 0; a **support violation** (`p_i > 0`, `q_i = 0`) makes KL `+∞` and REFUSES; `+∞` is not a finite value to hand back |
| `mutual_information(joint)` | `&[Vec<f64>] → f64` | `I(X;Y) = Σ p(x,y) log2(p(x,y)/(p_X(x) p_Y(y)))`, bits, marginals from the joint | rectangular table (ragged refuses by name), mass 1; zero cells contribute 0; **MI ≥ 0 is a theorem about the math, not a clamp**; the honest sum is computed |
| `entropy_differential(…)` | refuses | **criterion-4 type distinction**: a density integral over a continuous carrier is NOT a mass sum | refuses `NotImplemented` naming the measure world (giry-probability); the discrete sum is never silently reused for densities |

## Reference implementations

`crates/emath-core/src/probability.rs`; std-only. Validation is
shared (`validate_mass`): non-empty, finite, non-negative, mass 1
within `1e-9`. All functions are deterministic; no randomness source
is consulted anywhere in this cell.

## B10 boundary (random variables; NOT landed)

`x: Random<Real> ~ Normal(0, 1)` requires (a) a `Random<T>` type
carrier and (b) the giry-probability world class
(measure-theoretic probability); the own WORLD-DEPENDENT flag.
The `~` glyph ownership is already settled (C7/X5: `~` = distribution
tag, `~~` = asymptotics, `not`/reserved `!` = negation) and is pinned
by the surface tests. The random-variable input row needs a
`FieldDecl` distribution annotation; `tree.rs` currently carries the
reactions lane's in-flight work, so the parse landing is sequenced
behind it. Both are declared follow-ups, not silent omissions.

## No-claim boundaries

- The f64 carrier is a declared approximation layer: probability
  ARITHMETIC here is floating-point with declared validation
  tolerance, not exact-rational probability. An exact measure algebra
  is a different contract.
- KL's finiteness claim holds only on the pinned support condition; a
  q with an empty support cell under a positive p is a refusal, not a
  large number.
- MI non-negativity is not enforced by clamping: a pin checks the
  computed value directly; a negative output would be an
  implementation bug the pins must catch.
- `entropy`, `kl_divergence`, and `mutual_information` are library
  contracts rather than admitted `.emath` calls. Seeded distribution
  sampling is demonstrated in
  `language/examples/probability/seeded_sampling.emath`.
