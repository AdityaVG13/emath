use std::{collections::BTreeMap, fmt};

use crate::EmirProgram;

/// A typed register value. Locals match generated Rust (`f64` / `bool` / `Vec<f64>`).
#[derive(Clone, Debug)]
pub enum Value {
    /// IEEE-754 binary64.
    F64(f64),
    /// Signed 64-bit integer (exact arithmetic in folds).
    I64(i64),
    /// Boolean, produced by comparisons, `is_finite`, `and`/`or`/`not`.
    Bool(bool),
    /// UTF-8 text produced by a literal or pure interpolation.
    Text(String),
    /// Immutable time-series data and its declared interpretation policy.
    Series {
        points: Vec<(f64, f64)>,
        interpolation: String,
        extrapolation: String,
    },
    /// Finite extensional set.
    Set(Vec<Value>),
    /// Inline record value with a nominal type path.
    Record {
        type_name: String,
        fields: BTreeMap<String, Value>,
    },
    /// Complex number (real + imaginary parts). B14.
    Complex { re: f64, im: f64 },
    /// Exact rational `num/den` . Canonical
    /// form: gcd-reduced with `den > 0`, so equality is componentwise.
    /// Built only from integer arithmetic — never from f64.
    Rat { num: i128, den: i128 },
    /// Stage-2 big integer (emath-t63iz): exact NON-NEGATIVE field
    /// element with |F| < 2^256, produced by `ConstBigInt` and the six
    /// modular number-theory builtins when any operand is big. Never
    /// coerced to f64/i64.
    BigInt(emath_rt::UBig),
    /// Vector of stage-2 big integers (the `rs_encode` codeword over a
    /// big modulus).
    BigVector(Vec<emath_rt::UBig>),
    /// Vector of Float64.
    Vector(Vec<f64>),
    /// Matrix of Float64 (row-major).
    Matrix {
        rows: usize,
        cols: usize,
        data: Vec<f64>,
    },
    /// Rank-3+ tensor of Float64, row-major.
    Tensor { shape: Vec<usize>, data: Vec<f64> },
    /// Certified interval `[lo, hi]`. Constructed only through
    /// `IntervalCreate`, which refuses ill-formed bounds.
    Interval { lo: f64, hi: f64 },
    /// Option value semantics: `Some(inner)` or a
    /// None that genuinely carries NOTHING (never a hidden zero — the
    /// honesty gate is the TOTAL `OptionUnwrapOr`, since no panicking
    /// unwrap exists at this layer).
    Option(Option<Box<Value>>),
    /// Result value semantics: the `ok` flag
    /// distinguishes Ok-payload from Err-payload on ONE carrier (a
    /// shared Option carrier could not — Err(42) would read as
    /// Ok(42)); the payload is the value when Ok, the error when Err.
    Result { ok: bool, payload: Box<Value> },
    /// Universal program-as-value artifact produced by
    /// `EmirOp::ProgramLiteral`. Domain-neutral: the carrier names no
    /// FeatureID and dispatches nothing; it can flow into an
    /// `ApplyCapability` argument register like any other value.
    Program(EmirProgram),
    /// Universal heterogeneous sequence: the ordinary carrier for
    /// variadic capability arguments (a subscript `Text` plus a
    /// rank-polymorphic operand list rides as one value).
    /// Domain-neutral: no operand-shape or capability naming.
    List(Vec<Value>),
}

impl Value {
    /// Stage-2 (emath-t63iz): parse an exact non-negative decimal
    /// integer into the big lane. Used by the eval `--set` binding path;
    /// admits |F| < 2^256 only.
    pub fn parse_bigint(raw: &str) -> Option<Value> {
        let trimmed = raw.trim();
        if trimmed.starts_with('-') {
            return None;
        }
        let value = emath_rt::UBig::parse_decimal(trimmed).ok()?;
        if value.bits() > emath_rt::LIMIT_BITS {
            return None;
        }
        Some(Value::BigInt(value))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F64(left), Self::F64(right)) => left.to_bits() == right.to_bits(),
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::I64(left), Self::F64(right)) => emath_rt::eq_i64_f64(*left, *right),
            (Self::F64(left), Self::I64(right)) => emath_rt::eq_i64_f64(*right, *left),
            (Self::BigInt(left), Self::BigInt(right)) => left == right,
            (Self::BigVector(left), Self::BigVector(right)) => left == right,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Text(left), Self::Text(right)) => left == right,
            (
                Self::Series {
                    points: left_points,
                    interpolation: left_interpolation,
                    extrapolation: left_extrapolation,
                },
                Self::Series {
                    points: right_points,
                    interpolation: right_interpolation,
                    extrapolation: right_extrapolation,
                },
            ) => {
                left_interpolation == right_interpolation
                    && left_extrapolation == right_extrapolation
                    && left_points.len() == right_points.len()
                    && left_points.iter().zip(right_points).all(
                        |((left_time, left_value), (right_time, right_value))| {
                            left_time.to_bits() == right_time.to_bits()
                                && left_value.to_bits() == right_value.to_bits()
                        },
                    )
            }
            (Self::Set(left), Self::Set(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .all(|item| right.iter().any(|other| item == other))
            }
            (
                Self::Record {
                    type_name: left_name,
                    fields: left,
                },
                Self::Record {
                    type_name: right_name,
                    fields: right,
                },
            ) => left_name == right_name && left == right,
            (Self::Complex { re: r1, im: i1 }, Self::Complex { re: r2, im: i2 }) => {
                r1.to_bits() == r2.to_bits() && i1.to_bits() == i2.to_bits()
            }
            (Self::Complex { re, im }, Self::F64(right)) => {
                im.to_bits() == 0.0_f64.to_bits() && re.to_bits() == right.to_bits()
            }
            (Self::F64(left), Self::Complex { re, im }) => {
                im.to_bits() == 0.0_f64.to_bits() && left.to_bits() == re.to_bits()
            }
            (Self::Complex { re, im }, Self::I64(right)) => {
                *im == 0.0 && emath_rt::eq_i64_f64(*right, *re)
            }
            (Self::I64(left), Self::Complex { re, im }) => {
                *im == 0.0 && emath_rt::eq_i64_f64(*left, *re)
            }
            (
                Self::Rat {
                    num: left_num,
                    den: left_den,
                },
                Self::Rat {
                    num: right_num,
                    den: right_den,
                },
            ) => left_num == right_num && left_den == right_den,
            (Self::Vector(left), Self::Vector(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            (
                Self::Matrix {
                    rows: r1,
                    cols: c1,
                    data: d1,
                },
                Self::Matrix {
                    rows: r2,
                    cols: c2,
                    data: d2,
                },
            ) => {
                r1 == r2
                    && c1 == c2
                    && d1.len() == d2.len()
                    && d1
                        .iter()
                        .zip(d2.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            (
                Self::Tensor {
                    shape: s1,
                    data: d1,
                },
                Self::Tensor {
                    shape: s2,
                    data: d2,
                },
            ) => {
                s1 == s2
                    && d1.len() == d2.len()
                    && d1
                        .iter()
                        .zip(d2.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            (Self::Interval { lo: l1, hi: h1 }, Self::Interval { lo: l2, hi: h2 }) => {
                l1.to_bits() == l2.to_bits() && h1.to_bits() == h2.to_bits()
            }
            (Self::Option(left), Self::Option(right)) => match (left, right) {
                (None, None) => true,
                (Some(l), Some(r)) => l == r,
                _ => false,
            },
            (
                Self::Result {
                    ok: ok1,
                    payload: p1,
                },
                Self::Result {
                    ok: ok2,
                    payload: p2,
                },
            ) => ok1 == ok2 && p1 == p2,
            (Self::Program(left), Self::Program(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            _ => false,
        }
    }
}

impl Value {
    /// Real scalar at a Float64 or AD/solver boundary: `F64` as-is, `I64`
    /// widened. Int-typed registers stay `I64` until this conversion.
    #[must_use]
    pub fn as_real_f64(&self) -> Option<f64> {
        match *self {
            Self::F64(v) => Some(v),
            Self::I64(v) => Some(v as f64),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F64(value) => f.write_str(&format_f64(*value)),
            Self::I64(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
            Self::Series {
                points,
                interpolation,
                extrapolation,
            } => write!(
                f,
                "Series({points:?}, interpolation: {interpolation}, extrapolation: {extrapolation})"
            ),
            Self::Set(values) => {
                f.write_str("{")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{value}")?;
                }
                f.write_str("}")
            }
            Self::Record { type_name, fields } => {
                write!(f, "{type_name}:{{")?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                f.write_str("}")
            }
            Self::Rat { num, den } => write!(f, "{num}/{den}"),
            Self::Complex { re, im } => {
                if *im == 0.0 {
                    f.write_str(&format_f64(*re))
                } else if *re == 0.0 {
                    write!(f, "{}i", format_f64(*im))
                } else {
                    write!(f, "{} + {}i", format_f64(*re), format_f64(*im))
                }
            }
            Self::BigInt(value) => f.write_str(&value.to_decimal()),
            Self::BigVector(values) => {
                f.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&value.to_decimal())?;
                }
                f.write_str("]")
            }
            Self::Vector(vec) => {
                f.write_str("[")?;
                for (i, elem) in vec.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&format_f64(*elem))?;
                }
                f.write_str("]")
            }
            Self::Matrix { rows, cols, data } => {
                f.write_str("[")?;
                for r in 0..*rows {
                    if r > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str("[")?;
                    for c in 0..*cols {
                        if c > 0 {
                            f.write_str(", ")?;
                        }
                        if let Some(elem) = data.get(r * cols + c) {
                            f.write_str(&format_f64(*elem))?;
                        }
                    }
                    f.write_str("]")?;
                }
                f.write_str("]")
            }
            Self::Tensor { shape, data } => {
                write!(f, "tensor{:?}[", shape)?;
                for (i, elem) in data.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&format_f64(*elem))?;
                }
                f.write_str("]")
            }
            Self::Interval { lo, hi } => {
                write!(f, "[{}, {}]", format_f64(*lo), format_f64(*hi))
            }
            Self::Option(Some(inner)) => write!(f, "some({inner})"),
            Self::Option(None) => f.write_str("none"),
            Self::Result { ok, payload } => {
                if *ok {
                    write!(f, "ok({payload})")
                } else {
                    write!(f, "err({payload})")
                }
            }
            // Canonical dump of the program artifact: the nested program's
            // own Debug form, bracketed so the carrier is unmistakable.
            Self::Program(program) => write!(f, "program({program:?})"),
            Self::List(values) => {
                f.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{value}")?;
                }
                f.write_str("]")
            }
        }
    }
}

/// Format an f64 for display/JSON: finite values get a trailing `.0` when
/// they would otherwise look like integers; non-finite become `NaN`,
/// `Infinity`, `-Infinity`.
#[must_use]
pub fn format_f64(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let mut text = format!("{value}");
    if !text.contains('.') && !text.contains('e') && !text.contains('E') {
        text.push_str(".0");
    }
    text
}

/// Typed evaluation fault. The interpreter never panics on a well-formed
/// program; every failure is one of these variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalFault {
    /// An operand had the wrong type for `op` (no coercion).
    TypeConfusion {
        /// Register that failed the type check.
        register: u32,
        /// EMIR op name (`EmirOp::name`).
        op: &'static str,
    },
    /// `LoadInput` index was outside the provided input slice.
    MissingInput(u16),
    /// `LoadState` index was outside the provided state slice.
    MissingState(u16),
    /// An operand or the program result named an unwritten register.
    BadRegister(u32),
    /// Index was not a finite whole number, or was outside the value.
    IndexOutOfBounds {
        /// EMIR op name (`vec-index` / `mat-index`).
        op: &'static str,
        /// Requested index (row for matrices when `col` is set).
        index: i64,
        /// Exclusive upper bound of the indexed axis.
        len: usize,
    },
    /// Op violated an arithmetic precondition (zero/odd Simpson steps,
    /// index-offset overflow, etc.). Distinct from IEEE `/0` on f64 ops,
    /// which remains Inf/NaN to match generated Rust.
    Arithmetic {
        /// EMIR op name.
        op: &'static str,
        /// Short reason (`integral steps must be positive and even`).
        detail: &'static str,
    },
    /// A series with `extrapolation: refuse` was sampled outside its support.
    SeriesOutOfSupport {
        time_bits: u64,
        start_bits: u64,
        end_bits: u64,
    },
    /// Capability-cell contract refusal surfaced from the capability
    /// layer (e.g. `E-CELL-006`: missing numeric policy, or non-finite
    /// logits under the strict-f64 finite policy). The strict vs
    /// Genesis/custom firewall holds at the VM seam.
    CapabilityRefused {
        /// Cell the refusal names.
        capability: String,
        /// Stable code from the capability layer (`E-CELL-*`).
        code: String,
    },
    /// Outstanding provider call: the cell has no local reference
    /// semantics in this world. This is the typed continuation hole —
    /// resumable by a provider run, never a silent identity.
    ProviderCallRequired {
        /// Cell the provider must fulfill.
        capability: String,
        /// Number of arguments the provider receives.
        args: usize,
    },
    /// Evaluation budget exhausted before completion; no partial
    /// authority escapes.
    BudgetExhausted {
        /// Ops successfully executed before the refusal.
        executed: u32,
    },
}

impl fmt::Display for EvalFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeConfusion { register, op } => {
                write!(f, "type confusion at %{register} in {op}")
            }
            Self::MissingInput(index) => write!(f, "missing input {index}"),
            Self::MissingState(index) => write!(f, "missing state {index}"),
            Self::BadRegister(register) => write!(f, "bad register %{register}"),
            Self::IndexOutOfBounds { op, index, len } => {
                write!(f, "{op} index {index} is outside 0..{len}")
            }
            Self::Arithmetic { op, detail } => write!(f, "{op}: {detail}"),
            Self::SeriesOutOfSupport {
                time_bits,
                start_bits,
                end_bits,
            } => write!(
                f,
                "series-sample: t={} is outside support [{}, {}] under extrapolation: refuse",
                f64::from_bits(*time_bits),
                f64::from_bits(*start_bits),
                f64::from_bits(*end_bits)
            ),
            Self::CapabilityRefused { capability, code } => {
                write!(
                    f,
                    "capability cell `{capability}` refused its contract ({code})"
                )
            }
            Self::ProviderCallRequired { capability, args } => write!(
                f,
                "capability cell `{capability}` requires a provider call \
                 with {args} argument(s)"
            ),
            Self::BudgetExhausted { executed } => write!(
                f,
                "evaluation budget exhausted after {executed} step(s); \
                 no partial authority"
            ),
        }
    }
}
