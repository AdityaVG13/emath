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
    /// Inline record value with a nominal type path.
    Record {
        type_name: String,
        fields: BTreeMap<String, Value>,
    },
    /// Complex number (real + imaginary parts)..
    Complex { re: f64, im: f64 },
    /// Exact rational `num/den` . Canonical
    /// form: gcd-reduced with `den > 0`, so equality is componentwise.
    /// Built only from integer arithmetic, never from f64.
    Rat { num: i128, den: i128 },
    /// Signed constructor integer that does not fit `i64`.
    ExactInt(emath_rt::ExactInt),
    /// Rational whose numerator or denominator does not fit `i128`.
    ExactRat {
        num: emath_rt::ExactInt,
        den: emath_rt::ExactInt,
    },
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
    /// Result value semantics: the `ok` flag
    /// distinguishes Ok-payload from Err-payload on ONE carrier (a
    /// shared Option carrier could not: Err(42) would read as
    /// Ok(42)); the payload is the value when Ok, the error when Err.
    Result { ok: bool, payload: Box<Value> },
    /// Universal program-as-value artifact produced by
    /// `EmirOp::ProgramLiteral`. Domain-neutral: the carrier names no
    /// `FeatureID` and dispatches nothing; it can flow into an
    /// `ApplyCapability` argument register like any other value.
    Program(ProgramValue),
    /// Universal heterogeneous sequence: the ordinary carrier for
    /// variadic capability arguments (a subscript `Text` plus a
    /// rank-polymorphic operand list rides as one value).
    /// Domain-neutral: no operand-shape or capability naming.
    List(Vec<Value>),
}

/// A numeric callback with an explicit argument convention and typed captures.
/// Captures occupy the final input slots; the body has no state slots.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramValue {
    pub body: EmirProgram,
    pub captures: Vec<Value>,
    pub vector_input: bool,
}

impl Value {
    pub fn program(body: EmirProgram) -> Self {
        Self::Program(ProgramValue { body, captures: Vec::new(), vector_input: false })
    }

}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F64(left), Self::F64(right)) => left.to_bits() == right.to_bits(),
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::ExactInt(left), Self::ExactInt(right)) => left == right,
            (Self::ExactInt(left), Self::I64(right)) => left == &emath_rt::ExactInt::from(*right),
            (Self::I64(left), Self::ExactInt(right)) => emath_rt::ExactInt::from(*left) == *right,
            (Self::ExactInt(left), Self::Rat { num, den })
            | (Self::Rat { num, den }, Self::ExactInt(left)) => {
                *den == 1 && left == &emath_rt::ExactInt::from(*num)
            }
            (
                Self::ExactInt(left),
                Self::ExactRat {
                    num: right_num,
                    den: right_den,
                },
            )
            | (
                Self::ExactRat {
                    num: right_num,
                    den: right_den,
                },
                Self::ExactInt(left),
            ) => right_den.is_one() && left == right_num,
            (
                Self::ExactRat {
                    num: left_num,
                    den: left_den,
                },
                Self::ExactRat {
                    num: right_num,
                    den: right_den,
                },
            ) => left_num == right_num && left_den == right_den,
            (
                Self::ExactRat {
                    num: left_num,
                    den: left_den,
                },
                Self::Rat {
                    num: right_num,
                    den: right_den,
                },
            )
            | (
                Self::Rat {
                    num: right_num,
                    den: right_den,
                },
                Self::ExactRat {
                    num: left_num,
                    den: left_den,
                },
            ) => {
                left_num == &emath_rt::ExactInt::from(*right_num)
                    && left_den == &emath_rt::ExactInt::from(*right_den)
            }
            (Self::I64(left), Self::Rat { num, den })
            | (Self::Rat { num, den }, Self::I64(left)) => *den == 1 && i128::from(*left) == *num,
            (Self::I64(left), Self::ExactRat { num, den })
            | (Self::ExactRat { num, den }, Self::I64(left)) => {
                den.is_one() && &emath_rt::ExactInt::from(*left) == num
            }
            (Self::I64(left), Self::F64(right)) => emath_rt::eq_i64_f64(*left, *right),
            (Self::F64(left), Self::I64(right)) => emath_rt::eq_i64_f64(*right, *left),
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Text(left), Self::Text(right)) => left == right,
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
        match self {
            Self::F64(v) => Some(*v),
            Self::I64(v) => Some(*v as f64),
            Self::ExactInt(v) => Some(v.to_f64()),
            Self::ExactRat { num, den } => Some(crate::exact_int::exact_ratio_f64(num, den)),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F64(value) => f.write_str(&format_f64(*value)),
            Self::I64(value) => write!(f, "{value}"),
            Self::ExactInt(value) => write!(f, "{value}"),
            Self::ExactRat { num, den } => write!(f, "{num}/{den}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
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
                write!(f, "tensor{shape:?}[")?;
                for (i, elem) in data.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&format_f64(*elem))?;
                }
                f.write_str("]")
            }
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
    /// Op violated an arithmetic precondition (index-offset
    /// overflow, etc.). Distinct from IEEE `/0` on f64 ops,
    /// which remains Inf/NaN to match generated Rust.
    Arithmetic {
        /// EMIR op name.
        op: &'static str,
        /// Short reason (`integral steps must be positive and even`).
        detail: &'static str,
    },
    /// A scalar carrier operation refused; retain its full diagnostic.
    CarrierRefused { op: &'static str, detail: String },
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
    /// semantics in this world. This is the typed continuation hole:
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
            Self::CarrierRefused { op, detail } => write!(f, "{op}: {detail}"),
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


impl Value {
    /// Structured value view; numeric payloads retain exact carrier bits.
    /// Program values remain opaque rather than resumable method arguments.
    pub fn json(&self) -> String {
        use emath_core::JsonWriter;
        let mut out = JsonWriter::object();
        let kind = match self {
            Self::I64(_) => "Int", Self::ExactInt(_) => "Int", Self::F64(_) => "Float64", Self::Bool(_) => "Bool",
            Self::Rat { .. } | Self::ExactRat { .. } => "Rat", Self::Text(_) => "Text", Self::Record { .. } => "Record",
            Self::Vector(_) => "Vector<Float64>", Self::Matrix { .. } => "Matrix<Float64>",
            Self::Tensor { .. } => "Tensor<Float64>", Self::Complex { .. } => "Complex",
            Self::Result { .. } => "Result", Self::Program(_) => "Program", Self::List(_) => "List",
        };
        out.string("type", kind);
        out.string("value", &self.to_string());
        let bits = |values: &[f64]| values.iter().map(|value| format!("{:016x}", value.to_bits())).collect::<Vec<_>>();
        match self {
            Self::F64(value) => { out.string("bits", &format!("{:016x}", value.to_bits())); }
            Self::Rat { num, den } => { out.string("numerator", &num.to_string()); out.string("denominator", &den.to_string()); }
            Self::Record { type_name, fields } => {
                out.string("name", type_name);
                let mut members = JsonWriter::object();
                for (name, value) in fields { members.object_field(name, &value.json()); }
                out.object_field("fields", &members.finish());
            }
            Self::List(values) => { out.objects("elements", &values.iter().map(Self::json).collect::<Vec<_>>()); }
            Self::Vector(values) => { out.strings("bits", &bits(values)); }
            Self::Matrix { rows, cols, data } => {
                out.int("rows", *rows as u64); out.int("columns", *cols as u64); out.strings("bits", &bits(data));
            }
            Self::Tensor { shape, data } => {
                out.strings("shape", &shape.iter().map(usize::to_string).collect::<Vec<_>>()); out.strings("bits", &bits(data));
            }
            Self::Complex { re, im } => { out.strings("bits", &bits(&[*re, *im])); }
            Self::Result { ok, payload } => { out.bool("ok", *ok); out.object_field("payload", &payload.json()); }
            Self::Program(program) => {
                out.string("program", &program.body.print());
                out.bool("vector_input", program.vector_input);
                out.objects("captures", &program.captures.iter().map(Self::json).collect::<Vec<_>>());
            }
            Self::I64(_) | Self::Bool(_) | Self::Text(_) | Self::ExactInt(_) | Self::ExactRat { .. } => {}
        }
        out.finish()
    }
}

impl Value {

}
