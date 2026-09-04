//! core::statistics — descriptive statistics and estimator contracts
//! (Phase 11).
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

/// Typed refusal: an empty sample where at least one value is required.
pub const E_STATS_EMPTY: &str = "E-STATS-1";
/// Typed refusal: a non-finite value entered a statistic.
pub const E_STATS_NONFINITE: &str = "E-STATS-2";
/// Typed refusal: sample variance with n < 2 (n−1 denominator is zero).
pub const E_STATS_SAMPLE_N: &str = "E-STATS-3";
/// Typed refusal: an unknown descriptive statistic name.
pub const E_STATS_NAME: &str = "E-STATS-4";
/// Typed refusal: a quantile probability outside [0, 1].
pub const E_STATS_PROB: &str = "E-STATS-5";

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

/// Variance denominator, honest and explicit: `Sample` uses n−1
/// (Bessel), `Population` uses n. They are different estimators with
/// different labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarianceKind {
    Sample,
    Population,
}

impl VarianceKind {
    fn method(self) -> &'static str {
        match self {
            Self::Sample => "variance_sample",
            Self::Population => "variance_population",
        }
    }

    fn denominator(self, n: usize) -> Result<f64, String> {
        match self {
            Self::Population => Ok(n as f64),
            Self::Sample => {
                if n < 2 {
                    return Err(format!(
                        "{E_STATS_SAMPLE_N}: sample variance needs n >= 2 (n-1 denominator is zero), got n = {n}"
                    ));
                }
                Ok((n - 1) as f64)
            }
        }
    }
}

fn validated(values: &[f64], what: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!(
            "{E_STATS_EMPTY}: {what} of an empty sample is not a statistic"
        ));
    }
    for (index, value) in values.iter().enumerate() {
        if !value.is_finite() {
            return Err(format!(
                "{E_STATS_NONFINITE}: {what} input index {index} is non-finite ({value:e}); \
                 a silent NaN statistic would be a fabricated number"
            ));
        }
    }
    Ok(())
}

/// Arithmetic mean.
pub fn mean(values: &[f64]) -> Result<Estimate, String> {
    validated(values, "mean")?;
    let total: f64 = values.iter().sum();
    Ok(Estimate {
        value: total / values.len() as f64,
        method: "mean",
        n: values.len(),
    })
}

/// Variance with the explicit denominator kind.
pub fn variance(values: &[f64], kind: VarianceKind) -> Result<Estimate, String> {
    validated(values, "variance")?;
    let n = values.len();
    let denominator = kind.denominator(n)?;
    let mu = values.iter().sum::<f64>() / n as f64;
    let squared = values
        .iter()
        .map(|value| (value - mu) * (value - mu))
        .sum::<f64>();
    Ok(Estimate {
        value: squared / denominator,
        method: kind.method(),
        n,
    })
}

/// Median: middle element for odd n, mean of the two middle elements
/// for even n (linear interpolation at p = 0.5).
pub fn median(values: &[f64]) -> Result<Estimate, String> {
    quantile(values, 0.5).map(|mut estimate| {
        estimate.method = "median";
        estimate
    })
}

/// Quantile by type-7 linear interpolation: with the sorted sample and
/// `h = (n−1)·p`, the result is `sorted[floor(h)] + frac(h) ·
/// (sorted[ceil(h)] − sorted[floor(h)])`. This is the numpy default
/// and the declared method label of this package.
pub fn quantile(values: &[f64], probability: f64) -> Result<Estimate, String> {
    validated(values, "quantile")?;
    if !(0.0..=1.0).contains(&probability) {
        return Err(format!(
            "{E_STATS_PROB}: quantile probability {probability} is outside [0, 1]"
        ));
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    let h = (n - 1) as f64 * probability;
    let lower = h.floor() as usize;
    let upper = h.ceil() as usize;
    let fraction = h - h.floor();
    let value = if lower == upper {
        sorted[lower]
    } else {
        sorted[lower] + fraction * (sorted[upper] - sorted[lower])
    };
    Ok(Estimate {
        value,
        method: "quantile_type7",
        n,
    })
}

/// Dispatch a descriptive statistic by name. Unknown names refuse
/// typed — in particular, `p_value` and every inference notion are NOT
/// descriptive statistics and are never silently computed here.
pub fn describe(values: &[f64], name: &str) -> Result<Estimate, String> {
    match name {
        "mean" => mean(values),
        "median" => median(values),
        "variance_sample" => variance(values, VarianceKind::Sample),
        "variance_population" => variance(values, VarianceKind::Population),
        other => Err(format!(
            "{E_STATS_NAME}: `{other}` is not a descriptive statistic; inference \
             (p-values, regression, significance) lives in packages, not core"
        )),
    }
}

/// Bias declaration of an estimator contract: data, not prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BiasDeclaration {
    /// E[E] = target exactly.
    Unbiased,
    /// Declared direction of the bias (honest about which way it lies).
    Biased { direction: BiasDirection },
    /// No claim made (also honest).
    Undeclared,
}

/// Direction of a declared bias.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiasDirection {
    Overestimates,
    Underestimates,
}

/// Consistency declaration of an estimator contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsistencyDeclaration {
    /// Converges to the target as n grows.
    Consistent,
    /// Does not converge (declared honestly).
    Inconsistent,
    /// No claim made.
    Undeclared,
}

/// An estimator contract: what the estimator targets, what it assumes,
/// and its declared bias/consistency. The declaration is DATA — a
/// reviewer (or a later law check) can refute it against evidence;
/// nothing here silently claims significance or correctness.
#[derive(Clone, Debug, PartialEq)]
pub struct EstimatorContract {
    /// Estimator identifier (e.g. `arithmetic_mean`).
    pub estimator: String,
    /// The population parameter it targets.
    pub target_parameter: String,
    /// Method label carried onto every estimate this estimator makes.
    pub method: String,
    /// Declared assumptions (e.g. `iid`, `finite variance`).
    pub assumptions: Vec<String>,
    /// Declared bias.
    pub bias: BiasDeclaration,
    /// Declared consistency.
    pub consistency: ConsistencyDeclaration,
}

/// A labeled significance verdict. Constructed only through
/// [`SignificanceVerdict::classify`] with an explicit p, alpha, and
/// method — the package never emits a bare "significant" bool.
#[derive(Clone, Debug, PartialEq)]
pub enum SignificanceVerdict {
    /// p < alpha: significant AT THIS alpha, labeled with all three.
    SignificantAt { p: f64, alpha: f64, method: String },
    /// p >= alpha: not significant at this alpha, labeled likewise.
    NotSignificantAt { p: f64, alpha: f64, method: String },
}

impl SignificanceVerdict {
    /// Classify with the STRICT comparison `p < alpha`: a boundary
    /// `p == alpha` is not significant (the convention is declared
    /// here, not discovered later).
    pub fn classify(p: f64, alpha: f64, method: &str) -> Self {
        if p < alpha {
            Self::SignificantAt {
                p,
                alpha,
                method: method.to_string(),
            }
        } else {
            Self::NotSignificantAt {
                p,
                alpha,
                method: method.to_string(),
            }
        }
    }
}

/// A validated sample with a name: the boundary type that guarantees
/// the statistics below only ever see finite values.
#[derive(Clone, Debug, PartialEq)]
pub struct DistributionSample {
    name: String,
    values: Vec<f64>,
}

impl DistributionSample {
    /// Construct, refusing non-finite observations at the boundary.
    pub fn new(name: String, values: Vec<f64>) -> Result<Self, String> {
        validated(&values, &format!("sample `{name}`"))?;
        Ok(Self { name, values })
    }

    /// Sample name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Validated values.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }
}
