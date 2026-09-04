//! Constant folding over `EmirOp` streams.

use super::*;

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

pub(super) fn const_at(consts: &[Option<ConstVal>], v: EmirValue) -> Option<ConstVal> {
    consts.get(v.0 as usize).copied().flatten()
}

/// Fold helpers over the const table, mirroring the interpreter's
/// conversions exactly. A `None` result means the operand kinds would make
/// evaluation a typed fault (the op is left unfolded so the fault is
/// preserved) or an operand is not constant.
pub(super) fn fold_f64_bin(
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
pub(super) fn fold_arith(
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

pub(super) fn fold_ord(
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

pub(super) fn fold_f64_un(
    consts: &[Option<ConstVal>],
    a: EmirValue,
    f: impl FnOnce(f64) -> f64,
) -> Option<ConstVal> {
    Some(ConstVal::F64(f(const_at(consts, a)?.f64_of()?)))
}

pub(super) fn fold_bool_bin(
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

pub(super) fn fold_cmp(
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
pub(super) fn fold_op(op: &EmirOp, consts: &[Option<ConstVal>]) -> Option<ConstVal> {
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
pub(super) fn constant_fold(program: &mut EmirProgram) {
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
