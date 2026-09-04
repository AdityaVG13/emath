# `std.probability.bayes`; discrete grid inference slice

Status: **law pack + example landed** (Wave 16, Tier A
measure/probability foundations, selective). Bayesian inference on
finite hypothesis grids: total probability, the discrete posterior
update, and the odds form, evaluated as deterministic arithmetic on the
declared carrier. No prior doctrine, no decision rule, no sampler.

## Contract (finite hypothesis grid, f64 carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `LawOfTotalProbability` | law, `prior, likelihood → evidence` | `P(D) = Σᵢ P(Hᵢ)·P(D\|Hᵢ)` | masses total one by assumption; a zero evidence is refused upstream by the posterior law's `positive_probability` requirement |
| `DiscreteBayesPosterior` | law, `prior, likelihood → evidence, posterior` | `P(Hᵢ\|D) = P(Hᵢ)P(D\|Hᵢ)/P(D)` | positive evidence required; dyadic test values are exact at the f64 carrier |
| `BayesOddsUpdate` | law, `prior_odds, likelihood_ratio → posterior_odds` | odds form: `odds(H\|D) = odds(H)·LR` | strictly positive inputs required; the multiplicative form is the whole claim |

## Elementwise grid form

The n-hypothesis update is the elementwise contraction
`einsum("i,i->i", prior, likelihood)` renormalized by the scalar
evidence. The example computes the three-hypothesis posterior both
ways (indexed components and the einsum form) and asserts the two
grids agree exactly, so the contraction carries no new semantics.

## Executable surface

- Laws: `language/stdlib/laws/probability-bayes-grid.emath`
  (package `probability.bayes.laws`).
- Example: `language/examples/probability/bayes_grid_posterior.emath`
  — two-hypothesis update in exact dyadic arithmetic
  (`posterior = [0.75, 0.25]`), then a three-hypothesis grid with
  `prior = [0.25, 0.5, 0.25]` and dyadic likelihoods
  `[0.5, 0.25, 0.25]`: evidence `0.3125`, posterior
  `[0.4, 0.4, 0.2]` asserted exactly, plus an exact
  component-form/einsum-form agreement check.

## No-claim boundaries

- The f64 carrier is a declared approximation layer: probability
  arithmetic here is floating-point with dyadic exactness on the test
  values, not an exact rational measure algebra.
- Continuous priors/likelihoods, density normalization, and MCMC are
  named deferrals; the `~` distribution-tag world (B10 /
  giry-probability) owns that surface, per
  `cells/std-probability.md`.
- No decision theory is attached: `BayesOddsUpdate` produces a number
  under declared positivity, not a decision rule.
- Conjugate-family shortcuts (Beta-Binomial etc.) are not admitted; a
  conjugacy cell would be its own contract.
