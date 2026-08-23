//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

pub mod interp;
pub mod runner;

pub use runner::{
    definition_order, simulate_continuous, simulate_continuous_with, step_continuous,
    step_continuous_values, SimulateOptions, StepMethod, Trajectory, TrajectorySample,
};

use emath_core::Span;
use emath_ir::{BinaryOp, BinderKind, ExprNode, Literal, SemanticPackage, SliceAxis, UnaryOp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmirValue(pub u32);

/// One axis of [`EmirOp::TensorSlice`]: a scalar point or a half-open range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmirSliceAxis {
    Point(EmirValue),
    Range { start: EmirValue, end: EmirValue },
}

/// Accumulation strategy for [`EmirOp::Fold`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldCombine {
    Add,
    Mul,
    And,
    Or,
}

/// Edge policy for [`EmirOp::Stencil1d`]: how out-of-range stencil indices
/// are resolved.
///
/// - `Clamp` replicates the nearest in-range cell (a first-order
///   zero-gradient / insulated boundary).
/// - `Neumann` mirrors the next interior cell across the boundary
///   (`u[-1] = u[1]`, `u[n] = u[n-2]`): a second-order zero-gradient
///   (insulated) boundary that enforces a vanishing central-difference
///   gradient at the edge.
/// - `Dirichlet { left, right }` holds the boundary at fixed values:
///   out-of-range taps read `left` (below index 0) or `right` (above the
///   last index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgePolicy {
    Clamp,
    Neumann,
    Dirichlet { left: f64, right: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmirOp {
    ConstF64(u64),
    ConstI64(i64),
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
    Round(EmirValue),
    Sign(EmirValue),
    Log2(EmirValue),
    Log10(EmirValue),
    Sinh(EmirValue),
    Cosh(EmirValue),
    Atan(EmirValue),
    Cbrt(EmirValue),
    Recip(EmirValue),
    Fract(EmirValue),
    Hypot(EmirValue, EmirValue),
    Min(EmirValue, EmirValue),
    Max(EmirValue, EmirValue),
    Atan2(EmirValue, EmirValue),
    Mod(EmirValue, EmirValue),
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
    /// 1D spatial stencil: `out[i] = sum_k weights[k] * input[i + k - center]`,
    /// with out-of-range indices resolved by `edge`. Weights are fixed at
    /// admission (e.g. `laplacian(u, dx)` lowers to weights `[1, -2, 1] / dx²`
    /// with `center = 1`). Output length equals input length.
    Stencil1d {
        input: EmirValue,
        weights: Vec<f64>,
        center: usize,
        edge: EdgePolicy,
    },
    /// 2D spatial stencil: `out[r][c] = sum_{kr,kc} weights[kr*3+kc] *
    /// input[r+kr-cr, c+kc-cc]`, with out-of-range indices resolved by
    /// `edge`. Weights are the 3x3 tap (row-major, length 9), fixed at
    /// admission (e.g. `laplacian_2d(u, dx)` lowers to the 5-point
    /// stencil `[[0,1,0],[1,-4,1],[0,1,0]] / dx²` with `center = (1,1)`).
    /// Output shape equals input shape. `Dirichlet` is not admitted for
    /// 2D in Phase 1 (use Clamp or Neumann).
    Stencil2d {
        input: EmirValue,
        weights: Vec<f64>,
        center: (usize, usize),
        edge: EdgePolicy,
    },
    MatrixAdd(EmirValue, EmirValue),
    MatrixSub(EmirValue, EmirValue),
    MatrixScale(EmirValue, EmirValue),
    MatrixMulVector(EmirValue, EmirValue),
    MatrixMulMatrix(EmirValue, EmirValue),
    MatrixTranspose(EmirValue),
    TensorCreate {
        shape: Vec<usize>,
        elements: Vec<EmirValue>,
    },
    TensorIndex {
        tensor: EmirValue,
        indices: Vec<EmirValue>,
    },
    TensorSlice {
        tensor: EmirValue,
        axes: Vec<EmirSliceAxis>,
    },
    TensorAdd(EmirValue, EmirValue),
    TensorSub(EmirValue, EmirValue),
    /// Runtime fold (sum/product/forall/exists) over a variable-bound
    /// integer range.  `body` is a sub-program evaluated once per
    /// iteration with the loop variable supplied as an extra input at
    /// `loop_var_index`.
    Fold {
        start: EmirValue,
        end: EmirValue,
        init: EmirValue,
        combine: FoldCombine,
        loop_var_index: u16,
        body: EmirProgram,
    },
    /// Numerical integration (composite Simpson's rule) over a continuous
    /// range.  `integrand` is a sub-program evaluated at each sample
    /// point with the integration variable supplied as an extra input at
    /// `loop_var_index`.  `steps` must be even.
    Integral {
        start: EmirValue,
        end: EmirValue,
        steps: u32,
        loop_var_index: u16,
        integrand: EmirProgram,
    },
    /// Forward-mode autodiff.  Evaluates `body` with dual numbers,
    /// seeding the input at `var_index` with tangent 1.0.  Returns the
    /// tangent (derivative) of the body's result.
    Differentiate {
        body: EmirProgram,
        var_index: u16,
    },
    /// Newton's-method root-finding.  Iteratively adjusts the input at
    /// `var_index` until `body` (the residual) is within `tolerance` of
    /// zero.  Each iteration uses dual-number evaluation for both the
    /// residual value and its derivative.  Returns the converged input
    /// value.
    Solve {
        body: EmirProgram,
        var_index: u16,
        tolerance: f64,
        max_iter: u32,
    },
    /// Gradient-descent optimization.  Iteratively adjusts the inputs at
    /// `var_indices` to minimize (or maximize when `maximize` is true)
    /// `body` (the objective).  Uses dual-number evaluation for the
    /// gradient (one pass per variable).  Returns the first converged
    /// input value; all variables are updated in place each iteration.
    Optimize {
        body: EmirProgram,
        var_indices: Vec<u16>,
        maximize: bool,
        learning_rate: f64,
        tolerance: f64,
        max_iter: u32,
    },
}

impl EmirOp {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::ConstI64(_) => "const-i64",
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
            Self::Round(_) => "round",
            Self::Sign(_) => "sign",
            Self::Log2(_) => "log2",
            Self::Log10(_) => "log10",
            Self::Sinh(_) => "sinh",
            Self::Cosh(_) => "cosh",
            Self::Atan(_) => "atan",
            Self::Cbrt(_) => "cbrt",
            Self::Recip(_) => "recip",
            Self::Fract(_) => "fract",
            Self::Hypot(..) => "hypot",
            Self::Min(..) => "min",
            Self::Max(..) => "max",
            Self::Atan2(..) => "atan2",
            Self::Mod(..) => "mod",
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
            Self::Stencil1d { .. } => "stencil-1d",
            Self::Stencil2d { .. } => "stencil-2d",
            Self::MatrixAdd(..) => "mat-add",
            Self::MatrixSub(..) => "mat-sub",
            Self::MatrixScale(..) => "mat-scale",
            Self::MatrixMulVector(..) => "mat-mul-vec",
            Self::MatrixMulMatrix(..) => "mat-mul-mat",
            Self::MatrixTranspose(_) => "mat-transpose",
            Self::TensorCreate { .. } => "tensor-create",
            Self::TensorIndex { .. } => "tensor-index",
            Self::TensorSlice { .. } => "tensor-slice",
            Self::TensorAdd(..) => "tensor-add",
            Self::TensorSub(..) => "tensor-sub",
            Self::Fold { .. } => "fold",
            Self::Integral { .. } => "integral",
            Self::Differentiate { .. } => "differentiate",
            Self::Solve { .. } => "solve",
            Self::Optimize { .. } => "optimize",
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
        input_count: u16_count(inputs.len(), "input")?,
        state_count: u16_count(states.len(), "state")?,
        domain_obligations: emitter.obligations,
    })
}

fn u16_count(n: usize, what: &str) -> Result<u16, String> {
    u16::try_from(n).map_err(|_| format!("{what} count {n} exceeds u16::MAX"))
}

fn u16_index(n: usize, what: &str) -> Result<u16, String> {
    u16::try_from(n).map_err(|_| format!("{what} index {n} exceeds u16::MAX"))
}

struct Emitter {
    ops: Vec<(EmirOp, Span)>,
    inputs: Vec<String>,
    states: Vec<String>,
    obligations: Vec<DomainObligation>,
}

impl Emitter {
    fn push(&mut self, op: EmirOp, span: Span) -> Result<EmirValue, String> {
        let id = u32::try_from(self.ops.len())
            .map_err(|_| format!("EMIR op count {} exceeds u32::MAX", self.ops.len()))?;
        self.ops.push((op, span));
        Ok(EmirValue(id))
    }

    fn state_index(&self, name: &str) -> Result<u16, String> {
        self.states
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| format!("unknown state field `{name}`"))
            .and_then(|i| u16_index(i, "state"))
    }

    fn input_index(&self, name: &str) -> Result<u16, String> {
        self.inputs
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| format!("unknown input `{name}`"))
            .and_then(|i| u16_index(i, "input"))
    }

    /// Create a sub-emitter that shares this emitter's input and state
    /// tables.  Used by nested ops (`Differentiate`, `Solve`,
    /// `Optimize`) so the sub-program can reference the same inputs.
    fn sub_emitter(&self) -> Emitter {
        Emitter {
            ops: Vec::new(),
            inputs: self.inputs.clone(),
            states: self.states.clone(),
            obligations: Vec::new(),
        }
    }

    /// Finalize this emitter into an `EmirProgram` with the given result
    /// value.  The state count comes from the parent emitter since
    /// sub-programs share state tables.
    fn finish(mut self, result: EmirValue, state_count: usize) -> Result<EmirProgram, String> {
        Ok(EmirProgram {
            ops: std::mem::take(&mut self.ops),
            result,
            input_count: u16_count(self.inputs.len(), "input")?,
            state_count: u16_count(state_count, "state")?,
            domain_obligations: std::mem::take(&mut self.obligations),
        })
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
                self.push(EmirOp::ConstF64(*bits), span)
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
                self.push(EmirOp::ConstF64(value.to_bits()), span)
            }
            ExprNode::Literal(Literal::Bool(on)) => {
                let value: f64 = if *on { 1.0 } else { 0.0 };
                self.push(EmirOp::ConstF64(value.to_bits()), span)
            }
            ExprNode::Literal(_) => Err("unsupported literal in Phase 1 subset".to_string()),
            ExprNode::Variable(name) => {
                let name = &name.0;
                if let Some(stripped) = name.strip_prefix("state.") {
                    let index = self.state_index(stripped)?;
                    self.push(EmirOp::LoadState(index), span)
                } else {
                    let index = self.input_index(name)?;
                    self.push(EmirOp::LoadInput(index), span)
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
                self.push(op, span)
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
                    BinaryOp::TensorAdd => EmirOp::TensorAdd(l, r),
                    BinaryOp::TensorSub => EmirOp::TensorSub(l, r),
                    BinaryOp::ExactAdd
                    | BinaryOp::ExactSub
                    | BinaryOp::ExactMul
                    | BinaryOp::ExactDiv => {
                        return Err(
                            "exact arithmetic is outside the Phase 1 strict-f64 subset".to_string()
                        );
                    }
                };
                self.push(op, span)
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.emit(package, *condition)?;
                let then_value = self.emit(package, *then_value)?;
                let else_value = self.emit(package, *else_value)?;
                self.push(
                    EmirOp::Select {
                        condition,
                        then_value,
                        else_value,
                    },
                    span,
                )
            }
            ExprNode::Vector(elements) => {
                let mut emitted = Vec::with_capacity(elements.len());
                for &element in elements {
                    emitted.push(self.emit(package, element)?);
                }
                self.push(EmirOp::VectorCreate(emitted), span)
            }
            ExprNode::Matrix(rows) => {
                let r = rows.len();
                let c = rows.first().map_or(0, |row| row.len());
                if rows.iter().any(|row| row.len() != c) {
                    return Err("jagged matrix rows (unequal column counts)".into());
                }
                let mut elements = Vec::with_capacity(r.saturating_mul(c));
                for row in rows {
                    for &element in row {
                        elements.push(self.emit(package, element)?);
                    }
                }
                self.push(
                    EmirOp::MatrixCreate {
                        rows: r,
                        cols: c,
                        elements,
                    },
                    span,
                )
            }
            ExprNode::Tensor { shape, elements } => {
                let expected = shape
                    .iter()
                    .try_fold(1usize, |acc, &dim| acc.checked_mul(dim));
                let Some(expected) = expected else {
                    return Err("tensor shape product overflow".into());
                };
                if elements.len() != expected {
                    return Err("tensor element count does not match shape product".into());
                }
                let mut emitted = Vec::with_capacity(elements.len());
                for &element in elements {
                    emitted.push(self.emit(package, element)?);
                }
                self.push(
                    EmirOp::TensorCreate {
                        shape: shape.clone(),
                        elements: emitted,
                    },
                    span,
                )
            }
            ExprNode::Index { value, indices } => {
                let target = self.emit(package, *value)?;
                if indices.len() == 1 {
                    let idx = self.emit(package, indices[0])?;
                    self.push(EmirOp::VectorIndex { vector: target, index: idx }, span)
                } else if indices.len() == 2 {
                    let row = self.emit(package, indices[0])?;
                    let col = self.emit(package, indices[1])?;
                    self.push(EmirOp::MatrixIndex { matrix: target, row, col }, span)
                } else {
                    let mut emitted = Vec::with_capacity(indices.len());
                    for &index in indices {
                        emitted.push(self.emit(package, index)?);
                    }
                    self.push(
                        EmirOp::TensorIndex {
                            tensor: target,
                            indices: emitted,
                        },
                        span,
                    )
                }
            }
            ExprNode::Slice { value, axes } => {
                let target = self.emit(package, *value)?;
                let mut emitted = Vec::with_capacity(axes.len());
                for axis in axes {
                    emitted.push(match axis {
                        SliceAxis::Point(index) => EmirSliceAxis::Point(self.emit(package, *index)?),
                        SliceAxis::Range { start, end } => EmirSliceAxis::Range {
                            start: self.emit(package, *start)?,
                            end: self.emit(package, *end)?,
                        },
                    });
                }
                self.push(
                    EmirOp::TensorSlice {
                        tensor: target,
                        axes: emitted,
                    },
                    span,
                )
            }
            ExprNode::Binder {
                kind,
                variables,
                body,
            } => {
                if variables.len() != 1 {
                    return Err(
                        "only a single binder variable is computed".to_string()
                    );
                }
                let binder = &variables[0];
                let domain_expr = package
                    .expr(binder.domain)
                    .ok_or_else(|| "binder domain out of range".to_string())?;
                let (start_id, end_id) = match domain_expr {
                    ExprNode::Vector(els) if els.len() == 2 => (els[0], els[1]),
                    _ => {
                        return Err(
                            "binder domain must be a range vector".to_string()
                        )
                    }
                };
                let start_val = self.emit(package, start_id)?;
                let end_val = self.emit(package, end_id)?;
                let loop_var_index = u16_index(self.inputs.len(), "loop variable")?;
                let mut body_inputs = self.inputs.clone();
                body_inputs.push(binder.name.clone());
                let mut body_emitter = Emitter {
                    ops: Vec::new(),
                    inputs: body_inputs,
                    states: self.states.clone(),
                    obligations: Vec::new(),
                };
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = EmirProgram {
                    ops: std::mem::take(&mut body_emitter.ops),
                    result: body_result,
                    input_count: u16_count(body_emitter.inputs.len(), "input")?,
                    state_count: u16_count(self.states.len(), "state")?,
                    domain_obligations: std::mem::take(&mut body_emitter.obligations),
                };
                match kind {
                    BinderKind::Sum
                    | BinderKind::Product
                    | BinderKind::ForAll
                    | BinderKind::Exists => {
                        let combine = match kind {
                            BinderKind::Sum => FoldCombine::Add,
                            BinderKind::Product => FoldCombine::Mul,
                            BinderKind::ForAll => FoldCombine::And,
                            BinderKind::Exists => FoldCombine::Or,
                            BinderKind::Integral => {
                                return Err(
                                    "integral binder must lower via Integral op".to_string(),
                                );
                            }
                        };
                        let init_val = match combine {
                            FoldCombine::Add => self.push(EmirOp::ConstI64(0), span)?,
                            FoldCombine::Mul => self.push(EmirOp::ConstI64(1), span)?,
                            FoldCombine::And => {
                                self.push(EmirOp::ConstF64(1.0f64.to_bits()), span)?
                            }
                            FoldCombine::Or => {
                                self.push(EmirOp::ConstF64(0.0f64.to_bits()), span)?
                            }
                        };
                        self.push(
                            EmirOp::Fold {
                                start: start_val,
                                end: end_val,
                                init: init_val,
                                combine,
                                loop_var_index,
                                body: body_program,
                            },
                            span,
                        )
                    }
                    BinderKind::Integral => self.push(
                        EmirOp::Integral {
                            start: start_val,
                            end: end_val,
                            steps: 1000,
                            loop_var_index,
                            integrand: body_program,
                        },
                        span,
                    ),
                }
            }
            ExprNode::Differentiate { body, var } => {
                let var_index = self.input_index(var)?;
                let mut body_emitter = Emitter {
                    ops: Vec::new(),
                    inputs: self.inputs.clone(),
                    states: self.states.clone(),
                    obligations: Vec::new(),
                };
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = EmirProgram {
                    ops: std::mem::take(&mut body_emitter.ops),
                    result: body_result,
                    input_count: u16_count(body_emitter.inputs.len(), "input")?,
                    state_count: u16_count(self.states.len(), "state")?,
                    domain_obligations: std::mem::take(&mut body_emitter.obligations),
                };
                self.push(
                    EmirOp::Differentiate {
                        body: body_program,
                        var_index,
                    },
                    span,
                )
            }
            ExprNode::Solve { body, var } => {
                let var_index = self.input_index(var)?;
                let sc = self.states.len();
                let mut body_emitter = self.sub_emitter();
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = body_emitter.finish(body_result, sc)?;
                self.push(
                    EmirOp::Solve {
                        body: body_program,
                        var_index,
                        tolerance: 1e-12,
                        max_iter: 100,
                    },
                    span,
                )
            }
            ExprNode::Optimize { body, vars, maximize } => {
                let mut var_indices = Vec::with_capacity(vars.len());
                for var in vars {
                    var_indices.push(self.input_index(var)?);
                }
                let sc = self.states.len();
                let mut body_emitter = self.sub_emitter();
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = body_emitter.finish(body_result, sc)?;
                self.push(
                    EmirOp::Optimize {
                        body: body_program,
                        var_indices,
                        maximize: *maximize,
                        learning_rate: 0.01,
                        // 1e-6 converges comfortably within max_iter for
                        // well-conditioned quadratics; 1e-10 (the prior
                        // value) is unreachable at lr=0.01 in 1000 steps
                        // (gradient floors near 1e-8), so every optimize
                        // call faulted with "did not converge".
                        tolerance: 1e-6,
                        max_iter: 1000,
                    },
                    span,
                )
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
                | "round"
                | "sign"
                | "log2"
                | "log10"
                | "sinh"
                | "cosh"
                | "atan"
                | "cbrt"
                | "recip"
                | "fract"
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
                | "mod"
                | "hypot"
                | "dot"
                | "core::math::min"
                | "core::math::max"
                | "core::math::atan2"
                | "core::math::pow"
                | "core::math::mod"
                | "core::math::hypot"
        );
        let ternary = matches!(function, "lerp" | "core::math::lerp" | "clamp" | "core::math::clamp");
        let expected = match (unary, binary, ternary) {
            (true, false, false) => Some(1),
            (false, true, false) => Some(2),
            (false, false, true) => Some(3),
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
                self.push(EmirOp::Exp(v), span)
            }
            "ln" | "log" | "core::math::ln" | "core::math::log" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Ln(v), span)
            }
            "sqrt" | "core::math::sqrt" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::SqrtNonNegative);
                self.push(EmirOp::Sqrt(v), span)
            }
            "sin" | "core::math::sin" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sin(v), span)
            }
            "cos" | "core::math::cos" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cos(v), span)
            }
            "tan" | "core::math::tan" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Tan(v), span)
            }
            "tanh" | "core::math::tanh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Tanh(v), span)
            }
            "abs" | "core::math::abs" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Abs(v), span)
            }
            "floor" | "core::math::floor" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Floor(v), span)
            }
            "ceil" | "core::math::ceil" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Ceil(v), span)
            }
            "round" | "core::math::round" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Round(v), span)
            }
            "sign" | "core::math::sign" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sign(v), span)
            }
            "log2" | "core::math::log2" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Log2(v), span)
            }
            "log10" | "core::math::log10" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Log10(v), span)
            }
            "sinh" | "core::math::sinh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sinh(v), span)
            }
            "cosh" | "core::math::cosh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cosh(v), span)
            }
            "atan" | "core::math::atan" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Atan(v), span)
            }
            "cbrt" | "core::math::cbrt" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cbrt(v), span)
            }
            "recip" | "core::math::recip" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Recip(v), span)
            }
            "fract" | "core::math::fract" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Fract(v), span)
            }
            "norm" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::VectorNorm(v), span)
            }
            "laplacian" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`laplacian` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                // Phase 1: dx must be a positive literal so the stencil weights
                // are fixed at emission (no runtime division, no IEEE inf/NaN).
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Clamp,
                    },
                    span,
                )
            }
            "laplacian_neumann" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`laplacian_neumann` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian_neumann` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian_neumann` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Neumann,
                    },
                    span,
                )
            }
            "laplacian_dirichlet" => {
                if args.len() != 4 {
                    return Err(format!(
                        "`laplacian_dirichlet` expects 4 operands (vector, dx, g_left, g_right), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian_dirichlet` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian_dirichlet` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let g_left = match package.expr(args[2]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => f64::from_bits(*bits),
                    _ => return Err(
                        "`laplacian_dirichlet` left boundary value must be a literal constant in Phase 1"
                            .to_string(),
                    ),
                };
                let g_right = match package.expr(args[3]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => f64::from_bits(*bits),
                    _ => return Err(
                        "`laplacian_dirichlet` right boundary value must be a literal constant in Phase 1"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Dirichlet { left: g_left, right: g_right },
                    },
                    span,
                )
            }
            "laplacian_2d" | "laplacian_2d_neumann" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`{function}` expects 2 operands (matrix, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`{function}` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(format!(
                        "`{function}` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                    )),
                };
                let inv = 1.0 / (dx * dx);
                // 5-point Laplacian: [[0,1,0],[1,-4,1],[0,1,0]] / dx^2.
                let weights = vec![
                    0.0, inv, 0.0,
                    inv, -4.0 * inv, inv,
                    0.0, inv, 0.0,
                ];
                let edge = if function == "laplacian_2d_neumann" {
                    EdgePolicy::Neumann
                } else {
                    EdgePolicy::Clamp
                };
                self.push(
                    EmirOp::Stencil2d {
                        input,
                        weights,
                        center: (1, 1),
                        edge,
                    },
                    span,
                )
            }
            "transpose" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::MatrixTranspose(v), span)
            }
            "length" | "len" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::VectorLength(v), span)
            }
            "dot" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::VectorDot(l, r), span)
            }
            "min" | "core::math::min" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Min(l, r), span)
            }
            "max" | "core::math::max" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Max(l, r), span)
            }
            "atan2" | "core::math::atan2" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Atan2(l, r), span)
            }
            "mod" | "core::math::mod" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Mod(l, r), span)
            }
            "hypot" | "core::math::hypot" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Hypot(l, r), span)
            }
            "pow" | "core::math::pow" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.obligations.push(DomainObligation::PowFiniteResult);
                self.push(EmirOp::F64Pow(l, r), span)
            }
            "is_finite" | "core::math::is_finite" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::IsFinite(v), span)
            }
            "lerp" | "core::math::lerp" => {
                // lerp(a, b, t) = a + (b - a) * t
                let va = self.emit(package, args[0])?;
                let vb = self.emit(package, args[1])?;
                let vt = self.emit(package, args[2])?;
                let diff = self.push(EmirOp::F64Sub(vb, va), span)?;
                let interp = self.push(EmirOp::F64Mul(diff, vt), span)?;
                self.push(EmirOp::F64Add(va, interp), span)
            }
            "clamp" | "core::math::clamp" => {
                // clamp(x, lo, hi) = min(max(x, lo), hi)
                let vx = self.emit(package, args[0])?;
                let vlo = self.emit(package, args[1])?;
                let vhi = self.emit(package, args[2])?;
                let lo_clamped = self.push(EmirOp::Max(vx, vlo), span)?;
                self.push(EmirOp::Min(lo_clamped, vhi), span)
            }
            other => Err(format!("unknown function `{other}` in strict-f64 subset")),
        }
    }
}
