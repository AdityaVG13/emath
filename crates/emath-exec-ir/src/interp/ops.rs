//! The EmirOp dispatch interpreter, extracted from interp.rs.
//! Mechanical move - no logic changes.

use super::*;

pub(super) fn eval_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    let name = op.name();
    match *op {
        EmirOp::ConstF64(..)
        | EmirOp::ConstI64(..)
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(..)
        | EmirOp::FormatText { .. }
        | EmirOp::TextLength(..)
        | EmirOp::TextNfc(..)
        | EmirOp::SpecialFunction { .. }
        | EmirOp::ReportSection { .. }
        | EmirOp::ReportDocument { .. }
        | EmirOp::ReportMarkdown(..)
        | EmirOp::ReportLatex(..)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::SeriesSample { .. }
        | EmirOp::SetCreate { .. }
        | EmirOp::SetContains { .. }
        | EmirOp::RecordCreate { .. }
        | EmirOp::ConstComplex(..)
        | EmirOp::RatConstruct { .. }
        | EmirOp::RatAdd(..)
        | EmirOp::RatNorm(..)
        | EmirOp::ConstBool(..)
        | EmirOp::LoadInput(..)
        | EmirOp::LoadState(..)
        | EmirOp::ApplyCapability { .. }
        | EmirOp::ProgramLiteral(..)
        | EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. }
        | EmirOp::VectorAllFinite(_)
        | EmirOp::IntervalCreate(..)
        | EmirOp::IntervalIntersect(..) => eval_data_op(op, registers, inputs, state, name),
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::Neg(..)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(..)
        | EmirOp::IsFinite(..) => eval_arith_op(op, registers, name),
        EmirOp::Select { .. }
        | EmirOp::VectorCreate(..)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::VectorAdd(..)
        | EmirOp::VectorSub(..)
        | EmirOp::VectorScale(..)
        | EmirOp::VectorDot(..)
        | EmirOp::VectorNorm(..)
        | EmirOp::VectorLength(..)
        | EmirOp::Stencil1d { .. }
        | EmirOp::Stencil2d { .. }
        | EmirOp::Stencil3d { .. }
        | EmirOp::MatrixAdd(..)
        | EmirOp::MatrixSub(..)
        | EmirOp::MatrixScale(..)
        | EmirOp::MatrixMulVector(..)
        | EmirOp::MatrixMulMatrix(..)
        | EmirOp::MatrixTranspose(..)
        | EmirOp::EigenSymmetric(..)
        | EmirOp::EigenVectorsSymmetric(..)
        | EmirOp::SvdSingularValues(..)
        | EmirOp::SvdFactors(..)
        | EmirOp::CgSolve(..)
        | EmirOp::LinearSolve(..)
        | EmirOp::LuFactors(..)
        | EmirOp::QrFactors(..)
        | EmirOp::OuterProduct(..)
        | EmirOp::GraphReachable(..)
        | EmirOp::GraphBfsOrder(..)
        | EmirOp::GraphDijkstra(..)
        | EmirOp::GraphDegreeOut(..)
        | EmirOp::GraphLaplacian(..)
        | EmirOp::GraphSymmetrize(..)
        | EmirOp::GraphBellmanFord(..) => eval_linalg_op(op, registers, name),
        EmirOp::OptionSome(..)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(..)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(..)
        | EmirOp::ResultErr(..)
        | EmirOp::ResultIsOk(..)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(..) => eval_carrier_op(op, registers, name),
        EmirOp::GraphSparseTriplets(..)
        | EmirOp::IntNullspace(..)
        | EmirOp::ExactProductDelta(..)
        | EmirOp::GraphSparseFromTriplets(..)
        | EmirOp::LpMinimize(..)
        | EmirOp::ParetoFront(..)
        | EmirOp::PolyMul(..)
        | EmirOp::PolyEval(..)
        | EmirOp::SequenceGenerate { .. }
        | EmirOp::SequenceConvolve { .. }
        | EmirOp::OdeBackwardEuler(..)
        | EmirOp::OdeVelocityVerlet(..)
        | EmirOp::PoissonDirichletSine(..)
        | EmirOp::ControlTransferEval(..)
        | EmirOp::ControlDcGain(..)
        | EmirOp::ControlPolesStable(..)
        | EmirOp::CategoryCheck(..)
        | EmirOp::CategoryDiagramCommutative(..)
        | EmirOp::ProbSample { .. }
        | EmirOp::ProbDensity { .. }
        | EmirOp::TensorCreate { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::TensorAdd(..)
        | EmirOp::TensorSub(..)
        | EmirOp::TensorScale(..)
        | EmirOp::Einsum { .. }
        | EmirOp::Factorial(..)
        | EmirOp::ModInv(..)
        | EmirOp::SqrtMod(..)
        | EmirOp::PowMod(..)
        | EmirOp::IntRem(..)
        | EmirOp::Congruence(..)
        | EmirOp::PolyEvalMod(..)
        | EmirOp::RSEncode(..)
        | EmirOp::HammingDistance(..) => eval_domain_op(op, registers, inputs, name),
        EmirOp::Fold { .. }
        | EmirOp::Integral { .. }
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::SampleLimit { .. }
        | EmirOp::ReverseMode { .. } => eval_flow_op(op, registers, inputs, state, name),
    }
}

mod eval_arith;
mod eval_carriers;
mod eval_data;
mod eval_domains;
mod eval_flow;
mod eval_linalg;

use eval_arith::eval_arith_op;
use eval_carriers::eval_carrier_op;
use eval_data::eval_data_op;
use eval_domains::eval_domain_op;
use eval_flow::eval_flow_op;
use eval_linalg::eval_linalg_op;

fn optimize_grads(
    body: &EmirProgram,
    work_inputs: &mut [Value],
    state: &[Value],
    var_indices: &[u16],
    x: &[f64],
    name: &'static str,
) -> Result<Vec<f64>, EvalFault> {
    for (i, &vi) in var_indices.iter().enumerate() {
        work_inputs[vi as usize] = Value::F64(x[i]);
    }
    let mut grads = Vec::with_capacity(var_indices.len());
    for &vi in var_indices {
        let dual = evaluate_dual(body, work_inputs, state, vi, name)?;
        grads.push(dual.tangent);
    }
    Ok(grads)
}

/// Solve `matrix * x = rhs` by Gaussian elimination with partial pivoting.
fn dense_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, ()> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err(());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in (col + 1)..n {
            let candidate = a[row][col].abs();
            if candidate > best {
                best = candidate;
                pivot = row;
            }
        }
        if best < 1e-30 {
            return Err(());
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = b[row];
        for k in (row + 1)..n {
            acc -= a[row][k] * x[k];
        }
        x[row] = acc / a[row][row];
    }
    Ok(x)
}

/// Exact-integer operand extractor for the Rat cells: accepts the
/// integer value kinds (I64; Nat is I64 too). Widens to i128 so
/// products of two i64 denominators stay exact. Everything else is a
/// typed type-confusion, never a silent numeric coercion.
fn i128_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<i128, EvalFault> {
    match register(registers, value)? {
        Value::I64(value) => Ok(i128::from(*value)),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// Decompose a register holding an exact rational into (num, den).
fn rat_parts(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<(i128, i128), EvalFault> {
    match register(registers, value)? {
        Value::Rat { num, den } => Ok((*num, *den)),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// Canonical form: gcd-reduced with a positive denominator. The zero
/// denominator is a typed refusal here (eval-time backstop; the check
/// pass refuses literal zero denominators earlier with E-RAT-001).
fn rat_canonicalize(num: i128, den: i128, op: &'static str) -> Result<Value, EvalFault> {
    if den == 0 {
        return Err(EvalFault::Arithmetic {
            op,
            detail: "rat denominator must be nonzero",
        });
    }
    let mut num = num;
    let mut den = den;
    if den < 0 {
        num = -num;
        den = -den;
    }
    let gcd = gcd_u128(num.unsigned_abs(), den.unsigned_abs());
    Ok(Value::Rat {
        num: num / gcd as i128,
        den: den / gcd as i128,
    })
}

/// Euclidean gcd on u128 (rem only — no overflow).
fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// The four rational combines for the generic arithmetic operators.
enum RatCombine {
    Add,
    Sub,
    Mul,
    Div,
}

/// Exact rational arithmetic for the generic `+`/`-`/`*`/`/` operators
/// every intermediate is checked — overflow
/// is a typed refusal, never a silent wrap; a zero divisor is a typed
/// refusal, never an infinity. Results are gcd-canonical.
fn rat_binary(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    combine: RatCombine,
) -> Result<Value, EvalFault> {
    let (ln, ld) = rat_parts(registers, left, op)?;
    let (rn, rd) = rat_parts(registers, right, op)?;
    let (numerator, denominator) = match combine {
        RatCombine::Add => (
            ln.checked_mul(rd).and_then(|left_term| {
                rn.checked_mul(ld)
                    .and_then(|right_term| left_term.checked_add(right_term))
            }),
            ld.checked_mul(rd),
        ),
        RatCombine::Sub => (
            ln.checked_mul(rd).and_then(|left_term| {
                rn.checked_mul(ld)
                    .and_then(|right_term| left_term.checked_sub(right_term))
            }),
            ld.checked_mul(rd),
        ),
        RatCombine::Mul => (ln.checked_mul(rn), ld.checked_mul(rd)),
        RatCombine::Div => {
            if rn == 0 {
                return Err(EvalFault::Arithmetic {
                    op,
                    detail: "rational division by zero",
                });
            }
            (ln.checked_mul(rd), ld.checked_mul(rn))
        }
    };
    match (numerator, denominator) {
        (Some(num), Some(den)) => rat_canonicalize(num, den, op),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "rational arithmetic overflow (i128)",
        }),
    }
}

fn eval_complex_unary(
    id: BuiltinId,
    re: f64,
    im: f64,
    register: u32,
    op: &'static str,
) -> Result<Value, EvalFault> {
    match id {
        BuiltinId::Sqrt => {
            let (out_re, out_im) = emath_rt::complex_sqrt(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Ln => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Exp => {
            let (out_re, out_im) = emath_rt::complex_exp(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Log10 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_10;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Log2 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_2;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Abs => Ok(Value::F64(re.hypot(im))),
        BuiltinId::Recip => {
            let denom = re * re + im * im;
            Ok(Value::Complex {
                re: re / denom,
                im: -im / denom,
            })
        }
        _ => Err(EvalFault::TypeConfusion { register, op }),
    }
}

// --- Helper functions extracted to interp/helpers.rs ---
// --- Dual-number autodiff subsystem extracted to interp/dual.rs ---

// Extended GCD moved to crates/emath-rt/src/body.rs (mod_inv_checked).
