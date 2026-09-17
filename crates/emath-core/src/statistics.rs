//! core::statistics — descriptive statistics and estimator contracts.
//!
//! Honesty doctrine of this package:
//!
//! - **An estimate is labeled.** Every statistic is an
//!   [`Estimate`](struct.Estimate.html): value + method + n. A bare f64
//!   with no method label is not an admissible output.
//! - **The denominator is part of the label.** `sample` (n−1) and
//!   `population` (n) variance are distinct named methods, never
//!   conflated.
//! - **Quantiles declare their method**: type-7 linear interpolation
//!   (the numpy default), position `h = (n−1)·p`.
//! - **Inputs are validated.** Empty samples and non-finite values are
//!   typed refusals (`E-STATS-*`), never a silent NaN mean.
//! - **Estimator contracts declare bias and consistency** as data
//!   (inspectable, refutable), not prose.
//! - **"Significance" is never a silent output**: a verdict is a
//!   labeled claim carrying p, alpha, and method, constructed only
//!   through an explicit call. There is deliberately no
//!   `is_significant() -> bool` on this module.
//!
//! Inference machinery (p-value computation, regression, portfolios)
//! lives in packages, not core — this is the descriptive layer and the
//! contract vocabulary.
//!
//! Leaves are selected by capsule kernel ids (`finite-average`,
//! `type7-middle-order-statistic`, `centered-square-n-minus-one`,
//! `centered-square-n`, `type7-order-statistic`). Meaning and FeatureIDs
//! live in `language/spec/capabilities/probability/probability-statistics.emath`.

/// A labeled estimate: the number PLUS the method that produced it and
/// the sample size it was computed over.
#[derive(Clone, Debug, PartialEq)]
pub struct Estimate {
    /// The statistic's value.
    pub value: f64,
    /// Stable method label (e.g. `variance_sample`).
    pub method: &'static str,
    /// Sample size the statistic was computed over.
    pub n: usize,
}

