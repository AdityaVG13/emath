//! Forward-mode autodiff via dual numbers, extracted from the main
//! interpreter.  See [`evaluate_dual`].

use crate::{EmirOp, EmirProgram, EmirValue};
use super::{EvalFault, Value};

/// Dual number for forward-mode autodiff: (primal, tangent).
#[derive(Clone)]
pub(super) struct Dual {
    pub(super) primal: f64,
    pub(super) tangent: f64,
}

/// Evaluate an EMIR sub-program with dual numbers, seeding the input at
/// `var_index` with tangent 1.0.  Returns the full dual (primal +
/// tangent) of the result — i.e. both the function value and its
/// derivative with respect to that input.
pub(super) fn evaluate_dual(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    var_index: u16,
    name: &'static str,
) -> Result<Dual, EvalFault> {
    let mut registers: Vec<Dual> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let dual = match op {
            EmirOp::ConstF64(bits) => Dual {
                primal: f64::from_bits(*bits),
                tangent: 0.0,
            },
            EmirOp::ConstI64(value) => Dual {
                primal: *value as f64,
                tangent: 0.0,
            },
            EmirOp::LoadInput(idx) => {
                let primal = match inputs.get(*idx as usize) {
                    Some(Value::F64(v)) => *v,
                    _ => return Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
                };
                let tangent = if *idx == var_index { 1.0 } else { 0.0 };
                Dual { primal, tangent }
            }
            EmirOp::LoadState(idx) => {
                let primal = match state.get(*idx as usize) {
                    Some(Value::F64(v)) => *v,
                    _ => return Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
                };
                Dual { primal, tangent: 0.0 }
            }
            EmirOp::F64Add(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: a.primal + b.primal, tangent: a.tangent + b.tangent }
            }
            EmirOp::F64Sub(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: a.primal - b.primal, tangent: a.tangent - b.tangent }
            }
            EmirOp::F64Mul(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual {
                    primal: a.primal * b.primal,
                    tangent: a.tangent * b.primal + a.primal * b.tangent,
                }
            }
            EmirOp::F64Div(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let bp2 = b.primal * b.primal;
                Dual {
                    primal: a.primal / b.primal,
                    tangent: (a.tangent * b.primal - a.primal * b.tangent) / bp2,
                }
            }
            EmirOp::Neg(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: -a.primal, tangent: -a.tangent }
            }
            EmirOp::Exp(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.exp();
                Dual { primal: p, tangent: p * a.tangent }
            }
            EmirOp::Ln(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.ln(), tangent: a.tangent / a.primal }
            }
            EmirOp::Sqrt(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.sqrt();
                Dual { primal: p, tangent: a.tangent / (2.0 * p) }
            }
            EmirOp::Sin(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.sin(), tangent: a.primal.cos() * a.tangent }
            }
            EmirOp::Cos(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.cos(), tangent: -a.primal.sin() * a.tangent }
            }
            EmirOp::Tan(a) => {
                let a = dual_of(&registers, a, name)?;
                let c = a.primal.cos();
                Dual { primal: a.primal.tan(), tangent: a.tangent / (c * c) }
            }
            EmirOp::Tanh(a) => {
                let a = dual_of(&registers, a, name)?;
                let t = a.primal.tanh();
                Dual { primal: t, tangent: (1.0 - t * t) * a.tangent }
            }
            EmirOp::Abs(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.abs(), tangent: a.primal.signum() * a.tangent }
            }
            EmirOp::Stencil1d { .. } | EmirOp::Stencil2d { .. } => {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "spatial stencil ops are not differentiable in Phase 1",
                });
            }
            EmirOp::Floor(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.floor(), tangent: 0.0 }
            }
            EmirOp::Ceil(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.ceil(), tangent: 0.0 }
            }
            EmirOp::Round(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.round(), tangent: 0.0 }
            }
            EmirOp::Sign(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.signum(), tangent: 0.0 }
            }
            EmirOp::Log2(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.log2(), tangent: a.tangent / (a.primal * std::f64::consts::LN_2) }
            }
            EmirOp::Log10(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.log10(), tangent: a.tangent / (a.primal * std::f64::consts::LN_10) }
            }
            EmirOp::Sinh(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.sinh(), tangent: a.primal.cosh() * a.tangent }
            }
            EmirOp::Cosh(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.cosh(), tangent: a.primal.sinh() * a.tangent }
            }
            EmirOp::Atan(a) => {
                let a = dual_of(&registers, a, name)?;
                let d = 1.0 / (1.0 + a.primal * a.primal);
                Dual { primal: a.primal.atan(), tangent: d * a.tangent }
            }
            EmirOp::Cbrt(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.cbrt();
                Dual { primal: p, tangent: a.tangent / (3.0 * p * p) }
            }
            EmirOp::Recip(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.recip();
                Dual { primal: p, tangent: -a.tangent * p * p }
            }
            EmirOp::Fract(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.fract(), tangent: a.tangent }
            }
            EmirOp::Hypot(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let h = a.primal.hypot(b.primal);
                // At the origin the Euclidean norm is not differentiable;
                // emit 0 rather than poisoning Newton/optimize with Inf/NaN.
                let tangent = if h == 0.0 {
                    0.0
                } else {
                    (a.primal * a.tangent + b.primal * b.tangent) / h
                };
                Dual { primal: h, tangent }
            }
            EmirOp::F64Pow(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let p = a.primal.powf(b.primal);
                if b.tangent == 0.0 {
                    // Constant exponent: d/dx [a^b] = b * a^(b-1) * a'
                    Dual { primal: p, tangent: b.primal * a.primal.powf(b.primal - 1.0) * a.tangent }
                } else {
                    // General: a^b * (b * a'/a + b' * ln(a))
                    Dual { primal: p, tangent: p * (b.primal * a.tangent / a.primal + b.tangent * a.primal.ln()) }
                }
            }
            EmirOp::Min(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                if a.primal < b.primal {
                    Dual { primal: a.primal, tangent: a.tangent }
                } else {
                    Dual { primal: b.primal, tangent: b.tangent }
                }
            }
            EmirOp::Max(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                if a.primal > b.primal {
                    Dual { primal: a.primal, tangent: a.tangent }
                } else {
                    Dual { primal: b.primal, tangent: b.tangent }
                }
            }
            EmirOp::Atan2(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let denom = a.primal * a.primal + b.primal * b.primal;
                Dual {
                    primal: a.primal.atan2(b.primal),
                    tangent: (b.primal * a.tangent - a.primal * b.tangent) / denom,
                }
            }
            EmirOp::Mod(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                // mod(a, b) = a - b * floor(a/b).  At non-boundary points,
                // d/da = 1, d/db ≈ -floor(a/b).  Use the simple approximation.
                Dual {
                    primal: a.primal % b.primal,
                    tangent: a.tangent,
                }
            }
            EmirOp::Select { condition: c, then_value: t, else_value: e } => {
                let c = dual_of(&registers, c, name)?;
                let t = dual_of(&registers, t, name)?;
                let e = dual_of(&registers, e, name)?;
                if c.primal != 0.0 {
                    Dual { primal: t.primal, tangent: t.tangent }
                } else {
                    Dual { primal: e.primal, tangent: e.tangent }
                }
            }
            EmirOp::IsFinite(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: if a.primal.is_finite() { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            // Comparisons and boolean ops: tangent is always 0.0.
            EmirOp::Eq(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal == b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Ne(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Lt(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal < b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Le(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal <= b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Gt(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal > b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Ge(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal >= b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::And(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != 0.0 && b.primal != 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Or(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != 0.0 || b.primal != 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Not(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: if a.primal == 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: program.result.0,
                    op: "differentiate (unsupported op in dual evaluation)",
                });
            }
        };
        registers.push(dual);
    }
    let result = registers
        .get(program.result.0 as usize)
        .ok_or(EvalFault::BadRegister(program.result.0))?;
    Ok(result.clone())
}

fn dual_of(registers: &[Dual], value: &EmirValue, op: &'static str) -> Result<Dual, EvalFault> {
    registers
        .get(value.0 as usize)
        .cloned()
        .ok_or(EvalFault::TypeConfusion { register: value.0, op })
}
