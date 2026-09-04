# `std.probability.markov`; finite-state Markov chain slice

Status: **law pack + example landed** (Wave 16, Tier A
measure/probability foundations, selective). Discrete-time Markov
chains on finite state spaces, evaluated as exact deterministic
arithmetic on the declared carrier. No sampling, no mixing-time claim.

## Contract (finite state space, f64 carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `MarkovChainStepDistribution` | law, `p, a, b → next_p` | one-step two-state update `d'[0] = d[0](1−a) + d[1]b` for the row-stochastic chain with off-diagonals `a` (0→1), `b` (1→0) | transition probabilities and `p` must lie in `[0, 1]`; dyadic test values are exact at the f64 carrier |
| `MarkovChainTwoStepDistribution` | law, `p, a, b → second_p` | the one-step map iterated twice | same admission box as the one-step law |
| `MarkovChainStationaryTwoState` | law, `a, b → stationary_p` | closed-form stationary mass `π₀ = b/(a+b)` | refuses the degenerate chain `a + b = 0` by assumption; the stationary point is algebraic, no convergence claim is attached |
| `MarkovChainStepVector` | law, `d, P → d_next` | `d'[j] = Σᵢ d[i]·P[i][j]` via the transition contraction `einsum("j,ji->i", d, P)` | `P` row-stochastic by assumption; the contraction is the admitted einsum surface, not a new core op |

## Executable surface

- Laws: `language/stdlib/laws/probability-markov-chains.emath`
  (package `probability.markov.laws`).
- Example: `language/examples/probability/markov_chain_evolution.emath`
  — six-step relaxation of a two-state chain from state 0 toward
  `(2/3, 1/3)`, split into one function per claim (Phase 1 tests
  observe a single evaluate target). The one-, four-, and six-step
  distributions and mass conservation `Σd = 1` are dyadic and asserted
  exactly; the closed-form stationary point `2/3` is not exactly
  representable at the f64 carrier and is displayed, not asserted.

## No-claim boundaries

- The f64 carrier is a declared approximation layer; dyadic test values
  are exact, everything else is floating-point arithmetic with the
  standard validation tolerance.
- No mixing time, no spectral-gap statement, no CLT-for-chains: a
  convergence certificate is a different contract.
- General n-state stationarity (power iteration, eigen-solve) is a
  declared follow-up; only the two-state closed form and the general
  one-step contraction are claimed.
- Continuous-time chains (generator `Q`, Kolmogorov equations) are not
  admitted here; that is the Markov-processes world, not this cell.
