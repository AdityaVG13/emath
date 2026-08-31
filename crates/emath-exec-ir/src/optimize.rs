//! EMIR peephole optimization: constant folding + dead-register elimination.
//!
//! Runs after lowering, before interpretation or codegen, so both consumers
//! see the same shrunk program. Preserves observable behavior exactly —
//! including strict eager fault timing: only provably-total ops are removed
//! or folded; ops that can fault at runtime are never touched, and folding
//! mirrors the interpreter's conversions (`f64_of`/`i64_of`/`bool_of`,
//! `eq_ne`), exact I64×I64 arithmetic, and IEEE behavior bit-exactly.

use emath_core::Span;

use crate::{EmirOp, EmirProgram, EmirValue};

/// Folded compile-time constant, mirroring the interpreter's value kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstVal {
    F64(f64),
    I64(i64),
    Bool(bool),
}

impl ConstVal {
    /// `f64_of` semantics: F64 as-is, I64 widened, Bool has no f64 form.
    fn f64_of(self) -> Option<f64> {
        match self {
            ConstVal::F64(x) => Some(x),
            ConstVal::I64(x) => Some(x as f64),
            ConstVal::Bool(_) => None,
        }
    }

    /// `bool_of` semantics: Bool as-is, F64 truthiness, I64 has no bool form.
    fn bool_of(self) -> Option<bool> {
        match self {
            ConstVal::Bool(b) => Some(b),
            ConstVal::F64(x) => Some(x != 0.0),
            ConstVal::I64(_) => None,
        }
    }

    /// `eq_ne` scalar semantics: IEEE for F64×F64, exact I64×I64, exact
    /// mixed I64×F64 (not a 2^53 widening round), Bool coerces with F64
    /// by truthiness.
    fn eq(self, other: ConstVal) -> Option<bool> {
        Some(match (self, other) {
            (ConstVal::F64(a), ConstVal::F64(b)) => a == b,
            (ConstVal::I64(a), ConstVal::I64(b)) => a == b,
            (ConstVal::I64(a), ConstVal::F64(b)) => emath_rt::eq_i64_f64(a, b),
            (ConstVal::F64(a), ConstVal::I64(b)) => emath_rt::eq_i64_f64(b, a),
            (ConstVal::Bool(a), ConstVal::Bool(b)) => a == b,
            (ConstVal::Bool(a), ConstVal::F64(b)) => a == (b != 0.0),
            (ConstVal::F64(a), ConstVal::Bool(b)) => (a != 0.0) == b,
            _ => return None,
        })
    }
}

/// Optimize a program in place: constant-fold scalar arithmetic and
/// eliminate dead registers, recursing into nested sub-programs (fold
/// bodies, integrands, solver bodies, ...).
pub fn optimize_program(program: &mut EmirProgram) {
    // Nested bodies number their own registers; optimize them first so the
    // outer pass treats each as an opaque unit.
    for (op, _) in &mut program.ops {
        match op {
            EmirOp::Fold { body, .. }
            | EmirOp::Differentiate { body, .. }
            | EmirOp::Solve { body, .. }
            | EmirOp::Optimize { body, .. }
            | EmirOp::SampleLimit { body, .. }
            | EmirOp::ReverseMode { body, .. } => optimize_program(body),
            EmirOp::Integral { integrand, .. } => optimize_program(integrand),
            _ => {}
        }
    }
    constant_fold(program);
    dead_code_eliminate(program);
}

// ── Constant folding ─────────────────────────────────────────────────────

fn const_at(consts: &[Option<ConstVal>], v: EmirValue) -> Option<ConstVal> {
    consts.get(v.0 as usize).copied().flatten()
}

/// Fold helpers over the const table, mirroring the interpreter's
/// conversions exactly. A `None` result means the operand kinds would make
/// evaluation a typed fault (the op is left unfolded so the fault is
/// preserved) or an operand is not constant.
fn fold_f64_bin(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    b: EmirValue,
    f: impl FnOnce(f64, f64) -> f64,
) -> Option<ConstVal> {
    Some(ConstVal::F64(f(
        const_at(consts, a)?.f64_of()?,
        const_at(consts, b)?.f64_of()?,
    )))
}

/// I64×I64 stays exact (overflow leaves the op unfolded so interp faults);
/// mixed kinds widen to f64, matching the interpreter.
fn fold_arith(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    b: EmirValue,
    i64_op: impl FnOnce(i64, i64) -> Option<i64>,
    f64_op: impl FnOnce(f64, f64) -> f64,
) -> Option<ConstVal> {
    match (const_at(consts, a), const_at(consts, b)) {
        (Some(ConstVal::I64(x)), Some(ConstVal::I64(y))) => i64_op(x, y).map(ConstVal::I64),
        _ => fold_f64_bin(consts, a, b, f64_op),
    }
}

fn fold_ord(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    b: EmirValue,
    pred: impl Fn(core::cmp::Ordering) -> bool,
    on_f64: impl FnOnce(f64, f64) -> bool,
) -> Option<ConstVal> {
    match (const_at(consts, a)?, const_at(consts, b)?) {
        (ConstVal::I64(x), ConstVal::I64(y)) => Some(ConstVal::Bool(pred(x.cmp(&y)))),
        (ConstVal::I64(x), ConstVal::F64(y)) => Some(ConstVal::Bool(
            emath_rt::cmp_i64_f64(x, y).is_some_and(&pred),
        )),
        (ConstVal::F64(x), ConstVal::I64(y)) => Some(ConstVal::Bool(
            emath_rt::cmp_i64_f64(y, x)
                .map(core::cmp::Ordering::reverse)
                .is_some_and(&pred),
        )),
        _ => fold_cmp(consts, a, b, on_f64),
    }
}

fn fold_f64_un(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    f: impl FnOnce(f64) -> f64,
) -> Option<ConstVal> {
    Some(ConstVal::F64(f(const_at(consts, a)?.f64_of()?)))
}

fn fold_bool_bin(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    b: EmirValue,
    f: impl FnOnce(bool, bool) -> bool,
) -> Option<ConstVal> {
    Some(ConstVal::Bool(f(
        const_at(consts, a)?.bool_of()?,
        const_at(consts, b)?.bool_of()?,
    )))
}

fn fold_cmp(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    b: EmirValue,
    f: impl FnOnce(f64, f64) -> bool,
) -> Option<ConstVal> {
    Some(ConstVal::Bool(f(
        const_at(consts, a)?.f64_of()?,
        const_at(consts, b)?.f64_of()?,
    )))
}

/// Fold one op's result, mirroring the interpreter's conversions; differs
/// from `consts[i]` when the op is non-foldable or would fault (the fault
/// is preserved by leaving the op unfolded).
fn fold_op(op: &EmirOp, consts: &[Option<ConstVal>]) -> Option<ConstVal> {
    match *op {
        EmirOp::ConstF64(bits) => Some(ConstVal::F64(f64::from_bits(bits))),
        EmirOp::ConstI64(v) => Some(ConstVal::I64(v)),
        EmirOp::ConstBool(b) => Some(ConstVal::Bool(b)),
        EmirOp::F64Add(a, b) => fold_arith(consts, a, b, i64::checked_add, |x, y| x + y),
        EmirOp::F64Sub(a, b) => fold_arith(consts, a, b, i64::checked_sub, |x, y| x - y),
        EmirOp::F64Mul(a, b) => fold_arith(consts, a, b, i64::checked_mul, |x, y| x * y),
        EmirOp::F64Div(a, b) => fold_f64_bin(consts, a, b, |x, y| x / y),
        EmirOp::F64Pow(a, b) => fold_f64_bin(consts, a, b, |x, y| x.powf(y)),
        EmirOp::Neg(a) => match const_at(consts, a)? {
            ConstVal::I64(x) => x.checked_neg().map(ConstVal::I64),
            ConstVal::F64(x) => Some(ConstVal::F64(-x)),
            ConstVal::Bool(_) => None,
        },
        EmirOp::UnaryBuiltin(id, a) => fold_f64_un(consts, a, |x| id.eval_unary(x)),
        EmirOp::BinaryBuiltin(id, a, b) => fold_f64_bin(consts, a, b, |x, y| id.eval_binary(x, y)),
        EmirOp::Lt(a, b) => fold_ord(consts, a, b, |o| o.is_lt(), |x, y| x < y),
        EmirOp::Le(a, b) => fold_ord(consts, a, b, |o| o.is_le(), |x, y| x <= y),
        EmirOp::Gt(a, b) => fold_ord(consts, a, b, |o| o.is_gt(), |x, y| x > y),
        EmirOp::Ge(a, b) => fold_ord(consts, a, b, |o| o.is_ge(), |x, y| x >= y),
        EmirOp::Eq(a, b) => Some(ConstVal::Bool(
            const_at(consts, a)?.eq(const_at(consts, b)?)?,
        )),
        EmirOp::Ne(a, b) => Some(ConstVal::Bool(
            !const_at(consts, a)?.eq(const_at(consts, b)?)?,
        )),
        EmirOp::And(a, b) => fold_bool_bin(consts, a, b, |x, y| x && y),
        EmirOp::Or(a, b) => fold_bool_bin(consts, a, b, |x, y| x || y),
        EmirOp::Imply(a, b) => fold_bool_bin(consts, a, b, |x, y| !x || y),
        EmirOp::Iff(a, b) => fold_bool_bin(consts, a, b, |x, y| x == y),
        EmirOp::Not(a) => Some(ConstVal::Bool(!const_at(consts, a)?.bool_of()?)),
        EmirOp::IsFinite(a) => Some(ConstVal::Bool(const_at(consts, a)?.f64_of()?.is_finite())),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            let cond = const_at(consts, condition)?.bool_of()?;
            if cond {
                const_at(consts, then_value)
            } else {
                const_at(consts, else_value)
            }
        }
        _ => None,
    }
}

/// Replace provably-constant ops with their constant form (`ConstF64` /
/// `ConstI64` / `ConstBool`), keeping every register slot so downstream
/// references stay valid. Division by zero folds to inf/NaN exactly as
/// evaluating the op would.
fn constant_fold(program: &mut EmirProgram) {
    let mut consts: Vec<Option<ConstVal>> = vec![None; program.ops.len()];
    for (i, (op, _)) in program.ops.iter_mut().enumerate() {
        let folded = fold_op(op, &consts);
        consts[i] = folded;
        let replacement = match folded {
            Some(ConstVal::F64(x)) => Some(EmirOp::ConstF64(x.to_bits())),
            Some(ConstVal::I64(x)) => Some(EmirOp::ConstI64(x)),
            Some(ConstVal::Bool(b)) => Some(EmirOp::ConstBool(b)),
            None => None,
        };
        if let Some(replacement) = replacement {
            *op = replacement;
        }
    }
}

// ── Dead-register elimination ───────────────────────────────────────────

/// Whether evaluating this op can fault at runtime for a well-formed
/// program. Total ops are eligible for removal and for single-use
/// inlining; everything else is kept so strict eager fault semantics are
/// preserved.
pub fn is_total(op: &EmirOp, program: &EmirProgram) -> bool {
    match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => true,
        EmirOp::LoadInput(i) => usize::from(*i) < usize::from(program.input_count),
        EmirOp::LoadState(i) => usize::from(*i) < usize::from(program.state_count),
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::Neg(_)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..) => true,
        EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(_)
        | EmirOp::IsFinite(_) => true,
        EmirOp::Select { .. }
        | EmirOp::FormatText { .. }
        | EmirOp::TextLength(_)
        | EmirOp::TextNfc(_)
        | EmirOp::ReportSection { .. }
        | EmirOp::ReportDocument { .. }
        | EmirOp::ReportMarkdown(_)
        | EmirOp::ReportLatex(_)
        | EmirOp::SetCreate { .. }
        | EmirOp::RecordCreate { .. } => true,
        EmirOp::VectorCreate(..) | EmirOp::MatrixCreate { .. } | EmirOp::TensorCreate { .. } => {
            true
        }
        // Dynamic index bounds can fault even in well-formed programs.
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::SetContains { .. }
        | EmirOp::SeriesSample { .. } => false,
        // Static-shape aggregate ops: shape/type faults are excluded by the
        // typed front end.
        EmirOp::VectorAdd(..)
        | EmirOp::VectorSub(..)
        | EmirOp::VectorScale(..)
        | EmirOp::VectorDot(..)
        | EmirOp::VectorNorm(_)
        | EmirOp::VectorLength(_)
        | EmirOp::Stencil1d { .. }
        | EmirOp::Stencil2d { .. }
        | EmirOp::Stencil3d { .. }
        | EmirOp::MatrixAdd(..)
        | EmirOp::MatrixSub(..)
        | EmirOp::MatrixScale(..)
        | EmirOp::MatrixMulVector(..)
        | EmirOp::MatrixMulMatrix(..)
        | EmirOp::MatrixTranspose(_)
        // Spectral/iterative kernels (xx0x.2) refuse typed on bad input
        // (E-LINALG-001..003) — dynamic faults, like Factorial.
        | EmirOp::EigenSymmetric(_)
        | EmirOp::EigenVectorsSymmetric(_)
        | EmirOp::SvdSingularValues(_)
        | EmirOp::SvdFactors(_)
        | EmirOp::CgSolve(..)
        | EmirOp::LinearSolve(..)
        | EmirOp::LuFactors(_)
        | EmirOp::QrFactors(_)
        | EmirOp::OuterProduct(..)
        // Graph kernels (r2-graphs-masa) refuse typed on bad input
        // (E-GRAPH-001..003) — dynamic faults, like the spectral set.
        | EmirOp::GraphReachable(..)
        | EmirOp::GraphBfsOrder(..)
        | EmirOp::GraphDijkstra(..)
        | EmirOp::GraphDegreeOut(_)
        | EmirOp::GraphLaplacian(_)
        | EmirOp::GraphSymmetrize(_)
        | EmirOp::GraphBellmanFord(..)
        | EmirOp::GraphSparseTriplets(_)
        | EmirOp::GraphSparseFromTriplets(..)
        // Exact integer nullspace (rymw): dynamic faults (E-NULLSPACE-001/002)
        // — never folded, matched only by children-preserving rewriting.
        | EmirOp::IntNullspace(_)
        // Exact integer product delta (rymw thermo): dynamic faults
        // (E-EXACT-001/002) — never folded.
        | EmirOp::ExactProductDelta(..)
        // Optimization kernels (r3-lp-milp-wlif) refuse typed on bad
        // input (E-LP-001..004, E-PARETO-001) — dynamic faults, like
        // the spectral/graph sets.
        | EmirOp::LpMinimize(..)
        | EmirOp::ParetoFront(_)
        // Polynomial kernels (r3-funcspaces-poly-hjor) refuse typed on
        // non-finite input (E-POLY-001/002) — dynamic faults.
        | EmirOp::PolyMul(..)
        | EmirOp::PolyEval(..)
        | EmirOp::SequenceGenerate { .. }
        | EmirOp::SequenceConvolve { .. }
        // ODE stepping (xx0x.3) refuses typed: Newton non-convergence
        // (E-ODE-001), non-advancing step (E-ODE-003), non-finite
        // carriers (E-ODE-004).
        | EmirOp::OdeBackwardEuler(..)
        | EmirOp::OdeVelocityVerlet(..)
        // Spectral Poisson (xx0x.4) refuses typed: empty interior
        // (E-PDE-001), non-finite load (E-PDE-002).
        | EmirOp::PoissonDirichletSine(_)
        // Probability ops (xx0x.5) refuse typed: invalid parameters
        // (E-PROB-001), non-finite carriers (E-PROB-002), wrong arity
        // (E-PROB-003).
        | EmirOp::ProbSample { .. }
        | EmirOp::ProbDensity { .. }
        // Control kernels (zxkl thin B43) refuse typed on bad input
        // (E-CONTROL-001..005) — dynamic faults, like the probability
        // set.
        | EmirOp::ControlTransferEval(..)
        | EmirOp::ControlDcGain(..)
        | EmirOp::ControlPolesStable(_)
        // Category kernels (88wo thin B39) refuse typed on bad input
        // (E-CAT-001..007) — dynamic faults, like the control set.
        | EmirOp::CategoryCheck(..)
        | EmirOp::CategoryDiagramCommutative(..)
        // Option/Result semantics (aj8d) are TOTAL value ops — they
        // fault only via TypeConfusion on a wrong carrier shape, the
        // same dynamic-fault class as the typed-kernel sets above.
        | EmirOp::OptionSome(_)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(_)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(_)
        | EmirOp::ResultErr(_)
        | EmirOp::ResultIsOk(_)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(_)
        | EmirOp::TensorAdd(..)
        | EmirOp::TensorSub(..)
        | EmirOp::TensorScale(..)
        | EmirOp::Einsum { .. } => true,
        // Dynamic domain faults (factorial of a negative, non-invertible
        // modulus, congruence mod 0, ...) and runtime panics (solver
        // non-convergence).
        EmirOp::Factorial(..)
        | EmirOp::ModInv(..)
        | EmirOp::IntRem(..)
        | EmirOp::Congruence(..)
        | EmirOp::PolyEvalMod(..)
        | EmirOp::RSEncode(..)
        | EmirOp::HammingDistance(..) => false,
        // Higher-order drivers evaluate user bodies and can fault or
        // panic inside them.
        EmirOp::Fold { .. }
        | EmirOp::Integral { .. }
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::SampleLimit { .. }
        | EmirOp::ReverseMode { .. } => false,
        // Capability dispatch can refuse (E-CELL-006), demand a provider
        // run, or fault on arity/shape — never total.
        EmirOp::ApplyCapability { .. } => false,
        // Generic vector map/broadcast/finite-guard ops are total on any
        // (possibly empty) vector; reduce faults on an empty vector, so
        // it is kept eager like the index ops.
        EmirOp::VectorMap { .. } | EmirOp::VectorMapScalar { .. } => true,
        EmirOp::VectorReduce { .. } => false,
        EmirOp::VectorAllFinite(_) => true,
        // Certified intervals fault on ill-formed bounds (8pjn) and
        // intersection faults on an empty result — never total.
        EmirOp::IntervalCreate(..)
        | EmirOp::IntervalIntersect(..)
        | EmirOp::SpecialFunction { .. } => false,
        // Exact-rational cells (emath-rat-real-types-p5cj) fault
        // dynamically: zero denominator (E-RAT-001 class) and i128
        // overflow — never total, never folded here.
        EmirOp::RatConstruct { .. } | EmirOp::RatAdd(..) | EmirOp::RatNorm(_) => false,
    }
}

/// Collect the register operands of an op (nested sub-programs excluded —
/// they number their own registers). Shared with the Rust backend for
/// single-use inlining decisions.
pub fn operand_registers(op: &EmirOp, out: &mut Vec<EmirValue>) {
    let mut push = |v: EmirValue| out.push(v);
    match *op {
        EmirOp::RatConstruct { num, den } => {
            push(num);
            push(den);
        }
        EmirOp::RatAdd(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::RatNorm(a) => push(a),
        EmirOp::F64Add(a, b)
        | EmirOp::F64Sub(a, b)
        | EmirOp::F64Mul(a, b)
        | EmirOp::F64Div(a, b)
        | EmirOp::F64Pow(a, b)
        | EmirOp::Lt(a, b)
        | EmirOp::Le(a, b)
        | EmirOp::Gt(a, b)
        | EmirOp::Ge(a, b)
        | EmirOp::Eq(a, b)
        | EmirOp::Ne(a, b)
        | EmirOp::And(a, b)
        | EmirOp::Or(a, b)
        | EmirOp::Imply(a, b)
        | EmirOp::Iff(a, b)
        | EmirOp::VectorAdd(a, b)
        | EmirOp::VectorSub(a, b)
        | EmirOp::VectorScale(a, b)
        | EmirOp::VectorDot(a, b)
        | EmirOp::MatrixAdd(a, b)
        | EmirOp::MatrixSub(a, b)
        | EmirOp::MatrixScale(a, b)
        | EmirOp::MatrixMulVector(a, b)
        | EmirOp::MatrixMulMatrix(a, b)
        | EmirOp::CgSolve(a, b)
        | EmirOp::LinearSolve(a, b)
        | EmirOp::OuterProduct(a, b)
        | EmirOp::GraphReachable(a, b)
        | EmirOp::GraphBfsOrder(a, b)
        | EmirOp::GraphDijkstra(a, b)
        | EmirOp::GraphBellmanFord(a, b)
        | EmirOp::GraphSparseFromTriplets(a, b)
        | EmirOp::PolyMul(a, b)
        | EmirOp::PolyEval(a, b)
        | EmirOp::TensorAdd(a, b)
        | EmirOp::TensorSub(a, b)
        | EmirOp::TensorScale(a, b)
        | EmirOp::ModInv(a, b)
        | EmirOp::IntRem(a, b)
        | EmirOp::HammingDistance(a, b)
        | EmirOp::BinaryBuiltin(_, a, b) => {
            push(a);
            push(b);
        }
        EmirOp::IntNullspace(a) => {
            push(a);
        }
        EmirOp::ExactProductDelta(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::SeriesSample { series, time } => {
            push(series);
            push(time);
        }
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => {
            push(initial);
            push(recurrence);
            push(budget);
        }
        EmirOp::SequenceConvolve { left, right, count } => {
            push(left);
            push(right);
            push(count);
        }
        // Three-operand ops (r3-lp-milp-wlif LP; xx0x.3 backward Euler).
        EmirOp::LpMinimize(a, b, c) | EmirOp::OdeBackwardEuler(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Three-operand ops (zxkl control: transfer num/den/x,
        // state-space A/b/c).
        EmirOp::ControlTransferEval(a, b, c) | EmirOp::ControlDcGain(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Three-operand op (88wo category law gate: dom, cod, comp).
        EmirOp::CategoryCheck(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Four-operand op (88wo category commutativity: dom, cod,
        // comp, faces).
        EmirOp::CategoryDiagramCommutative(a, b, c, d) => {
            push(a);
            push(b);
            push(c);
            push(d);
        }
        // Four-operand op (xx0x.3 velocity Verlet: a, q, v, h).
        EmirOp::OdeVelocityVerlet(a, b, c, d) => {
            push(a);
            push(b);
            push(c);
            push(d);
        }
        // Three-operand op (xx0x.5 seeded sampling: params, seed, draws).
        EmirOp::ProbSample {
            params: a,
            seed: b,
            draws: c,
            stream,
            ..
        } => {
            push(a);
            push(b);
            push(c);
            if let Some(stream) = stream {
                push(stream);
            }
        }
        // Two-operand op (xx0x.5 density: params, x).
        EmirOp::ProbDensity {
            params: a, x: b, ..
        } => {
            push(a);
            push(b);
        }
        // Option/Result ops (aj8d): constructors/polarity carry one
        // register, unwraps carry two, None carries none.
        EmirOp::OptionSome(a)
        | EmirOp::OptionIsSome(a)
        | EmirOp::ResultOk(a)
        | EmirOp::ResultErr(a)
        | EmirOp::ResultIsOk(a)
        | EmirOp::ResultErrorOf(a) => push(a),
        EmirOp::OptionUnwrapOr(a, b) | EmirOp::ResultUnwrapOr(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::OptionNone => {}
        // Single-operand op (xx0x.4 spectral Poisson: the load).
        EmirOp::PoissonDirichletSine(a) => {
            push(a);
        }
        EmirOp::Neg(a)
        | EmirOp::UnaryBuiltin(_, a)
        | EmirOp::TextLength(a)
        | EmirOp::TextNfc(a)
        | EmirOp::ReportMarkdown(a)
        | EmirOp::ReportLatex(a)
        | EmirOp::Not(a)
        | EmirOp::IsFinite(a)
        | EmirOp::VectorNorm(a)
        | EmirOp::VectorLength(a)
        | EmirOp::MatrixTranspose(a)
        | EmirOp::EigenSymmetric(a)
        | EmirOp::EigenVectorsSymmetric(a)
        | EmirOp::SvdSingularValues(a)
        | EmirOp::SvdFactors(a)
        | EmirOp::LuFactors(a)
        | EmirOp::QrFactors(a)
        | EmirOp::GraphDegreeOut(a)
        | EmirOp::GraphLaplacian(a)
        | EmirOp::GraphSymmetrize(a)
        | EmirOp::GraphSparseTriplets(a)
        | EmirOp::ParetoFront(a)
        | EmirOp::ControlPolesStable(a)
        | EmirOp::Factorial(a) => push(a),
        EmirOp::ReportSection { heading, body } => {
            push(heading);
            push(body);
        }
        EmirOp::ReportDocument { title, section } => {
            push(title);
            push(section);
        }
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            push(condition);
            push(then_value);
            push(else_value);
        }
        EmirOp::VectorCreate(ref elements)
        | EmirOp::MatrixCreate { ref elements, .. }
        | EmirOp::TensorCreate { ref elements, .. } => {
            for &e in elements {
                push(e);
            }
        }
        EmirOp::ApplyCapability { ref args, .. } => {
            for &value in args {
                push(value);
            }
        }
        EmirOp::FormatText { ref arguments, .. } => {
            for &value in arguments {
                push(value);
            }
        }
        EmirOp::SpecialFunction { ref arguments, .. } => {
            for &value in arguments {
                push(value);
            }
        }
        EmirOp::SetCreate {
            ref elements,
            ref guards,
        } => {
            for &value in elements {
                push(value);
            }
            for &value in guards.iter().flatten() {
                push(value);
            }
        }
        EmirOp::SetContains { element, set } => {
            push(element);
            push(set);
        }
        EmirOp::RecordCreate { ref fields, .. } => {
            for (_, value) in fields {
                push(*value);
            }
        }
        EmirOp::VectorMap { source, .. } => push(source),
        EmirOp::VectorMapScalar { vector, scalar, .. } => {
            push(vector);
            push(scalar);
        }
        EmirOp::VectorReduce { source, .. } => push(source),
        EmirOp::VectorAllFinite(source) => push(source),
        EmirOp::VectorIndex { vector, index } => {
            push(vector);
            push(index);
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            push(matrix);
            push(row);
            push(col);
        }
        EmirOp::Stencil1d { input, .. }
        | EmirOp::Stencil2d { input, .. }
        | EmirOp::Stencil3d { input, .. } => push(input),
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            push(tensor);
            for &i in indices {
                push(i);
            }
        }
        EmirOp::TensorSlice { tensor, ref axes } => {
            push(tensor);
            for axis in axes {
                match *axis {
                    crate::EmirSliceAxis::Point(v) => push(v),
                    crate::EmirSliceAxis::Range { start, end } => {
                        push(start);
                        push(end);
                    }
                }
            }
        }
        EmirOp::Einsum { ref inputs, .. } => {
            for &i in inputs {
                push(i);
            }
        }
        EmirOp::Congruence(a, b, m) => {
            push(a);
            push(b);
            push(m);
        }
        EmirOp::IntervalCreate(lo, hi) => {
            push(lo);
            push(hi);
        }
        EmirOp::IntervalIntersect(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::PolyEvalMod(c, x, p) | EmirOp::RSEncode(c, x, p) => {
            push(c);
            push(x);
            push(p);
        }
        EmirOp::Fold {
            start, end, init, ..
        } => {
            push(start);
            push(end);
            push(init);
        }
        EmirOp::Integral { start, end, .. } => {
            push(start);
            push(end);
        }
        EmirOp::SampleLimit {
            target, direction, ..
        } => {
            push(target);
            push(direction);
        }
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..)
        | EmirOp::LoadInput(_)
        | EmirOp::LoadState(_)
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::ReverseMode { .. } => {}
    }
}

/// Rebuild `op` with each register operand mapped through `f`. Nested
/// sub-programs (fold bodies, integrands, solver bodies) are returned
/// unchanged.
fn remap_operands(op: &EmirOp, f: &mut impl FnMut(EmirValue) -> EmirValue) -> EmirOp {
    let mut g = |v: EmirValue| f(v);
    match *op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => op.clone(),
        EmirOp::LoadInput(_) | EmirOp::LoadState(_) => op.clone(),
        EmirOp::SeriesSample { series, time } => EmirOp::SeriesSample {
            series: g(series),
            time: g(time),
        },
        EmirOp::RatConstruct { num, den } => EmirOp::RatConstruct {
            num: g(num),
            den: g(den),
        },
        EmirOp::RatAdd(a, b) => EmirOp::RatAdd(g(a), g(b)),
        EmirOp::RatNorm(a) => EmirOp::RatNorm(g(a)),
        EmirOp::TextLength(text) => EmirOp::TextLength(g(text)),
        EmirOp::TextNfc(text) => EmirOp::TextNfc(g(text)),
        EmirOp::ReportSection { heading, body } => EmirOp::ReportSection {
            heading: g(heading),
            body: g(body),
        },
        EmirOp::ReportDocument { title, section } => EmirOp::ReportDocument {
            title: g(title),
            section: g(section),
        },
        EmirOp::ReportMarkdown(document) => EmirOp::ReportMarkdown(g(document)),
        EmirOp::ReportLatex(document) => EmirOp::ReportLatex(g(document)),
        EmirOp::F64Add(a, b) => EmirOp::F64Add(g(a), g(b)),
        EmirOp::F64Sub(a, b) => EmirOp::F64Sub(g(a), g(b)),
        EmirOp::F64Mul(a, b) => EmirOp::F64Mul(g(a), g(b)),
        EmirOp::F64Div(a, b) => EmirOp::F64Div(g(a), g(b)),
        EmirOp::F64Pow(a, b) => EmirOp::F64Pow(g(a), g(b)),
        EmirOp::Neg(a) => EmirOp::Neg(g(a)),
        EmirOp::UnaryBuiltin(id, a) => EmirOp::UnaryBuiltin(id, g(a)),
        EmirOp::BinaryBuiltin(id, a, b) => EmirOp::BinaryBuiltin(id, g(a), g(b)),
        EmirOp::Lt(a, b) => EmirOp::Lt(g(a), g(b)),
        EmirOp::Le(a, b) => EmirOp::Le(g(a), g(b)),
        EmirOp::Gt(a, b) => EmirOp::Gt(g(a), g(b)),
        EmirOp::Ge(a, b) => EmirOp::Ge(g(a), g(b)),
        EmirOp::Eq(a, b) => EmirOp::Eq(g(a), g(b)),
        EmirOp::Ne(a, b) => EmirOp::Ne(g(a), g(b)),
        EmirOp::And(a, b) => EmirOp::And(g(a), g(b)),
        EmirOp::Or(a, b) => EmirOp::Or(g(a), g(b)),
        EmirOp::Imply(a, b) => EmirOp::Imply(g(a), g(b)),
        EmirOp::Iff(a, b) => EmirOp::Iff(g(a), g(b)),
        EmirOp::Not(a) => EmirOp::Not(g(a)),
        EmirOp::IsFinite(a) => EmirOp::IsFinite(g(a)),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => EmirOp::Select {
            condition: g(condition),
            then_value: g(then_value),
            else_value: g(else_value),
        },
        EmirOp::VectorCreate(ref elements) => {
            EmirOp::VectorCreate(elements.iter().copied().map(g).collect())
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            ref elements,
        } => EmirOp::MatrixCreate {
            rows,
            cols,
            elements: elements.iter().map(|value| g(*value)).collect(),
        },
        EmirOp::VectorIndex { vector, index } => EmirOp::VectorIndex {
            vector: g(vector),
            index: g(index),
        },
        EmirOp::MatrixIndex { matrix, row, col } => EmirOp::MatrixIndex {
            matrix: g(matrix),
            row: g(row),
            col: g(col),
        },
        EmirOp::VectorAdd(a, b) => EmirOp::VectorAdd(g(a), g(b)),
        EmirOp::VectorSub(a, b) => EmirOp::VectorSub(g(a), g(b)),
        EmirOp::VectorScale(a, b) => EmirOp::VectorScale(g(a), g(b)),
        EmirOp::VectorDot(a, b) => EmirOp::VectorDot(g(a), g(b)),
        EmirOp::VectorNorm(a) => EmirOp::VectorNorm(g(a)),
        EmirOp::VectorLength(a) => EmirOp::VectorLength(g(a)),
        EmirOp::Stencil1d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil1d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil2d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::Stencil3d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil3d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::MatrixAdd(a, b) => EmirOp::MatrixAdd(g(a), g(b)),
        EmirOp::MatrixSub(a, b) => EmirOp::MatrixSub(g(a), g(b)),
        EmirOp::MatrixScale(a, b) => EmirOp::MatrixScale(g(a), g(b)),
        EmirOp::MatrixMulVector(a, b) => EmirOp::MatrixMulVector(g(a), g(b)),
        EmirOp::MatrixMulMatrix(a, b) => EmirOp::MatrixMulMatrix(g(a), g(b)),
        EmirOp::MatrixTranspose(a) => EmirOp::MatrixTranspose(g(a)),
        EmirOp::EigenSymmetric(a) => EmirOp::EigenSymmetric(g(a)),
        EmirOp::EigenVectorsSymmetric(a) => EmirOp::EigenVectorsSymmetric(g(a)),
        EmirOp::SvdSingularValues(a) => EmirOp::SvdSingularValues(g(a)),
        EmirOp::SvdFactors(a) => EmirOp::SvdFactors(g(a)),
        EmirOp::CgSolve(a, b) => EmirOp::CgSolve(g(a), g(b)),
        EmirOp::LinearSolve(a, b) => EmirOp::LinearSolve(g(a), g(b)),
        EmirOp::LuFactors(a) => EmirOp::LuFactors(g(a)),
        EmirOp::QrFactors(a) => EmirOp::QrFactors(g(a)),
        EmirOp::OuterProduct(a, b) => EmirOp::OuterProduct(g(a), g(b)),
        EmirOp::GraphReachable(a, b) => EmirOp::GraphReachable(g(a), g(b)),
        EmirOp::GraphBfsOrder(a, b) => EmirOp::GraphBfsOrder(g(a), g(b)),
        EmirOp::GraphDijkstra(a, b) => EmirOp::GraphDijkstra(g(a), g(b)),
        EmirOp::GraphBellmanFord(a, b) => EmirOp::GraphBellmanFord(g(a), g(b)),
        EmirOp::GraphDegreeOut(a) => EmirOp::GraphDegreeOut(g(a)),
        EmirOp::GraphLaplacian(a) => EmirOp::GraphLaplacian(g(a)),
        EmirOp::GraphSymmetrize(a) => EmirOp::GraphSymmetrize(g(a)),
        EmirOp::GraphSparseTriplets(a) => EmirOp::GraphSparseTriplets(g(a)),
        EmirOp::GraphSparseFromTriplets(a, b) => EmirOp::GraphSparseFromTriplets(g(a), g(b)),
        EmirOp::IntNullspace(a) => EmirOp::IntNullspace(g(a)),
        EmirOp::ExactProductDelta(a, b) => EmirOp::ExactProductDelta(g(a), g(b)),
        EmirOp::LpMinimize(a, b, c) => EmirOp::LpMinimize(g(a), g(b), g(c)),
        EmirOp::ParetoFront(a) => EmirOp::ParetoFront(g(a)),
        EmirOp::ControlTransferEval(a, b, c) => EmirOp::ControlTransferEval(g(a), g(b), g(c)),
        EmirOp::ControlDcGain(a, b, c) => EmirOp::ControlDcGain(g(a), g(b), g(c)),
        EmirOp::ControlPolesStable(a) => EmirOp::ControlPolesStable(g(a)),
        EmirOp::CategoryCheck(a, b, c) => EmirOp::CategoryCheck(g(a), g(b), g(c)),
        EmirOp::CategoryDiagramCommutative(a, b, c, d) => {
            EmirOp::CategoryDiagramCommutative(g(a), g(b), g(c), g(d))
        }
        EmirOp::PolyMul(a, b) => EmirOp::PolyMul(g(a), g(b)),
        EmirOp::PolyEval(a, b) => EmirOp::PolyEval(g(a), g(b)),
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => EmirOp::SequenceGenerate {
            initial: g(initial),
            recurrence: g(recurrence),
            budget: g(budget),
        },
        EmirOp::SequenceConvolve { left, right, count } => EmirOp::SequenceConvolve {
            left: g(left),
            right: g(right),
            count: g(count),
        },
        EmirOp::OdeBackwardEuler(a, b, c) => EmirOp::OdeBackwardEuler(g(a), g(b), g(c)),
        EmirOp::OdeVelocityVerlet(a, b, c, d) => EmirOp::OdeVelocityVerlet(g(a), g(b), g(c), g(d)),
        EmirOp::PoissonDirichletSine(a) => EmirOp::PoissonDirichletSine(g(a)),
        EmirOp::ProbSample {
            kind,
            params,
            seed,
            draws,
            stream,
        } => EmirOp::ProbSample {
            kind,
            params: g(params),
            seed: g(seed),
            draws: g(draws),
            stream: stream.map(|value| g(value)),
        },
        EmirOp::ProbDensity { kind, params, x } => EmirOp::ProbDensity {
            kind,
            params: g(params),
            x: g(x),
        },
        EmirOp::OptionSome(a) => EmirOp::OptionSome(g(a)),
        EmirOp::OptionNone => EmirOp::OptionNone,
        EmirOp::OptionIsSome(a) => EmirOp::OptionIsSome(g(a)),
        EmirOp::OptionUnwrapOr(a, b) => EmirOp::OptionUnwrapOr(g(a), g(b)),
        EmirOp::ResultOk(a) => EmirOp::ResultOk(g(a)),
        EmirOp::ResultErr(a) => EmirOp::ResultErr(g(a)),
        EmirOp::ResultIsOk(a) => EmirOp::ResultIsOk(g(a)),
        EmirOp::ResultUnwrapOr(a, b) => EmirOp::ResultUnwrapOr(g(a), g(b)),
        EmirOp::ResultErrorOf(a) => EmirOp::ResultErrorOf(g(a)),
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => EmirOp::TensorCreate {
            shape: shape.clone(),
            elements: elements.iter().map(|value| g(*value)).collect(),
        },
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => EmirOp::TensorIndex {
            tensor: g(tensor),
            indices: indices.iter().copied().map(g).collect(),
        },
        EmirOp::TensorSlice { tensor, ref axes } => EmirOp::TensorSlice {
            tensor: g(tensor),
            axes: axes
                .iter()
                .map(|axis| match *axis {
                    crate::EmirSliceAxis::Point(v) => crate::EmirSliceAxis::Point(g(v)),
                    crate::EmirSliceAxis::Range { start, end } => crate::EmirSliceAxis::Range {
                        start: g(start),
                        end: g(end),
                    },
                })
                .collect(),
        },
        EmirOp::TensorAdd(a, b) => EmirOp::TensorAdd(g(a), g(b)),
        EmirOp::TensorSub(a, b) => EmirOp::TensorSub(g(a), g(b)),
        EmirOp::TensorScale(a, b) => EmirOp::TensorScale(g(a), g(b)),
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => EmirOp::Einsum {
            subscripts: subscripts.clone(),
            inputs: inputs.iter().copied().map(g).collect(),
        },
        EmirOp::Factorial(a) => EmirOp::Factorial(g(a)),
        EmirOp::ModInv(a, b) => EmirOp::ModInv(g(a), g(b)),
        EmirOp::IntRem(a, b) => EmirOp::IntRem(g(a), g(b)),
        EmirOp::Congruence(a, b, m) => EmirOp::Congruence(g(a), g(b), g(m)),
        EmirOp::PolyEvalMod(c, x, p) => EmirOp::PolyEvalMod(g(c), g(x), g(p)),
        EmirOp::RSEncode(c, n, p) => EmirOp::RSEncode(g(c), g(n), g(p)),
        EmirOp::HammingDistance(a, b) => EmirOp::HammingDistance(g(a), g(b)),
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => EmirOp::Fold {
            start: g(start),
            end: g(end),
            init: g(init),
            combine,
            loop_var_index,
            body: body.clone(),
        },
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            ref integrand,
        } => EmirOp::Integral {
            start: g(start),
            end: g(end),
            steps,
            loop_var_index,
            integrand: integrand.clone(),
        },
        EmirOp::Differentiate {
            ref body,
            var_index,
        } => EmirOp::Differentiate {
            body: body.clone(),
            var_index,
        },
        EmirOp::Solve {
            ref body,
            var_index,
            tolerance,
            max_iter,
        } => EmirOp::Solve {
            body: body.clone(),
            var_index,
            tolerance,
            max_iter,
        },
        EmirOp::Optimize {
            ref body,
            ref var_indices,
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        } => EmirOp::Optimize {
            body: body.clone(),
            var_indices: var_indices.clone(),
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        },
        EmirOp::SampleLimit {
            ref body,
            var_index,
            target,
            direction,
        } => EmirOp::SampleLimit {
            body: body.clone(),
            var_index,
            target: g(target),
            direction: g(direction),
        },
        EmirOp::ReverseMode {
            ref body,
            ref var_indices,
        } => EmirOp::ReverseMode {
            body: body.clone(),
            var_indices: var_indices.clone(),
        },
        EmirOp::ApplyCapability {
            ref capability,
            ref class,
            ref args,
        } => EmirOp::ApplyCapability {
            capability: capability.clone(),
            class: class.clone(),
            args: args.iter().copied().map(g).collect(),
        },
        EmirOp::FormatText {
            ref template,
            ref arguments,
        } => EmirOp::FormatText {
            template: template.clone(),
            arguments: arguments.iter().copied().map(g).collect(),
        },
        EmirOp::SpecialFunction {
            function,
            ref arguments,
            error_bound,
        } => EmirOp::SpecialFunction {
            function,
            arguments: arguments.iter().copied().map(g).collect(),
            error_bound,
        },
        EmirOp::SetCreate {
            ref elements,
            ref guards,
        } => EmirOp::SetCreate {
            elements: elements.iter().map(|value| g(*value)).collect(),
            guards: guards
                .iter()
                .map(|guard| guard.as_ref().map(|value| g(*value)))
                .collect(),
        },
        EmirOp::SetContains { element, set } => EmirOp::SetContains {
            element: g(element),
            set: g(set),
        },
        EmirOp::RecordCreate {
            ref type_name,
            ref fields,
        } => EmirOp::RecordCreate {
            type_name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| (name.clone(), g(*value)))
                .collect(),
        },
        EmirOp::VectorMap { builtin, source } => EmirOp::VectorMap {
            builtin,
            source: g(source),
        },
        EmirOp::VectorMapScalar { op, vector, scalar } => EmirOp::VectorMapScalar {
            op,
            vector: g(vector),
            scalar: g(scalar),
        },
        EmirOp::VectorReduce { reduce, source } => EmirOp::VectorReduce {
            reduce,
            source: g(source),
        },
        EmirOp::VectorAllFinite(source) => EmirOp::VectorAllFinite(g(source)),
        EmirOp::IntervalCreate(lo, hi) => EmirOp::IntervalCreate(g(lo), g(hi)),
        EmirOp::IntervalIntersect(a, b) => EmirOp::IntervalIntersect(g(a), g(b)),
    }
}

/// Mark registers reachable from `result` as needed (backward over the
/// linear SSA list; operands always precede their user), then compact the
/// op list and renumber every live register.
fn dead_code_eliminate(program: &mut EmirProgram) {
    let n = program.ops.len();
    let mut needed = vec![false; n];
    if (program.result.0 as usize) < n {
        needed[program.result.0 as usize] = true;
    }
    let mut operands = Vec::new();
    for i in (0..n).rev() {
        if needed[i] || !is_total(&program.ops[i].0, program) {
            needed[i] = true;
            operands.clear();
            operand_registers(&program.ops[i].0, &mut operands);
            for v in &operands {
                if (v.0 as usize) < n {
                    needed[v.0 as usize] = true;
                }
            }
        }
    }
    // Compact: old index -> new index. Operands precede their users, so
    // the remap table is complete when each op is rebuilt in order.
    let mut remap = vec![0u32; n];
    let mut kept: Vec<(EmirOp, Span)> = Vec::with_capacity(n);
    for i in 0..n {
        if needed[i] {
            remap[i] = kept.len() as u32;
            let (op, span) = &program.ops[i];
            let mut map = |v: EmirValue| EmirValue(remap[v.0 as usize]);
            kept.push((remap_operands(op, &mut map), span.clone()));
        }
    }
    program.ops = kept;
    if (program.result.0 as usize) < n {
        program.result = EmirValue(remap[program.result.0 as usize]);
    }
}
