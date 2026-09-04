# `std.statistics`; descriptive statistics and estimator contracts

Status: std-layer package (Phase 11),
implemented in `crates/emath-core/src/statistics.rs`. Descriptive
statistics are honest arithmetic over collections; inference machinery
(p-values, regression, portfolios) lives in PACKAGES, not core.

## Labeled estimates

Every statistic is an `Estimate { value, method, n }`; the number
PLUS the method that produced it and the sample size. A bare f64 with
no method label is not an admissible output of this package.

- `mean`; arithmetic mean, label `mean`.
- `variance(values, kind)`; the denominator is part of the label:
  `variance_sample` (n−1, Bessel) vs `variance_population` (n). Sample
  variance with n < 2 refuses typed (`E-STATS-3`), never a silent
  infinity.
- `median`; linear interpolation at p = 0.5 (middle element for odd
  n).
- `quantile(values, p)`; type-7 linear interpolation, `h = (n−1)·p`
  (the numpy default), label `quantile_type7`; probability outside
  [0, 1] refuses (`E-STATS-5`).
- `describe(values, name)`; name dispatch; unknown names refuse
  (`E-STATS-4`), and inference notions (`p_value`, …) are explicitly
  NOT descriptive statistics.

## Input honesty

Empty samples (`E-STATS-1`) and non-finite values (`E-STATS-2`) are
typed refusals naming the offending index; a silent NaN mean would be
a fabricated number. `DistributionSample::new` is the validated
boundary type.

## Estimator contracts

`EstimatorContract { estimator, target_parameter, method, assumptions,
bias, consistency }`: declared bias (`Unbiased` / `Biased{direction}` /
`Undeclared`) and consistency (`Consistent` / `Inconsistent` /
`Undeclared`) as inspectable, refutable DATA; not prose. `Undeclared`
is an honest option; silently claiming unbiasedness is not.

## "Significance" is never a silent output

`SignificanceVerdict::classify(p, alpha, method)` is the ONLY way to
produce a significance claim, and it returns a labeled verdict;
`SignificantAt{p, alpha, method}` or `NotSignificantAt{...}`. There is
deliberately no `is_significant() -> bool` anywhere in the module. The
boundary convention is declared: `p < alpha` is significant, `p ==
alpha` is NOT (strict comparison).

## No-claim boundaries

- No p-value computation, distributions, regression, or Bayesian
  machinery in core; packages only.
- No hypothesis-test orchestration; `classify` labels a p-value the
  caller already has.
- No weighting/robust estimators (trimmed/winsorized); follow-up
  slices.
- No `.emath` surface change: the language's existing vector `mean`
  builtin is untouched; this is the data-layer vocabulary those
  definitions feed.
