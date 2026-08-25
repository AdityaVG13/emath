//! SG-13 finite analogue binders: budgeted numeric stand-ins for the
//! conventional binder kinds, emitting a deterministic receipt.
//!
//! Sum/product fold integer ranges; integral is composite trapezoid;
//! derivative is central difference; limit samples a monotone right
//! dyadic approach with verdict [`AnalogueVerdict::NoClaim`]. No
//! continuum claim. Determinism: IEEE-754 binary64, fixed ops, values
//! as bits; budget excluded from [`analogue_id`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_term::{SymbolId, Term, VariableId};
use emath_world_ir::fnv1a64;

use crate::binder::{BinderBudget, BinderKind, BinderTerm};

/// Finite-analogue schema id for artifacts and receipts.
pub const ANALOGUE_SCHEMA: &str = "emath.analogue";
/// Finite-analogue schema version. Bump on any change to a rule, the
/// canonical request encoding, or the receipt layout; consumers refuse
/// versions they do not know.
pub const ANALOGUE_VERSION: u32 = 1;
/// Limit-sampling receipts always carry this verdict string. Samples are
/// evidence, not a limit proof; the verdict never claims convergence.
pub const ANALOGUE_NO_CLAIM: &str = "no-claim";

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), AnalogueError> {
    if version == ANALOGUE_VERSION {
        Ok(())
    } else {
        Err(AnalogueError::UnknownVersion { version })
    }
}

/// Typed refusals for finite-analogue evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalogueError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// The evaluation needed more samples than the budget allows.
    BudgetExceeded {
        /// Budget limit that was exhausted.
        limit: u32,
    },
    /// Domain failed a well-formedness check (`a>b`, `n=0`, `h<=0`,
    /// non-finite bounds, or a kind/domain mismatch).
    InvalidDomain {
        /// Stable reason token.
        reason: &'static str,
    },
    /// Binder kind this module does not evaluate (custom names, or a
    /// nested binder body).
    UnsupportedKind {
        /// Binder kind name.
        kind: String,
    },
    /// Strict-f64 evaluation of the body failed.
    EvaluationFailed {
        /// Stable reason token.
        reason: &'static str,
    },
}

/// Domain a finite analogue ranges over. Distinct from
/// [`crate::BinderDomain`]: analogues need real bounds, a step, or a
/// sample count, not just an integer range or a symbolic anchor.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalogueDomain {
    /// Inclusive integer range for bounded sum/product folds.
    IntegerRange {
        /// Inclusive lower bound.
        lower: i64,
        /// Inclusive upper bound.
        upper: i64,
    },
    /// Closed interval `[lower, upper]` with `n` trapezoid subintervals.
    Interval {
        /// Left endpoint.
        lower: f64,
        /// Right endpoint.
        upper: f64,
        /// Number of subintervals; must be ≥ 1.
        n: u32,
    },
    /// Central-difference stencil at `point` with step `h`.
    Difference {
        /// Evaluation point.
        point: f64,
        /// Positive step.
        h: f64,
    },
    /// Right-hand monotone approach toward `point` with `samples` terms.
    Approach {
        /// Limit point (never evaluated; the sequence stays strictly to
        /// the right).
        point: f64,
        /// Number of samples; must be ≥ 1.
        samples: u32,
    },
}

impl AnalogueDomain {
    fn write_canonical(&self, out: &mut String) {
        match self {
            Self::IntegerRange { lower, upper } => {
                let _ = write!(out, "range({lower},{upper})");
            }
            Self::Interval { lower, upper, n } => {
                let _ = write!(
                    out,
                    "interval({:016x},{:016x},{n})",
                    lower.to_bits(),
                    upper.to_bits()
                );
            }
            Self::Difference { point, h } => {
                let _ = write!(
                    out,
                    "difference({:016x},{:016x})",
                    point.to_bits(),
                    h.to_bits()
                );
            }
            Self::Approach { point, samples } => {
                let _ = write!(out, "approach({:016x},{samples})", point.to_bits());
            }
        }
    }

    fn canonical(&self) -> String {
        let mut out = String::new();
        self.write_canonical(&mut out);
        out
    }
}

/// One finite-analogue request: a binder kind, a numeric domain, a
/// budget, and a body in the SG-10 [`BinderTerm`] shape.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogueRequest {
    /// Binder kind to analogue.
    pub kind: BinderKind,
    /// Numeric domain.
    pub domain: AnalogueDomain,
    /// Sample/instantiation ceiling. Excluded from [`analogue_id`].
    pub budget: BinderBudget,
    /// Bound variable substituted with each sample point.
    pub bound: VariableId,
    /// Body evaluated as a strict-f64 function of `bound`.
    pub body: BinderTerm,
}

/// Receipt verdict. Limit sampling is hard-wired to [`Self::NoClaim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalogueVerdict {
    /// A finite analogue value was computed. Not a continuum claim.
    Computed,
    /// Samples recorded; the analogue never claims a limit exists.
    NoClaim,
}

impl AnalogueVerdict {
    /// Canonical verdict token.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Computed => "computed",
            Self::NoClaim => ANALOGUE_NO_CLAIM,
        }
    }
}

/// One sampled `(x, f(x))` pair, stored as IEEE bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalogueSample {
    /// Sample abscissa bits.
    pub x_bits: u64,
    /// Sample ordinate bits.
    pub fx_bits: u64,
}

/// Deterministic machine-readable analogue receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// FNV-1a64 of the versioned canonical request.
    pub request_id: u64,
    /// Canonical kind name.
    pub kind: String,
    /// Canonical domain encoding.
    pub domain: String,
    /// Rule that produced the numbers.
    pub rule: &'static str,
    /// Verdict; [`AnalogueVerdict::NoClaim`] for limit sampling.
    pub verdict: AnalogueVerdict,
    /// Budget ceiling that was in force.
    pub budget_limit: u32,
    /// Evaluations actually performed.
    pub budget_spent: u32,
    /// Result bits when the verdict is [`AnalogueVerdict::Computed`].
    pub value_bits: Option<u64>,
    /// Sampled points (quadrature nodes, difference stencil, limit
    /// approach, or fold abscissae).
    pub samples: Vec<AnalogueSample>,
    /// Running fold values as `f64` bits (sum/product only).
    pub partials: Vec<u64>,
}

impl AnalogueReceipt {
    /// BTreeMap-ordered JSON, byte-identical across runs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert("budget_limit", Json::Number(self.budget_limit.to_string()));
        root.insert("budget_spent", Json::Number(self.budget_spent.to_string()));
        root.insert("domain", Json::Str(self.domain.clone()));
        root.insert("kind", Json::Str(self.kind.clone()));
        root.insert(
            "partials",
            Json::Array(
                self.partials
                    .iter()
                    .map(|bits| Json::Str(format!("{bits:016x}")))
                    .collect(),
            ),
        );
        root.insert("request_id", Json::Str(format!("{:016x}", self.request_id)));
        root.insert("rule", Json::Str(self.rule.to_string()));
        root.insert(
            "samples",
            Json::Array(self.samples.iter().map(sample_json).collect()),
        );
        root.insert("schema", Json::Str(ANALOGUE_SCHEMA.to_string()));
        root.insert(
            "value",
            match self.value_bits {
                Some(bits) => Json::Str(format!("{bits:016x}")),
                None => Json::Null,
            },
        );
        root.insert("verdict", Json::Str(self.verdict.canonical().to_string()));
        root.insert("version", Json::Number(self.version.to_string()));
        emit_object(&root)
    }
}

/// Alpha-style analogue identity: FNV-1a64 over the versioned canonical
/// request. The budget is excluded.
#[must_use]
pub fn analogue_id(request: &AnalogueRequest) -> u64 {
    fnv1a64(
        format!(
            "{ANALOGUE_SCHEMA}.v{ANALOGUE_VERSION}:{}",
            request.canonical()
        )
        .as_bytes(),
    )
}

impl AnalogueRequest {
    /// Canonical request text (budget omitted).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = write!(out, "analogue({},", escape(self.kind.canonical_name()));
        self.domain.write_canonical(&mut out);
        let _ = write!(out, ",{},{})", escape(&self.bound.0), self.body.canonical());
        out
    }

    /// Evaluate `body` as a strict-f64 function of `bound`.
    pub fn evaluate(&self) -> Result<AnalogueReceipt, AnalogueError> {
        self.evaluate_with(|x| eval_numeric(&self.body, &self.bound, x))
    }

    /// Evaluate against an explicit closure. The receipt still echoes
    /// this request's term (identity is over the term, not the closure).
    pub fn evaluate_with<F>(&self, eval: F) -> Result<AnalogueReceipt, AnalogueError>
    where
        F: Fn(f64) -> Result<f64, AnalogueError>,
    {
        check_version(ANALOGUE_VERSION)?;
        match &self.kind {
            BinderKind::Sum => fold(self, &eval, FoldOp::Sum),
            BinderKind::Product => fold(self, &eval, FoldOp::Product),
            BinderKind::Integral => quadrature(self, &eval),
            BinderKind::Derivative => difference(self, &eval),
            BinderKind::Limit => limit_sample(self, &eval),
            BinderKind::Custom(name) => Err(AnalogueError::UnsupportedKind { kind: name.clone() }),
        }
    }
}

#[derive(Clone, Copy)]
enum FoldOp {
    Sum,
    Product,
}

impl FoldOp {
    fn rule(self) -> &'static str {
        match self {
            Self::Sum => "left-fold-sum",
            Self::Product => "left-fold-product",
        }
    }

    fn combine(self, acc: f64, next: f64) -> f64 {
        match self {
            Self::Sum => acc + next,
            Self::Product => acc * next,
        }
    }
}

fn fold<F>(
    request: &AnalogueRequest,
    eval: &F,
    op: FoldOp,
) -> Result<AnalogueReceipt, AnalogueError>
where
    F: Fn(f64) -> Result<f64, AnalogueError>,
{
    let AnalogueDomain::IntegerRange { lower, upper } = request.domain else {
        return Err(AnalogueError::InvalidDomain {
            reason: "kind-domain-mismatch",
        });
    };
    if lower > upper {
        return Err(AnalogueError::InvalidDomain { reason: "a>b" });
    }
    let span = (i128::from(upper) - i128::from(lower)).saturating_add(1);
    if span > i128::from(request.budget.max_terms) {
        return Err(AnalogueError::BudgetExceeded {
            limit: request.budget.max_terms,
        });
    }
    let mut samples = Vec::new();
    let mut partials = Vec::new();
    let mut acc: Option<f64> = None;
    let mut spent = 0_u32;
    for value in lower..=upper {
        spent += 1;
        let x = value as f64;
        let fx = finite_value(eval(x)?)?;
        samples.push(AnalogueSample {
            x_bits: x.to_bits(),
            fx_bits: fx.to_bits(),
        });
        let next = match acc {
            None => fx,
            Some(previous) => op.combine(previous, fx),
        };
        let next = finite_value(next)?;
        partials.push(next.to_bits());
        acc = Some(next);
    }
    let value = acc.ok_or(AnalogueError::InvalidDomain {
        reason: "empty-range",
    })?;
    Ok(receipt(
        request,
        op.rule(),
        AnalogueVerdict::Computed,
        spent,
        Some(value.to_bits()),
        samples,
        partials,
    ))
}

fn quadrature<F>(request: &AnalogueRequest, eval: &F) -> Result<AnalogueReceipt, AnalogueError>
where
    F: Fn(f64) -> Result<f64, AnalogueError>,
{
    let AnalogueDomain::Interval { lower, upper, n } = request.domain else {
        return Err(AnalogueError::InvalidDomain {
            reason: "kind-domain-mismatch",
        });
    };
    if !lower.is_finite() || !upper.is_finite() {
        return Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds",
        });
    }
    if lower > upper {
        return Err(AnalogueError::InvalidDomain { reason: "a>b" });
    }
    if n == 0 {
        return Err(AnalogueError::InvalidDomain { reason: "n=0" });
    }
    // Composite trapezoid evaluates n+1 nodes (endpoints plus interiors).
    let needed = u64::from(n).saturating_add(1);
    if needed > u64::from(request.budget.max_terms) {
        return Err(AnalogueError::BudgetExceeded {
            limit: request.budget.max_terms,
        });
    }
    let width = upper - lower;
    let step = width / f64::from(n);
    let mut samples = Vec::with_capacity(n as usize + 1);
    let mut acc = 0.0_f64;
    for i in 0..=n {
        let x = if i == n {
            upper
        } else {
            lower + step * f64::from(i)
        };
        let fx = finite_value(eval(x)?)?;
        samples.push(AnalogueSample {
            x_bits: x.to_bits(),
            fx_bits: fx.to_bits(),
        });
        let weight = if i == 0 || i == n { 1.0 } else { 2.0 };
        acc += weight * fx;
    }
    let value = finite_value(acc * (step * 0.5))?;
    Ok(receipt(
        request,
        "composite-trapezoid",
        AnalogueVerdict::Computed,
        n + 1,
        Some(value.to_bits()),
        samples,
        Vec::new(),
    ))
}

fn difference<F>(request: &AnalogueRequest, eval: &F) -> Result<AnalogueReceipt, AnalogueError>
where
    F: Fn(f64) -> Result<f64, AnalogueError>,
{
    let AnalogueDomain::Difference { point, h } = request.domain else {
        return Err(AnalogueError::InvalidDomain {
            reason: "kind-domain-mismatch",
        });
    };
    if !point.is_finite() || !h.is_finite() {
        return Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds",
        });
    }
    if h <= 0.0 {
        return Err(AnalogueError::InvalidDomain { reason: "h<=0" });
    }
    if request.budget.max_terms < 2 {
        return Err(AnalogueError::BudgetExceeded {
            limit: request.budget.max_terms,
        });
    }
    let left = point - h;
    let right = point + h;
    if !left.is_finite() || !right.is_finite() {
        return Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds",
        });
    }
    let f_left = finite_value(eval(left)?)?;
    let f_right = finite_value(eval(right)?)?;
    let value = finite_value((f_right - f_left) / (2.0 * h))?;
    Ok(receipt(
        request,
        "central-difference",
        AnalogueVerdict::Computed,
        2,
        Some(value.to_bits()),
        vec![
            AnalogueSample {
                x_bits: left.to_bits(),
                fx_bits: f_left.to_bits(),
            },
            AnalogueSample {
                x_bits: right.to_bits(),
                fx_bits: f_right.to_bits(),
            },
        ],
        Vec::new(),
    ))
}

fn limit_sample<F>(request: &AnalogueRequest, eval: &F) -> Result<AnalogueReceipt, AnalogueError>
where
    F: Fn(f64) -> Result<f64, AnalogueError>,
{
    let AnalogueDomain::Approach { point, samples } = request.domain else {
        return Err(AnalogueError::InvalidDomain {
            reason: "kind-domain-mismatch",
        });
    };
    if !point.is_finite() {
        return Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds",
        });
    }
    if samples == 0 {
        return Err(AnalogueError::InvalidDomain { reason: "n=0" });
    }
    if u64::from(samples) > u64::from(request.budget.max_terms) {
        return Err(AnalogueError::BudgetExceeded {
            limit: request.budget.max_terms,
        });
    }
    let mut recorded = Vec::with_capacity(samples as usize);
    for k in 0..samples {
        // Monotone decreasing toward `point` from the right; the point is
        // never evaluated. If the dyadic increment rounds to zero, the
        // "strictly right" invariant would break, so refuse.
        let exponent = i32::try_from(k).ok().and_then(|k| k.checked_add(1)).ok_or(
            AnalogueError::InvalidDomain {
                reason: "approach-underflow",
            },
        )?;
        let x = point + 0.5_f64.powi(exponent);
        if x <= point {
            return Err(AnalogueError::InvalidDomain {
                reason: "approach-underflow",
            });
        }
        let fx = finite_value(eval(x)?)?;
        recorded.push(AnalogueSample {
            x_bits: x.to_bits(),
            fx_bits: fx.to_bits(),
        });
    }
    Ok(receipt(
        request,
        "monotone-right-dyadic",
        AnalogueVerdict::NoClaim,
        samples,
        None,
        recorded,
        Vec::new(),
    ))
}

fn receipt(
    request: &AnalogueRequest,
    rule: &'static str,
    verdict: AnalogueVerdict,
    budget_spent: u32,
    value_bits: Option<u64>,
    samples: Vec<AnalogueSample>,
    partials: Vec<u64>,
) -> AnalogueReceipt {
    AnalogueReceipt {
        version: ANALOGUE_VERSION,
        request_id: analogue_id(request),
        kind: request.kind.canonical_name().to_string(),
        domain: request.domain.canonical(),
        rule,
        verdict,
        budget_limit: request.budget.max_terms,
        budget_spent,
        value_bits,
        samples,
        partials,
    }
}

fn finite_value(value: f64) -> Result<f64, AnalogueError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AnalogueError::EvaluationFailed {
            reason: "non-finite-value",
        })
    }
}

fn eval_numeric(term: &BinderTerm, bound: &VariableId, x: f64) -> Result<f64, AnalogueError> {
    match term {
        BinderTerm::Leaf(leaf) => eval_term(leaf, bound, x),
        BinderTerm::Bind(_) => Err(AnalogueError::UnsupportedKind {
            kind: "nested-binder".to_string(),
        }),
    }
}

fn eval_term(term: &Term, bound: &VariableId, x: f64) -> Result<f64, AnalogueError> {
    match term {
        Term::Variable(variable) if variable == bound => Ok(x),
        Term::Variable(_) => Err(AnalogueError::EvaluationFailed {
            reason: "unbound-variable",
        }),
        Term::Constant(symbol) => parse_constant(symbol),
        Term::Apply {
            operator,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| eval_term(argument, bound, x))
                .collect::<Result<Vec<_>, _>>()?;
            apply_op(operator, &values)
        }
    }
}

fn parse_constant(symbol: &SymbolId) -> Result<f64, AnalogueError> {
    symbol
        .0
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or(AnalogueError::EvaluationFailed {
            reason: "non-numeric-constant",
        })
}

fn apply_op(operator: &SymbolId, arguments: &[f64]) -> Result<f64, AnalogueError> {
    match (operator.0.as_str(), arguments) {
        ("+" | "add", [left, right]) => Ok(left + right),
        ("-" | "sub", [left, right]) => Ok(left - right),
        ("*" | "mul", [left, right]) => Ok(left * right),
        ("/" | "div", [left, right]) => Ok(left / right),
        ("pow", [left, right]) => Ok(left.powf(*right)),
        ("-" | "neg", [value]) => Ok(-value),
        ("+" | "add" | "-" | "sub" | "*" | "mul" | "/" | "div" | "pow", values) => {
            Err(AnalogueError::EvaluationFailed {
                reason: if values.len() == 1 && matches!(operator.0.as_str(), "+" | "add") {
                    "unknown-operator"
                } else {
                    "bad-arity"
                },
            })
        }
        _ => Err(AnalogueError::EvaluationFailed {
            reason: "unknown-operator",
        }),
    }
}

enum Json {
    Str(String),
    Number(String),
    Null,
    Array(Vec<Json>),
    Object(BTreeMap<&'static str, Json>),
}

fn sample_json(sample: &AnalogueSample) -> Json {
    let mut object = BTreeMap::new();
    object.insert("fx", Json::Str(format!("{:016x}", sample.fx_bits)));
    object.insert("x", Json::Str(format!("{:016x}", sample.x_bits)));
    Json::Object(object)
}

fn emit_object(fields: &BTreeMap<&str, Json>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", json_escape(key));
        emit_json(value, &mut out);
    }
    out.push('}');
    out
}

fn emit_json(value: &Json, out: &mut String) {
    match value {
        Json::Str(text) => {
            let _ = write!(out, "\"{}\"", json_escape(text));
        }
        Json::Number(text) => out.push_str(text),
        Json::Null => out.push_str("null"),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                emit_json(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push('{');
            for (index, (key, item)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let _ = write!(out, "\"{}\":", json_escape(key));
                emit_json(item, out);
            }
            out.push('}');
        }
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        analogue_id, check_version, AnalogueDomain, AnalogueError, AnalogueRequest,
        AnalogueVerdict, ANALOGUE_NO_CLAIM, ANALOGUE_VERSION,
    };
    use crate::binder::{BinderBudget, BinderKind, BinderTerm};
    use emath_term::{SymbolId, Term, VariableId};

    fn var(name: &str) -> Term {
        Term::Variable(VariableId(name.to_string()))
    }

    fn constant(text: &str) -> Term {
        Term::Constant(SymbolId(text.to_string()))
    }

    fn apply(op: &str, arguments: Vec<Term>) -> Term {
        Term::Apply {
            operator: SymbolId(op.to_string()),
            arguments,
        }
    }

    fn identity(kind: BinderKind, domain: AnalogueDomain) -> AnalogueRequest {
        AnalogueRequest {
            kind,
            domain,
            budget: BinderBudget::default(),
            bound: VariableId("x".to_string()),
            body: BinderTerm::Leaf(var("x")),
        }
    }

    fn bits(value: f64) -> u64 {
        value.to_bits()
    }

    #[test]
    fn happy_path_per_kind() {
        let sum = identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange { lower: 1, upper: 4 },
        )
        .evaluate()
        .expect("sum");
        assert_eq!(sum.value_bits, Some(bits(10.0)));
        assert_eq!(sum.rule, "left-fold-sum");
        assert_eq!(sum.verdict, AnalogueVerdict::Computed);
        assert_eq!(sum.budget_spent, 4);
        assert_eq!(sum.partials.len(), 4);

        let product = identity(
            BinderKind::Product,
            AnalogueDomain::IntegerRange { lower: 1, upper: 4 },
        )
        .evaluate()
        .expect("product");
        assert_eq!(product.value_bits, Some(bits(24.0)));
        assert_eq!(product.rule, "left-fold-product");

        let integral = identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 1.0,
                n: 4,
            },
        )
        .evaluate()
        .expect("integral");
        assert_eq!(integral.value_bits, Some(bits(0.5)));
        assert_eq!(integral.rule, "composite-trapezoid");
        assert_eq!(integral.budget_spent, 5);

        let square = AnalogueRequest {
            kind: BinderKind::Derivative,
            domain: AnalogueDomain::Difference {
                point: 3.0,
                h: 0.25,
            },
            budget: BinderBudget::default(),
            bound: VariableId("x".to_string()),
            body: BinderTerm::Leaf(apply("*", vec![var("x"), var("x")])),
        }
        .evaluate()
        .expect("derivative");
        assert_eq!(square.value_bits, Some(bits(6.0)));
        assert_eq!(square.rule, "central-difference");
        assert_eq!(square.budget_spent, 2);

        let limit = identity(
            BinderKind::Limit,
            AnalogueDomain::Approach {
                point: 0.0,
                samples: 4,
            },
        )
        .evaluate()
        .expect("limit");
        assert_eq!(limit.verdict, AnalogueVerdict::NoClaim);
        assert_eq!(limit.verdict.canonical(), ANALOGUE_NO_CLAIM);
        assert_eq!(limit.value_bits, None);
        assert_eq!(limit.samples.len(), 4);
        assert_eq!(limit.samples[0].x_bits, bits(0.5));
        assert_eq!(limit.samples[1].x_bits, bits(0.25));
        assert_eq!(limit.samples[2].x_bits, bits(0.125));
        assert_eq!(limit.samples[3].x_bits, bits(0.0625));
    }

    #[test]
    fn boundary_empty_range_and_single_interval() {
        assert_eq!(
            identity(
                BinderKind::Sum,
                AnalogueDomain::IntegerRange { lower: 3, upper: 2 },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain { reason: "a>b" })
        );

        let single = identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange { lower: 7, upper: 7 },
        )
        .evaluate()
        .expect("single-point fold");
        assert_eq!(single.value_bits, Some(bits(7.0)));
        assert_eq!(single.budget_spent, 1);

        let empty_width = identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 2.0,
                upper: 2.0,
                n: 1,
            },
        )
        .evaluate()
        .expect("zero-width interval");
        assert_eq!(empty_width.value_bits, Some(bits(0.0)));

        let one_panel = identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 2.0,
                n: 1,
            },
        )
        .evaluate()
        .expect("single interval");
        assert_eq!(one_panel.value_bits, Some(bits(2.0)));
        assert_eq!(one_panel.budget_spent, 2);
    }

    #[test]
    fn refusals_are_typed() {
        assert_eq!(
            identity(
                BinderKind::Sum,
                AnalogueDomain::IntegerRange {
                    lower: 1,
                    upper: 1000,
                },
            )
            .evaluate(),
            Err(AnalogueError::BudgetExceeded { limit: 64 })
        );
        let tight = AnalogueRequest {
            budget: BinderBudget { max_terms: 8 },
            ..identity(
                BinderKind::Sum,
                AnalogueDomain::IntegerRange {
                    lower: 1,
                    upper: 1000,
                },
            )
        };
        assert_eq!(
            tight.evaluate(),
            Err(AnalogueError::BudgetExceeded { limit: 8 })
        );

        assert_eq!(
            identity(
                BinderKind::Integral,
                AnalogueDomain::Interval {
                    lower: 0.0,
                    upper: 1.0,
                    n: 0,
                },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain { reason: "n=0" })
        );
        assert_eq!(
            identity(
                BinderKind::Derivative,
                AnalogueDomain::Difference { point: 0.0, h: 0.0 },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain { reason: "h<=0" })
        );
        assert_eq!(
            identity(
                BinderKind::Sum,
                AnalogueDomain::Interval {
                    lower: 0.0,
                    upper: 1.0,
                    n: 2,
                },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain {
                reason: "kind-domain-mismatch"
            })
        );
        assert_eq!(
            identity(
                BinderKind::Custom("bigjoin".to_string()),
                AnalogueDomain::IntegerRange { lower: 1, upper: 2 },
            )
            .evaluate(),
            Err(AnalogueError::UnsupportedKind {
                kind: "bigjoin".to_string()
            })
        );
        assert_eq!(check_version(ANALOGUE_VERSION), Ok(()));
        assert_eq!(
            check_version(ANALOGUE_VERSION + 1),
            Err(AnalogueError::UnknownVersion {
                version: ANALOGUE_VERSION + 1
            })
        );
    }

    #[test]
    fn malformed_nan_inf_bounds_and_huge_n_are_refused() {
        assert_eq!(
            identity(
                BinderKind::Integral,
                AnalogueDomain::Interval {
                    lower: f64::NAN,
                    upper: 1.0,
                    n: 2,
                },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain {
                reason: "non-finite-bounds"
            })
        );
        assert_eq!(
            identity(
                BinderKind::Integral,
                AnalogueDomain::Interval {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    n: 2,
                },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain {
                reason: "non-finite-bounds"
            })
        );
        assert_eq!(
            identity(
                BinderKind::Derivative,
                AnalogueDomain::Difference {
                    point: f64::NEG_INFINITY,
                    h: 1.0,
                },
            )
            .evaluate(),
            Err(AnalogueError::InvalidDomain {
                reason: "non-finite-bounds"
            })
        );
        let huge = AnalogueRequest {
            budget: BinderBudget { max_terms: 16 },
            ..identity(
                BinderKind::Integral,
                AnalogueDomain::Interval {
                    lower: 0.0,
                    upper: 1.0,
                    n: u32::MAX,
                },
            )
        };
        assert_eq!(
            huge.evaluate(),
            Err(AnalogueError::BudgetExceeded { limit: 16 })
        );
    }

    #[test]
    fn limit_approach_refuses_instead_of_sampling_the_point() {
        // ulp absorption: 1.0 + 2^-53 rounds back to 1.0 (ties-to-even),
        // so a long enough approach toward 1.0 would evaluate the point
        // itself. The invariant is "strictly right of the point": refuse.
        let absorbed = AnalogueRequest {
            budget: BinderBudget { max_terms: 64 },
            ..identity(
                BinderKind::Limit,
                AnalogueDomain::Approach {
                    point: 1.0,
                    samples: 64,
                },
            )
        };
        assert_eq!(
            absorbed.evaluate(),
            Err(AnalogueError::InvalidDomain {
                reason: "approach-underflow"
            })
        );
    }

    #[test]
    fn receipts_are_byte_identical_across_runs() {
        let request = identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 1.0,
                n: 8,
            },
        );
        let first = request.evaluate().expect("first").to_json();
        let second = request.evaluate().expect("second").to_json();
        assert_eq!(first, second);
        assert!(first.starts_with('{'));
        assert!(first.contains("\"schema\":\"emath.analogue\""));
        assert_eq!(analogue_id(&request), analogue_id(&request));
    }

    #[test]
    fn sum_of_identity_matches_closed_form() {
        for n in 1_i64..=20 {
            let receipt = identity(
                BinderKind::Sum,
                AnalogueDomain::IntegerRange { lower: 1, upper: n },
            )
            .evaluate()
            .expect("sum");
            let expected = (n * (n + 1) / 2) as f64;
            assert_eq!(
                receipt.value_bits,
                Some(bits(expected)),
                "sum 1..={n} must equal n(n+1)/2"
            );
        }
    }

    #[test]
    fn trapezoid_of_linear_is_exact() {
        // f(x) = 2x + 3 on [1, 4]; ∫ = [x² + 3x]₁⁴ = 24.
        let body = BinderTerm::Leaf(apply(
            "+",
            vec![apply("*", vec![constant("2"), var("x")]), constant("3")],
        ));
        for n in 1_u32..=8 {
            let receipt = AnalogueRequest {
                kind: BinderKind::Integral,
                domain: AnalogueDomain::Interval {
                    lower: 1.0,
                    upper: 4.0,
                    n,
                },
                budget: BinderBudget::default(),
                bound: VariableId("x".to_string()),
                body: body.clone(),
            }
            .evaluate()
            .expect("trapezoid");
            assert_eq!(
                receipt.value_bits,
                Some(bits(24.0)),
                "trapezoid of a line must be exact at n={n}"
            );
        }
    }
}
