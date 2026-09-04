# `std.probability.montecarlo`; deterministic seeded estimation slice

Status: **law pack + example landed** (Wave 16, Tier A
measure/probability foundations, selective). Monte Carlo estimation as
deterministic arithmetic over a declared seeded stream: fixed seed plus
fixed draw count fixes the sample set, so every estimate is
bit-identical on every run. No random experiment is performed.

## Contract (fixed sample vector, f64 carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `MonteCarloMeanEstimate` | law, `f_values → estimate` | sample mean `(1/n)Σᵢ f(xᵢ)` via the admitted `mean` builtin | the sample set is fixed by the stream contract; no unbiasedness or CLT claim is attached to the number |
| `MonteCarloSecondMoment` | law, `x, n → mean_square` | `(1/n)Σᵢ xᵢ²` via the contraction `einsum("i,i->", x, x)` | `n > 0` required; the contraction is the admitted einsum surface, not a new core op |

## Stream coupling

The sample vector comes from the seeded stream contract
(`normal_sample`, `uniform_sample`, `bernoulli_sample`; see
`language/examples/probability/seeded_sampling.emath`): the explicit
seed and declared draw count determine the samples bit-for-bit, and a
same-seed replay reproduces them exactly. This cell adds the estimator
side: what number the fixed sample set produces.

## Executable surface

- Laws: `language/stdlib/laws/probability-monte-carlo.emath`
  (package `probability.montecarlo.laws`).
- Example: `language/examples/probability/monte_carlo_quadrature.emath`
  — `E[X] → 1/2` and `E[X²] → 1/3` for `X ~ Uniform(0, 1)` evaluated on
  a declared 8-draw dyadic sample slice. The estimates are exact at the
  f64 carrier; truth gaps are honest finite-set discrepancies.

## No-claim boundaries

- Determinism class: deterministic given the seed; the estimates are
  reproducible numbers, not random variables in this cell.
- No variance reduction theory, no confidence intervals, no
  convergence-rate theorem, and no importance-sampling weights: those
  are separate contracts.
- The estimator bias/consistency vocabulary of `std.statistics`
  (`EstimatorContract`) is deliberately not attached here; coupling the
  two is a declared follow-up, not a silent assumption.
- Quasi-Monte Carlo and stratified grids are not admitted; only the
  plain sample-mean slice on the seeded stream.
