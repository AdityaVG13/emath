//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

mod emitter;
pub mod builtin;
pub mod interp;
pub mod optimize;
pub mod runner;

pub use builtin::BuiltinId;
pub use runner::{
    definition_order, simulate_continuous, simulate_continuous_with, step_continuous,
    step_continuous_values, SimulateOptions, StepMethod, Trajectory, TrajectorySample,
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
/// cell), `Neumann` (mirror the next interior cell), or `Dirichlet`
/// (fixed boundary values). 2D admits only `Clamp`/`Neumann` in Phase 1.
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
    /// Gradient descent (or ascent) over `body` w.r.t. `var_indices`.
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

