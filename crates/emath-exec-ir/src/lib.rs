//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

mod emitter;
pub mod interp;
pub mod runner;

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
    /// Complex constant (real, imaginary). B14.
    ConstComplex(f64, f64),
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
    /// `einsum("ik,kj->ij", A, B)` — Einstein summation contraction.
    /// `subscripts` is the subscript string (e.g., "ik,kj->ij").
    /// `inputs` are the operand value IDs. The interp parses the
    /// subscripts, determines index sizes from operand shapes, and
    /// computes the contracted result.
    Einsum {
        subscripts: String,
        inputs: Vec<EmirValue>,
    },
    /// `factorial(n)` — exact i64 factorial. B15/B17.
    Factorial(EmirValue),
    /// `mod_inv(a, m)` — modular inverse via extended GCD. B15.
    ModInv(EmirValue, EmirValue),
    /// `cong(a, b, m)` — congruence check: (a - b) % m == 0. B15.
    Congruence(EmirValue, EmirValue, EmirValue),
    /// `poly_eval_mod(coeffs, x, p)` — Horner-method polynomial
    /// evaluation over GF(p). `coeffs` is a Vector of i64 coefficients
    /// (c[0] + c[1]*x + ... + c[k-1]*x^(k-1)), `x` and `p` are i64.
    /// Returns i64 result mod p. For RS code construction.
    PolyEvalMod(EmirValue, EmirValue, EmirValue),
    /// `rs_encode(coeffs, n, p)` — construct Reed-Solomon codeword by
    /// evaluating polynomial at points 0..n over GF(p). Returns Vector
    /// of n i64-as-f64 values. For RS proximity testing.
    RSEncode(EmirValue, EmirValue, EmirValue),
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
            Self::ConstComplex(..) => "const-complex",
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
    emitter::lower(package, expr, param_names, &[])
}

/// Lower a definition expression. `inputs` are declaration inputs; `states`
/// are declaration state field names (referenced as `state.<name>`).
pub fn lower_definition(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    emitter::lower(package, expr, inputs, states)
}

pub type EmirExprRef = emath_ir::ExprId;

