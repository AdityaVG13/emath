//! Forward-mode autodiff via dual numbers (primal, tangent) pairs.

use crate::{EmirOp, EmirProgram, EmirValue};
use super::{EvalFault, Value};

/// Dual number for forward-mode autodiff: (primal, tangent).
#[derive(Clone)]
pub(super) struct Dual {
    pub(super) primal: f64,
    pub(super) tangent: f64,
}

/// Evaluate `program` with dual numbers, seeding `var_index` with tangent
/// 1.0; returns the result's (value, derivative) pair.
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
            // Bool constants encode as 1.0/0.0, like the dual-space bool ops.
            EmirOp::ConstBool(value) => Dual {
                primal: if *value { 1.0 } else { 0.0 },
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
            EmirOp::UnaryBuiltin(id, a) => {
                let val = dual_of(&registers, a, name)?;
                let (primal, tangent) = id.eval_dual_unary(val.primal, val.tangent);
                Dual { primal, tangent }
            }
            EmirOp::Stencil1d { .. } | EmirOp::Stencil2d { .. } => {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "spatial stencil ops are not differentiable in Phase 1",
                });
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
            EmirOp::BinaryBuiltin(id, a, b) => {
                let av = dual_of(&registers, a, name)?;
                let bv = dual_of(&registers, b, name)?;
                let (primal, tangent) = id.eval_dual_binary(av.primal, av.tangent, bv.primal, bv.tangent);
                Dual { primal, tangent }
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
