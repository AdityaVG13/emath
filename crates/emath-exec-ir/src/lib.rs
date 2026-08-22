//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

pub mod interp;
pub mod runner;

pub use runner::{
    definition_order, simulate_continuous, step_continuous, step_continuous_values, StepMethod,
    Trajectory, TrajectorySample,
};

use emath_core::Span;
use emath_ir::{BinaryOp, ExprNode, Literal, SemanticPackage, UnaryOp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmirValue(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum EmirOp {
    ConstF64(u64),
    LoadInput(u16),
    LoadState(u16),
    F64Add(EmirValue, EmirValue),
    F64Sub(EmirValue, EmirValue),
    F64Mul(EmirValue, EmirValue),
    F64Div(EmirValue, EmirValue),
    F64Pow(EmirValue, EmirValue),
    Neg(EmirValue),
    Exp(EmirValue),
    Ln(EmirValue),
    Sqrt(EmirValue),
    Sin(EmirValue),
    Cos(EmirValue),
    Tan(EmirValue),
    Tanh(EmirValue),
    Abs(EmirValue),
    Floor(EmirValue),
    Ceil(EmirValue),
    Min(EmirValue, EmirValue),
    Max(EmirValue, EmirValue),
    Atan2(EmirValue, EmirValue),
    Lt(EmirValue, EmirValue),
    Le(EmirValue, EmirValue),
    Gt(EmirValue, EmirValue),
    Ge(EmirValue, EmirValue),
    Eq(EmirValue, EmirValue),
    Ne(EmirValue, EmirValue),
    And(EmirValue, EmirValue),
    Or(EmirValue, EmirValue),
    Not(EmirValue),
    IsFinite(EmirValue),
    Select {
        condition: EmirValue,
        then_value: EmirValue,
        else_value: EmirValue,
    },
    VectorCreate(Vec<EmirValue>),
    MatrixCreate {
        rows: usize,
        cols: usize,
        elements: Vec<EmirValue>,
    },
    VectorIndex {
        vector: EmirValue,
        index: EmirValue,
    },
    MatrixIndex {
        matrix: EmirValue,
        row: EmirValue,
        col: EmirValue,
    },
    VectorAdd(EmirValue, EmirValue),
    VectorSub(EmirValue, EmirValue),
    VectorScale(EmirValue, EmirValue),
    VectorDot(EmirValue, EmirValue),
    VectorNorm(EmirValue),
    VectorLength(EmirValue),
    MatrixAdd(EmirValue, EmirValue),
    MatrixSub(EmirValue, EmirValue),
    MatrixScale(EmirValue, EmirValue),
    MatrixMulVector(EmirValue, EmirValue),
    MatrixMulMatrix(EmirValue, EmirValue),
    MatrixTranspose(EmirValue),
}

impl EmirOp {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::LoadInput(_) => "load-input",
            Self::LoadState(_) => "load-state",
            Self::F64Add(..) => "f64-add",
            Self::F64Sub(..) => "f64-sub",
            Self::F64Mul(..) => "f64-mul",
            Self::F64Div(..) => "f64-div",
            Self::F64Pow(..) => "f64-pow",
            Self::Neg(_) => "neg",
            Self::Exp(_) => "exp",
            Self::Ln(_) => "ln",
            Self::Sqrt(_) => "sqrt",
            Self::Sin(_) => "sin",
            Self::Cos(_) => "cos",
            Self::Tan(_) => "tan",
            Self::Tanh(_) => "tanh",
            Self::Abs(_) => "abs",
            Self::Floor(_) => "floor",
            Self::Ceil(_) => "ceil",
            Self::Min(..) => "min",
            Self::Max(..) => "max",
            Self::Atan2(..) => "atan2",
            Self::Lt(..) => "lt",
            Self::Le(..) => "le",
            Self::Gt(..) => "gt",
            Self::Ge(..) => "ge",
            Self::Eq(..) => "eq",
            Self::Ne(..) => "ne",
            Self::And(..) => "and",
            Self::Or(..) => "or",
            Self::Not(_) => "not",
            Self::IsFinite(_) => "is-finite",
            Self::Select { .. } => "select",
            Self::VectorCreate(_) => "vec-create",
            Self::MatrixCreate { .. } => "mat-create",
            Self::VectorIndex { .. } => "vec-index",
            Self::MatrixIndex { .. } => "mat-index",
            Self::VectorAdd(..) => "vec-add",
            Self::VectorSub(..) => "vec-sub",
            Self::VectorScale(..) => "vec-scale",
            Self::VectorDot(..) => "vec-dot",
            Self::VectorNorm(_) => "vec-norm",
            Self::VectorLength(_) => "vec-len",
            Self::MatrixAdd(..) => "mat-add",
            Self::MatrixSub(..) => "mat-sub",
            Self::MatrixScale(..) => "mat-scale",
            Self::MatrixMulVector(..) => "mat-mul-vec",
            Self::MatrixMulMatrix(..) => "mat-mul-mat",
            Self::MatrixTranspose(_) => "mat-transpose",
        }
    }
}

/// Domain obligations recorded during lowering. Phase 1 semantics: the
/// obligation is emitted as an assumption (strict-f64 IEEE behavior); no
/// silent erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainObligation {
    DivisionNonZero,
    SqrtNonNegative,
    LogPositive,
    PowFiniteResult,
}

impl DomainObligation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DivisionNonZero => "division requires a non-zero denominator",
            Self::SqrtNonNegative => "sqrt requires a non-negative argument",
            Self::LogPositive => "ln requires a strictly positive argument",
            Self::PowFiniteResult => "pow result must be finite under strict-f64 policy",
        }
    }
}

/// One lowered definition: a linear op list computing the output.
#[derive(Clone, Debug, PartialEq)]
pub struct EmirProgram {
    pub ops: Vec<(EmirOp, Span)>,
    pub result: EmirValue,
    pub input_count: u16,
    pub state_count: u16,
    pub domain_obligations: Vec<DomainObligation>,
}

impl EmirProgram {
    #[must_use]
    pub fn print(&self) -> String {
        let mut out = String::new();
        for (index, (op, _)) in self.ops.iter().enumerate() {
            let line = format!("%{index}: {}\n", op.name());
            out.push_str(&line);
        }
        let tail = format!("result: %{}\n", self.result.0);
        out.push_str(&tail);
        out
    }
}

/// Lower a Boolean requirement expression (constructor precondition).
pub fn lower_requirement(
    package: &SemanticPackage,
    expr: EmirExprRef,
    param_names: &[String],
) -> Result<EmirProgram, String> {
    lower(package, expr, param_names, &[])
}

/// Lower a definition expression. `inputs` are declaration inputs; `states`
/// are declaration state field names (referenced as `state.<name>`).
pub fn lower_definition(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    lower(package, expr, inputs, states)
}

pub type EmirExprRef = emath_ir::ExprId;

fn lower(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    let mut emitter = Emitter {
        ops: Vec::new(),
        inputs: inputs.to_vec(),
        states: states.to_vec(),
        obligations: Vec::new(),
    };
    let value = emitter.emit(package, expr)?;
    Ok(EmirProgram {
        ops: emitter.ops,
        result: value,
        input_count: u16::try_from(inputs.len()).unwrap_or(u16::MAX),
        state_count: u16::try_from(states.len()).unwrap_or(u16::MAX),
        domain_obligations: emitter.obligations,
    })
}

struct Emitter {
    ops: Vec<(EmirOp, Span)>,
    inputs: Vec<String>,
    states: Vec<String>,
    obligations: Vec<DomainObligation>,
}

impl Emitter {
    fn push(&mut self, op: EmirOp, span: Span) -> EmirValue {
        let value = EmirValue(u32::try_from(self.ops.len()).unwrap_or(u32::MAX));
        self.ops.push((op, span));
        value
    }

    fn state_index(&self, name: &str) -> Result<u16, String> {
        self.states
            .iter()
            .position(|s| s == name)
            .map(|i| u16::try_from(i).unwrap_or(u16::MAX))
            .ok_or_else(|| format!("unknown state field `{name}`"))
    }

    fn input_index(&self, name: &str) -> Result<u16, String> {
        self.inputs
            .iter()
            .position(|s| s == name)
            .map(|i| u16::try_from(i).unwrap_or(u16::MAX))
            .ok_or_else(|| format!("unknown input `{name}`"))
    }

    fn emit(&mut self, package: &SemanticPackage, id: EmirExprRef) -> Result<EmirValue, String> {
        let expr = package
            .expr(id)
            .ok_or_else(|| "expression id out of range".to_string())?;
        let span = package.expr_span(id);
        match expr {
            ExprNode::Literal(Literal::FloatBits(bits)) => {
                // NaN/Infinity policy: strict-f64 refuses non-finite constants
                let value = f64::from_bits(*bits);
                if !value.is_finite() {
                    return Err(format!(
                        "non-finite constant {value:?} refused under strict-f64 policy"
                    ));
                }
                Ok(self.push(EmirOp::ConstF64(*bits), span))
            }
            ExprNode::Literal(Literal::Integer(text)) => {
                let parsed: f64 = text
                    .replace('_', "")
                    .parse()
                    .map_err(|_| format!("invalid integer literal `{text}`"))?;
                if !parsed.is_finite() {
                    return Err(format!(
                        "integer literal `{text}` exceeds the strict-f64 finite range"
                    ));
                }
                let value: f64 = parsed;
                Ok(self.push(EmirOp::ConstF64(value.to_bits()), span))
            }
            ExprNode::Literal(Literal::Bool(on)) => {
                let value: f64 = if *on { 1.0 } else { 0.0 };
                Ok(self.push(EmirOp::ConstF64(value.to_bits()), span))
            }
            ExprNode::Literal(_) => Err("unsupported literal in Phase 1 subset".to_string()),
            ExprNode::Variable(name) => {
                let name = &name.0;
                if let Some(stripped) = name.strip_prefix("state.") {
                    let index = self.state_index(stripped)?;
                    Ok(self.push(EmirOp::LoadState(index), span))
                } else {
                    let index = self.input_index(name)?;
                    Ok(self.push(EmirOp::LoadInput(index), span))
                }
            }
            ExprNode::Call {
                function,
                arguments,
            } => self.emit_call(package, &function.0, arguments, span),
            ExprNode::Unary { operation, value } => {
                let operand = self.emit(package, *value)?;
                let op = match operation {
                    UnaryOp::Negate => EmirOp::Neg(operand),
                    UnaryOp::Not => EmirOp::Not(operand),
                    UnaryOp::Sqrt => {
                        self.obligations.push(DomainObligation::SqrtNonNegative);
                        EmirOp::Sqrt(operand)
                    }
                    UnaryOp::Exp => EmirOp::Exp(operand),
                    UnaryOp::Log => {
                        self.obligations.push(DomainObligation::LogPositive);
                        EmirOp::Ln(operand)
                    }
                    UnaryOp::Sin => EmirOp::Sin(operand),
                    UnaryOp::Cos => EmirOp::Cos(operand),
                    UnaryOp::Tan => EmirOp::Tan(operand),
                    UnaryOp::Tanh => EmirOp::Tanh(operand),
                    UnaryOp::Abs => EmirOp::Abs(operand),
                    UnaryOp::Floor => EmirOp::Floor(operand),
                    UnaryOp::Ceil => EmirOp::Ceil(operand),
                };
                Ok(self.push(op, span))
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                let l = self.emit(package, *left)?;
                let r = self.emit(package, *right)?;
                let op = match operation {
                    BinaryOp::StrictFloatAdd => EmirOp::F64Add(l, r),
                    BinaryOp::StrictFloatSub => EmirOp::F64Sub(l, r),
                    BinaryOp::StrictFloatMul => EmirOp::F64Mul(l, r),
                    BinaryOp::StrictFloatDiv => {
                        self.obligations.push(DomainObligation::DivisionNonZero);
                        EmirOp::F64Div(l, r)
                    }
                    BinaryOp::StrictFloatPow => {
                        self.obligations.push(DomainObligation::PowFiniteResult);
                        EmirOp::F64Pow(l, r)
                    }
                    BinaryOp::Equal => EmirOp::Eq(l, r),
                    BinaryOp::NotEqual => EmirOp::Ne(l, r),
                    BinaryOp::Less => EmirOp::Lt(l, r),
                    BinaryOp::LessEqual => EmirOp::Le(l, r),
                    BinaryOp::Greater => EmirOp::Gt(l, r),
                    BinaryOp::GreaterEqual => EmirOp::Ge(l, r),
                    BinaryOp::And => EmirOp::And(l, r),
                    BinaryOp::Or => EmirOp::Or(l, r),
                    BinaryOp::Min => EmirOp::Min(l, r),
                    BinaryOp::Max => EmirOp::Max(l, r),
                    BinaryOp::Atan2 => EmirOp::Atan2(l, r),
                    BinaryOp::VectorAdd => EmirOp::VectorAdd(l, r),
                    BinaryOp::VectorSub => EmirOp::VectorSub(l, r),
                    BinaryOp::VectorScale => EmirOp::VectorScale(l, r),
                    BinaryOp::VectorDot => EmirOp::VectorDot(l, r),
                    BinaryOp::MatrixAdd => EmirOp::MatrixAdd(l, r),
                    BinaryOp::MatrixSub => EmirOp::MatrixSub(l, r),
                    BinaryOp::MatrixScale => EmirOp::MatrixScale(l, r),
                    BinaryOp::MatrixMulVector => EmirOp::MatrixMulVector(l, r),
                    BinaryOp::MatrixMulMatrix => EmirOp::MatrixMulMatrix(l, r),
                    BinaryOp::ExactAdd
                    | BinaryOp::ExactSub
                    | BinaryOp::ExactMul
                    | BinaryOp::ExactDiv => {
                        return Err(
                            "exact arithmetic is outside the Phase 1 strict-f64 subset".to_string()
                        );
                    }
                };
                Ok(self.push(op, span))
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.emit(package, *condition)?;
                let then_value = self.emit(package, *then_value)?;
                let else_value = self.emit(package, *else_value)?;
                Ok(self.push(
                    EmirOp::Select {
                        condition,
                        then_value,
                        else_value,
                    },
                    span,
                ))
            }
            ExprNode::Vector(elements) => {
                let mut emitted = Vec::with_capacity(elements.len());
                for &element in elements {
                    emitted.push(self.emit(package, element)?);
                }
                Ok(self.push(EmirOp::VectorCreate(emitted), span))
            }
            ExprNode::Matrix(rows) => {
                let r = rows.len();
                let c = rows.first().map_or(0, |row| row.len());
                let mut elements = Vec::with_capacity(r * c);
                for row in rows {
                    for &element in row {
                        elements.push(self.emit(package, element)?);
                    }
                }
                Ok(self.push(
                    EmirOp::MatrixCreate {
                        rows: r,
                        cols: c,
                        elements,
                    },
                    span,
                ))
            }
            ExprNode::Index { value, indices } => {
                let target = self.emit(package, *value)?;
                if indices.len() == 1 {
                    let idx = self.emit(package, indices[0])?;
                    Ok(self.push(EmirOp::VectorIndex { vector: target, index: idx }, span))
                } else if indices.len() == 2 {
                    let row = self.emit(package, indices[0])?;
                    let col = self.emit(package, indices[1])?;
                    Ok(self.push(EmirOp::MatrixIndex { matrix: target, row, col }, span))
                } else {
                    Err("indexing with rank > 2 is outside the Phase 1 subset".to_string())
                }
            }
            other => Err(format!(
                "expression form {:?} is outside the Phase 1 strict-f64 subset",
                std::mem::discriminant(other)
            )),
        }
    }

    fn emit_call(
        &mut self,
        package: &SemanticPackage,
        function: &str,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
        // Arity is enforced in every build, debug or release (bug-hunt
        // residual: debug_assert let empty/1-arg unary calls through to an
        // indexing panic and silently dropped extras in release).
        let unary = matches!(
            function,
            "exp"
                | "ln"
                | "log"
                | "sqrt"
                | "sin"
                | "cos"
                | "tan"
                | "tanh"
                | "abs"
                | "floor"
                | "ceil"
                | "is_finite"
                | "norm"
                | "transpose"
                | "length"
                | "len"
                | "core::math::exp"
                | "core::math::ln"
                | "core::math::log"
                | "core::math::sqrt"
                | "core::math::sin"
                | "core::math::cos"
                | "core::math::tan"
                | "core::math::tanh"
                | "core::math::abs"
                | "core::math::floor"
                | "core::math::ceil"
                | "core::math::is_finite"
        );
        let binary = matches!(
            function,
            "min"
                | "max"
                | "atan2"
                | "pow"
                | "dot"
                | "core::math::min"
                | "core::math::max"
                | "core::math::atan2"
                | "core::math::pow"
        );
        let expected = match (unary, binary) {
            (true, false) => Some(1),
            (false, true) => Some(2),
            _ => None,
        };
        if let Some(expected) = expected {
            if args.len() != expected {
                return Err(format!(
                    "`{function}` expects {expected} operand(s), got {}",
                    args.len()
                ));
            }
        }
        match function {
            "exp" | "core::math::exp" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Exp(v), span))
            }
            "ln" | "log" | "core::math::ln" | "core::math::log" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                Ok(self.push(EmirOp::Ln(v), span))
            }
            "sqrt" | "core::math::sqrt" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::SqrtNonNegative);
                Ok(self.push(EmirOp::Sqrt(v), span))
            }
            "sin" | "core::math::sin" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Sin(v), span))
            }
            "cos" | "core::math::cos" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Cos(v), span))
            }
            "tan" | "core::math::tan" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Tan(v), span))
            }
            "tanh" | "core::math::tanh" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Tanh(v), span))
            }
            "abs" | "core::math::abs" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Abs(v), span))
            }
            "floor" | "core::math::floor" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Floor(v), span))
            }
            "ceil" | "core::math::ceil" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::Ceil(v), span))
            }
            "norm" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::VectorNorm(v), span))
            }
            "transpose" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::MatrixTranspose(v), span))
            }
            "length" | "len" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::VectorLength(v), span))
            }
            "dot" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                Ok(self.push(EmirOp::VectorDot(l, r), span))
            }
            "min" | "core::math::min" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                Ok(self.push(EmirOp::Min(l, r), span))
            }
            "max" | "core::math::max" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                Ok(self.push(EmirOp::Max(l, r), span))
            }
            "atan2" | "core::math::atan2" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                Ok(self.push(EmirOp::Atan2(l, r), span))
            }
            "pow" | "core::math::pow" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.obligations.push(DomainObligation::PowFiniteResult);
                Ok(self.push(EmirOp::F64Pow(l, r), span))
            }
            "is_finite" | "core::math::is_finite" => {
                let v = self.emit(package, args[0])?;
                Ok(self.push(EmirOp::IsFinite(v), span))
            }
            other => Err(format!("unknown function `{other}` in strict-f64 subset")),
        }
    }
}
