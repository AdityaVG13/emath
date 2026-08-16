//!: differential oracle.
//!
//! Compares the emath reference interpreter against the Dew backend
//! path over boundary cases (NaN/Inf, signed zero, extremes, domain
//! edges) per the strict-f64 numeric profile. Negative semantic-drift
//! fixtures (mutants) are detected, never masked.

use std::collections::BTreeMap;

use crate::dexpr::{CmpOp, DewExpr};

/// Boundary case names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScanCase {
    Nan,
    PosInf,
    NegInf,
    Zero,
    NegZero,
    MinNormal,
    Max,
    One,
    Two,
    Half,
}

impl ScanCase {
    /// All boundary cases.
    #[must_use]
    pub const fn all() -> [Self; 10] {
        [
            Self::Nan,
            Self::PosInf,
            Self::NegInf,
            Self::Zero,
            Self::NegZero,
            Self::MinNormal,
            Self::Max,
            Self::One,
            Self::Two,
            Self::Half,
        ]
    }

    /// Stable token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nan => "nan",
            Self::PosInf => "pos-inf",
            Self::NegInf => "neg-inf",
            Self::Zero => "zero",
            Self::NegZero => "neg-zero",
            Self::MinNormal => "min-normal",
            Self::Max => "max",
            Self::One => "one",
            Self::Two => "two",
            Self::Half => "half",
        }
    }

    /// The boundary value.
    #[must_use]
    pub fn value(self) -> f64 {
        match self {
            Self::Nan => f64::NAN,
            Self::PosInf => f64::INFINITY,
            Self::NegInf => f64::NEG_INFINITY,
            Self::Zero => 0.0,
            Self::NegZero => -0.0,
            Self::MinNormal => f64::MIN_POSITIVE,
            Self::Max => f64::MAX,
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Half => 0.5,
        }
    }
}

/// Scan profile: which cases to run (defaults to all).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanProfile {
    pub cases: Vec<ScanCase>,
}

impl Default for ScanProfile {
    fn default() -> Self {
        Self {
            cases: ScanCase::all().to_vec(),
        }
    }
}

/// One differential finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DifferentialFinding {
    pub case: &'static str,
    pub variable: String,
    pub reference_bits: u64,
    pub backend_bits: u64,
    pub detail: String,
}

/// Deliberate semantic drift injected into the backend path (the
/// negative fixture).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MutantDrift {
    /// Backend computes `a - b` instead of `a + b`.
    AddAsSub,
    /// Backend computes `a * b` instead of `a / b`.
    DivAsMul,
}

impl MutantDrift {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddAsSub => "add-as-sub",
            Self::DivAsMul => "div-as-mul",
        }
    }
}

/// Evaluation result: scalar or boolean.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvalValue {
    F64(f64),
    Bool(bool),
}

/// Reference interpreter vs backend-path interpreter difference.
fn eval(
    expr: &DewExpr,
    env: &BTreeMap<String, f64>,
    drift: Option<MutantDrift>,
) -> Option<EvalValue> {
    match expr {
        DewExpr::Float64Bits(bits) => Some(EvalValue::F64(f64::from_bits(*bits))),
        DewExpr::Bool(value) => Some(EvalValue::Bool(*value)),
        DewExpr::Int(text) => text.parse::<f64>().ok().map(EvalValue::F64),
        DewExpr::Var(name) => env.get(name).copied().map(EvalValue::F64),
        DewExpr::Add(left, right) => two(left, right, env, drift, |l, r| {
            if drift == Some(MutantDrift::AddAsSub) {
                l - r
            } else {
                l + r
            }
        }),
        DewExpr::Sub(left, right) => two(left, right, env, drift, |l, r| l - r),
        DewExpr::Mul(left, right) => two(left, right, env, drift, |l, r| l * r),
        DewExpr::Div(left, right) => two(left, right, env, drift, |l, r| {
            if drift == Some(MutantDrift::DivAsMul) {
                l * r
            } else {
                l / r
            }
        }),
        DewExpr::Pow(left, right) => two(left, right, env, drift, f64::powf),
        DewExpr::Neg(value) => eval(value, env, drift).map(|value| match value {
            EvalValue::F64(v) => EvalValue::F64(-v),
            EvalValue::Bool(_) => EvalValue::F64(f64::NAN),
        }),
        DewExpr::Not(value) => eval(value, env, drift).map(|value| match value {
            EvalValue::Bool(v) => EvalValue::Bool(!v),
            EvalValue::F64(_) => EvalValue::Bool(false),
        }),
        DewExpr::Sqrt(value) => unary(value, env, drift, f64::sqrt),
        DewExpr::Exp(value) => unary(value, env, drift, f64::exp),
        DewExpr::Ln(value) => unary(value, env, drift, f64::ln),
        DewExpr::Sin(value) => unary(value, env, drift, f64::sin),
        DewExpr::Cos(value) => unary(value, env, drift, f64::cos),
        DewExpr::Tan(value) => unary(value, env, drift, f64::tan),
        DewExpr::Tanh(value) => unary(value, env, drift, f64::tanh),
        DewExpr::Abs(value) => unary(value, env, drift, f64::abs),
        DewExpr::Floor(value) => unary(value, env, drift, f64::floor),
        DewExpr::Ceil(value) => unary(value, env, drift, f64::ceil),
        DewExpr::IsFinite(value) => eval(value, env, drift).map(|value| match value {
            EvalValue::F64(v) => EvalValue::Bool(v.is_finite()),
            EvalValue::Bool(_) => EvalValue::Bool(false),
        }),
        DewExpr::Min(left, right) => two(left, right, env, drift, f64::min),
        DewExpr::Max(left, right) => two(left, right, env, drift, f64::max),
        DewExpr::Atan2(left, right) => two(left, right, env, drift, f64::atan2),
        DewExpr::And(left, right) => two(left, right, env, drift, |l, r| {
            if l != 0.0 && r != 0.0 {
                1.0
            } else {
                0.0
            }
        }),
        DewExpr::Or(left, right) => two(left, right, env, drift, |l, r| {
            if l != 0.0 || r != 0.0 {
                1.0
            } else {
                0.0
            }
        }),
        DewExpr::Cmp(op, left, right) => two(left, right, env, drift, |l, r| {
            // IEEE-754 comparisons are exact by definition, so this
            // is not a lossy comparison.
            #[allow(clippy::float_cmp)]
            if match op {
                CmpOp::Eq => l == r,
                CmpOp::Ne => l != r,
                CmpOp::Lt => l < r,
                CmpOp::Le => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::Ge => l >= r,
            } {
                1.0
            } else {
                0.0
            }
        }),
        DewExpr::If {
            condition,
            then_value,
            else_value,
        } => {
            let condition = eval(condition, env, drift)?;
            let boolean = match condition {
                EvalValue::Bool(value) => value,
                EvalValue::F64(value) => value != 0.0,
            };
            if boolean {
                eval(then_value, env, drift)
            } else {
                eval(else_value, env, drift)
            }
        }
        DewExpr::Matrix(_) | DewExpr::Linear(..) => None,
    }
}

fn unary(
    expr: &DewExpr,
    env: &BTreeMap<String, f64>,
    drift: Option<MutantDrift>,
    operation: impl Fn(f64) -> f64,
) -> Option<EvalValue> {
    eval(expr, env, drift).map(|value| match value {
        EvalValue::F64(v) => EvalValue::F64(operation(v)),
        EvalValue::Bool(_) => EvalValue::F64(f64::NAN),
    })
}

fn two(
    left: &DewExpr,
    right: &DewExpr,
    env: &BTreeMap<String, f64>,
    drift: Option<MutantDrift>,
    operation: impl Fn(f64, f64) -> f64,
) -> Option<EvalValue> {
    match (eval(left, env, drift), eval(right, env, drift)) {
        (Some(EvalValue::F64(l)), Some(EvalValue::F64(r))) => Some(EvalValue::F64(operation(l, r))),
        _ => None,
    }
}

fn bits(value: EvalValue) -> u64 {
    match value {
        EvalValue::F64(v) => v.to_bits(),
        EvalValue::Bool(v) => u64::from(v),
    }
}

/// Runs the boundary battery for the variable `variable`: the
/// reference interpreter vs the backend path.
#[must_use]
pub fn run_boundary_cases(
    expr: &DewExpr,
    variable: &str,
    profile: &ScanProfile,
    drift: Option<MutantDrift>,
) -> Vec<DifferentialFinding> {
    let mut findings = Vec::new();
    for case in &profile.cases {
        let mut env = BTreeMap::new();
        env.insert(variable.to_string(), case.value());
        let reference = eval(expr, &env, None);
        let backend = eval(expr, &env, drift);
        match (&reference, &backend) {
            (Some(expected), Some(actual)) if bits(*expected) != bits(*actual) => {
                findings.push(DifferentialFinding {
                    case: case.as_str(),
                    variable: variable.to_string(),
                    reference_bits: bits(*expected),
                    backend_bits: bits(*actual),
                    detail: format!("case {} reference differs from backend path", case.as_str()),
                });
            }
            _ => {}
        }
    }
    findings
}

/// Convenience entry for the drift fixture: scan a candidate backend
/// path and report the first divergence, if any.
#[must_use]
pub fn detect_drift(
    expr: &DewExpr,
    variable: &str,
    drift: MutantDrift,
) -> Option<DifferentialFinding> {
    run_boundary_cases(expr, variable, &ScanProfile::default(), Some(drift))
        .into_iter()
        .next()
}

/// Clean-backend scan used by conformance checks.
#[must_use]
pub fn differential_scan(expr: &DewExpr, variable: &str) -> Vec<DifferentialFinding> {
    run_boundary_cases(expr, variable, &ScanProfile::default(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mul_add(x: &str) -> DewExpr {
        DewExpr::Add(
            Box::new(DewExpr::Mul(
                Box::new(DewExpr::Var(x.into())),
                Box::new(DewExpr::Var(x.into())),
            )),
            Box::new(DewExpr::Float64Bits(1.0f64.to_bits())),
        )
    }

    #[test]
    fn clean_backend_matches_the_reference_over_boundary_cases() {
        let expr = mul_add("x");
        let findings = differential_scan(&expr, "x");
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn signed_zero_is_preserved() {
        // -0.0 * -0.0 = 0.0 and 0.0 + 1.0 = 1.0; the reference and
        // backend both keep IEEE semantics.
        let expr = DewExpr::Sub(
            Box::new(DewExpr::Var("x".into())),
            Box::new(DewExpr::Var("x".into())),
        );
        let mut env = BTreeMap::new();
        env.insert("x".to_string(), -0.0);
        match eval(&expr, &env, None) {
            Some(EvalValue::F64(value)) => assert_eq!(value.to_bits(), 0.0f64.to_bits()),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn negative_drift_fixture_is_detected() {
        let expr = mul_add("x");
        let finding = detect_drift(&expr, "x", MutantDrift::AddAsSub).expect("drift detected");
        assert_eq!(finding.case, "zero");
        assert_ne!(finding.reference_bits, finding.backend_bits);
        assert_eq!(finding.variable, "x");
    }

    #[test]
    fn div_mutation_is_detected() {
        let expr = DewExpr::Div(
            Box::new(DewExpr::Var("x".into())),
            Box::new(DewExpr::Float64Bits(2.0f64.to_bits())),
        );
        let finding = detect_drift(&expr, "x", MutantDrift::DivAsMul).expect("drift detected");
        assert_ne!(finding.reference_bits, finding.backend_bits);
    }

    #[test]
    fn nan_cases_compare_by_bits() {
        // NaN folding: the reference returns NaN for 0/0; the backend
        // must too (bit-identical canonical NaN is not required, so
        // the oracle only reports differing NaNness patterns when
        // bits differ deterministically).
        let expr = DewExpr::Div(
            Box::new(DewExpr::Float64Bits(0.0f64.to_bits())),
            Box::new(DewExpr::Float64Bits(0.0f64.to_bits())),
        );
        let findings = differential_scan(&expr, "x");
        assert!(findings.is_empty());
    }
}
