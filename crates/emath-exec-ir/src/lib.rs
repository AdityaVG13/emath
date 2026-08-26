//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

pub mod builtin;
mod emitter;
pub mod interp;
pub mod optimize;
pub mod runner;

pub use builtin::BuiltinId;
pub use runner::{
    SimulateOptions, StepMethod, Trajectory, TrajectorySample, definition_order,
    simulate_continuous, simulate_continuous_with, step_continuous, step_continuous_values,
};

use emath_core::Span;
use emath_ir::SemanticPackage;

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

/// How out-of-range stencil indices resolve: `Clamp` (replicate the edge
/// cell), `Neumann` (mirror the next interior cell), `OneSided` (linear
/// extrapolation; first-order one-sided first differences), or `Dirichlet`
/// (fixed boundary values). 2D admits `Clamp`/`Neumann`/`OneSided` in Phase 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgePolicy {
    Clamp,
    Neumann,
    OneSided,
    Dirichlet { left: f64, right: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmirOp {
    ConstF64(u64),
    ConstI64(i64),
    /// Complex constant (real, imaginary). B14.
    ConstComplex(f64, f64),
    /// Boolean constant; produced by optimizer folding.
    ConstBool(bool),
    LoadInput(u16),
    LoadState(u16),
    F64Add(EmirValue, EmirValue),
    F64Sub(EmirValue, EmirValue),
    F64Mul(EmirValue, EmirValue),
    F64Div(EmirValue, EmirValue),
    F64Pow(EmirValue, EmirValue),
    Neg(EmirValue),
    /// Generic unary/binary math builtin via the `BuiltinId` registry.
    UnaryBuiltin(BuiltinId, EmirValue),
    BinaryBuiltin(BuiltinId, EmirValue, EmirValue),
    Lt(EmirValue, EmirValue),
    Le(EmirValue, EmirValue),
    Gt(EmirValue, EmirValue),
    Ge(EmirValue, EmirValue),
    Eq(EmirValue, EmirValue),
    Ne(EmirValue, EmirValue),
    And(EmirValue, EmirValue),
    Or(EmirValue, EmirValue),
    /// `==>` — `!a || b`
    Imply(EmirValue, EmirValue),
    /// `<==>` — `a == b` for Bool
    Iff(EmirValue, EmirValue),
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
    /// 1D convolution with fixed weights and an edge policy; output length
    /// equals input length.
    Stencil1d {
        input: EmirValue,
        weights: Vec<f64>,
        center: usize,
        edge: EdgePolicy,
    },
    /// 2D 3x3 stencil convolution (row-major weights, length 9); output
    /// shape equals input shape. `Dirichlet` is not admitted in Phase 1.
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
    /// Einstein summation over the given subscripts (e.g. `"ik,kj->ij"`).
    Einsum {
        subscripts: String,
        inputs: Vec<EmirValue>,
    },
    /// Exact i64 factorial / modular inverse / congruence check.
    Factorial(EmirValue),
    ModInv(EmirValue, EmirValue),
    Congruence(EmirValue, EmirValue, EmirValue),
    /// Horner polynomial evaluation over GF(p) / Reed-Solomon encode /
    /// Hamming distance (RS proximity machinery).
    PolyEvalMod(EmirValue, EmirValue, EmirValue),
    RSEncode(EmirValue, EmirValue, EmirValue),
    HammingDistance(EmirValue, EmirValue),
    /// Fold sum/product/forall/exists over an integer range; `body` runs
    /// once per iteration with the loop variable as an extra input.
    Fold {
        start: EmirValue,
        end: EmirValue,
        init: EmirValue,
        combine: FoldCombine,
        loop_var_index: u16,
        body: EmirProgram,
    },
    /// Composite Simpson integration; `integrand` runs per sample point,
    /// `steps` must be even.
    Integral {
        start: EmirValue,
        end: EmirValue,
        steps: u32,
        loop_var_index: u16,
        integrand: EmirProgram,
    },
    /// Dual-number forward-mode derivative of `body` w.r.t. `var_index`.
    Differentiate {
        body: EmirProgram,
        var_index: u16,
    },
    /// Newton root-finding on `body` (the residual) w.r.t. `var_index`.
    Solve {
        body: EmirProgram,
        var_index: u16,
        tolerance: f64,
        max_iter: u32,
    },
    /// Newton on ∇f = 0 over `body` w.r.t. `var_indices`.
    Optimize {
        body: EmirProgram,
        var_indices: Vec<u16>,
        maximize: bool,
        learning_rate: f64,
        tolerance: f64,
        max_iter: u32,
    },
    /// Numerical limit: sample `body` approaching `target` from
    /// `direction` (0 = two-sided, +1 = above, -1 = below).
    SampleLimit {
        body: EmirProgram,
        var_index: u16,
        target: EmirValue,
        direction: EmirValue,
    },
    /// Adjoint-method reverse AD: one forward + one backward pass gives
    /// gradients w.r.t. all `var_indices` at O(cost).
    ReverseMode {
        body: EmirProgram,
        var_indices: Vec<u16>,
    },
}

impl EmirOp {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::ConstI64(_) => "const-i64",
            Self::ConstComplex(..) => "const-complex",
            Self::ConstBool(_) => "const-bool",
            Self::LoadInput(_) => "load-input",
            Self::LoadState(_) => "load-state",
            Self::F64Add(..) => "f64-add",
            Self::F64Sub(..) => "f64-sub",
            Self::F64Mul(..) => "f64-mul",
            Self::F64Div(..) => "f64-div",
            Self::F64Pow(..) => "f64-pow",
            Self::Neg(_) => "neg",
            Self::UnaryBuiltin(id, _) => id.name(),
            Self::BinaryBuiltin(id, _, _) => id.name(),
            Self::Lt(..) => "lt",
            Self::Le(..) => "le",
            Self::Gt(..) => "gt",
            Self::Ge(..) => "ge",
            Self::Eq(..) => "eq",
            Self::Ne(..) => "ne",
            Self::And(..) => "and",
            Self::Or(..) => "or",
            Self::Imply(..) => "imply",
            Self::Iff(..) => "iff",
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
            Self::Einsum { .. } => "einsum",
            Self::Factorial(..) => "factorial",
            Self::ModInv(..) => "mod-inv",
            Self::Congruence(..) => "congruence",
            Self::PolyEvalMod(..) => "poly-eval-mod",
            Self::RSEncode(..) => "rs-encode",
            Self::HammingDistance(..) => "hamming-distance",
            Self::Fold { .. } => "fold",
            Self::Integral { .. } => "integral",
            Self::Differentiate { .. } => "differentiate",
            Self::Solve { .. } => "solve",
            Self::Optimize { .. } => "optimize",
            Self::SampleLimit { .. } => "sample-limit",
            Self::ReverseMode { .. } => "reverse-mode",
        }
    }

    /// SSA dump of this op: name, register operands, and non-register payloads.
    /// Nested sub-programs are omitted here; [`EmirProgram::print`] dumps them.
    #[must_use]
    pub fn format_ssa(&self) -> String {
        match self {
            Self::ConstF64(bits) => format!("const-f64 {bits:016x}"),
            Self::ConstI64(value) => format!("const-i64 {value}"),
            Self::ConstBool(value) => format!("const-bool {value}"),
            Self::ConstComplex(re, im) => {
                format!("const-complex {:016x} {:016x}", re.to_bits(), im.to_bits())
            }
            Self::LoadInput(index) => format!("load-input {index}"),
            Self::LoadState(index) => format!("load-state {index}"),
            Self::Select {
                condition,
                then_value,
                else_value,
            } => format!(
                "select %{} %{} %{}",
                condition.0, then_value.0, else_value.0
            ),
            Self::MatrixCreate {
                rows,
                cols,
                elements,
            } => format!("mat-create {rows} {cols} {}", format_regs(elements)),
            Self::TensorCreate { shape, elements } => format!(
                "tensor-create {} {}",
                format_shape(shape),
                format_regs(elements)
            ),
            Self::TensorSlice { tensor, axes } => {
                let mut out = format!("tensor-slice %{}", tensor.0);
                for axis in axes {
                    match axis {
                        EmirSliceAxis::Point(v) => out.push_str(&format!(" point %{}", v.0)),
                        EmirSliceAxis::Range { start, end } => {
                            out.push_str(&format!(" range %{} %{}", start.0, end.0));
                        }
                    }
                }
                out
            }
            Self::Stencil1d {
                input,
                weights,
                center,
                edge,
            } => format!(
                "stencil-1d %{} center={center} {} {}",
                input.0,
                format_edge(edge),
                format_f64_bits(weights)
            ),
            Self::Stencil2d {
                input,
                weights,
                center,
                edge,
            } => format!(
                "stencil-2d %{} center={},{} {} {}",
                input.0,
                center.0,
                center.1,
                format_edge(edge),
                format_f64_bits(weights)
            ),
            Self::Einsum { subscripts, inputs } => {
                format!("einsum {subscripts} {}", format_regs(inputs))
            }
            Self::Fold {
                start,
                end,
                init,
                combine,
                loop_var_index,
                ..
            } => format!(
                "fold {} start=%{} end=%{} init=%{} loop={loop_var_index}",
                fold_combine_name(*combine),
                start.0,
                end.0,
                init.0
            ),
            Self::Integral {
                start,
                end,
                steps,
                loop_var_index,
                ..
            } => format!(
                "integral steps={steps} start=%{} end=%{} loop={loop_var_index}",
                start.0, end.0
            ),
            Self::Differentiate { var_index, .. } => {
                format!("differentiate var={var_index}")
            }
            Self::Solve {
                var_index,
                tolerance,
                max_iter,
                ..
            } => format!(
                "solve var={var_index} tol={:016x} max={max_iter}",
                tolerance.to_bits()
            ),
            Self::Optimize {
                var_indices,
                maximize,
                learning_rate,
                tolerance,
                max_iter,
                ..
            } => format!(
                "optimize maximize={maximize} lr={:016x} tol={:016x} max={max_iter} vars={}",
                learning_rate.to_bits(),
                tolerance.to_bits(),
                format_u16s(var_indices)
            ),
            Self::SampleLimit {
                var_index,
                target,
                direction,
                ..
            } => format!(
                "sample-limit var={var_index} target=%{} direction=%{}",
                target.0, direction.0
            ),
            Self::ReverseMode { var_indices, .. } => {
                format!("reverse-mode vars={}", format_u16s(var_indices))
            }
            other => {
                let mut operands = Vec::new();
                optimize::operand_registers(other, &mut operands);
                let mut out = other.name().to_string();
                if !operands.is_empty() {
                    out.push(' ');
                    out.push_str(&format_regs(&operands));
                }
                out
            }
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
    /// Deterministic SSA dump. Distinct register operands, constant
    /// payloads, nested bodies, counts, and obligations produce distinct
    /// bytes; `op.name()`-only dumps used to collide on those.
    #[must_use]
    pub fn print(&self) -> String {
        let mut out = String::new();
        self.write_print(&mut out, 0);
        out
    }

    fn write_print(&self, out: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        out.push_str(&pad);
        out.push_str(&format!("inputs: {}\n", self.input_count));
        out.push_str(&pad);
        out.push_str(&format!("states: {}\n", self.state_count));
        for (index, (op, _)) in self.ops.iter().enumerate() {
            out.push_str(&pad);
            out.push_str(&format!("%{index}: {}\n", op.format_ssa()));
            write_nested_programs(out, op, indent + 1);
        }
        out.push_str(&pad);
        out.push_str(&format!("result: %{}\n", self.result.0));
        for obligation in &self.domain_obligations {
            out.push_str(&pad);
            out.push_str("obligation: ");
            out.push_str(obligation.as_str());
            out.push('\n');
        }
    }
}

fn format_regs(values: &[EmirValue]) -> String {
    let mut out = String::new();
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push('%');
        out.push_str(&value.0.to_string());
    }
    out
}

fn format_shape(shape: &[usize]) -> String {
    shape
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("x")
}

fn format_u16s(values: &[u16]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_f64_bits(values: &[f64]) -> String {
    values
        .iter()
        .map(|value| format!("{:016x}", value.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_edge(edge: &EdgePolicy) -> String {
    match edge {
        EdgePolicy::Clamp => "clamp".to_string(),
        EdgePolicy::Neumann => "neumann".to_string(),
        EdgePolicy::OneSided => "onesided".to_string(),
        EdgePolicy::Dirichlet { left, right } => {
            format!("dirichlet {:016x} {:016x}", left.to_bits(), right.to_bits())
        }
    }
}

fn fold_combine_name(combine: FoldCombine) -> &'static str {
    match combine {
        FoldCombine::Add => "add",
        FoldCombine::Mul => "mul",
        FoldCombine::And => "and",
        FoldCombine::Or => "or",
    }
}

fn write_nested_programs(out: &mut String, op: &EmirOp, indent: usize) {
    let pad = "  ".repeat(indent);
    match op {
        EmirOp::Fold { body, .. }
        | EmirOp::Differentiate { body, .. }
        | EmirOp::Solve { body, .. }
        | EmirOp::Optimize { body, .. }
        | EmirOp::SampleLimit { body, .. }
        | EmirOp::ReverseMode { body, .. } => {
            out.push_str(&pad);
            out.push_str("body:\n");
            body.write_print(out, indent + 1);
        }
        EmirOp::Integral { integrand, .. } => {
            out.push_str(&pad);
            out.push_str("integrand:\n");
            integrand.write_print(out, indent + 1);
        }
        _ => {}
    }
}

/// Lower a Boolean requirement expression (constructor precondition).
pub fn lower_requirement(
    package: &SemanticPackage,
    expr: EmirExprRef,
    param_names: &[String],
) -> Result<EmirProgram, String> {
    let mut program = emitter::lower(package, expr, param_names, &[])?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

/// Lower a definition expression. `inputs` are declaration inputs; `states`
/// are declaration state field names (referenced as `state.<name>`).
pub fn lower_definition(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    let mut program = emitter::lower(package, expr, inputs, states)?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

pub type EmirExprRef = emath_ir::ExprId;
