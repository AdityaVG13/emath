//! Reverse-mode autodiff (adjoint method) via Wengert tape.
//!
//! Forward pass records all primal values; backward pass propagates
//! adjoints in a single reverse traversal.  Efficient for scalar
//! functions of many inputs: O(cost) regardless of input count.

use crate::{EmirOp, EmirProgram, EmirValue};
use super::{EvalFault, Value};

/// Evaluate an EMIR sub-program in reverse mode, returning the gradient
/// w.r.t. each specified input index.
///
/// 1. Forward pass: evaluate all ops, storing primal f64 values.
/// 2. Backward pass: seed adjoint[result] = 1.0, propagate backward.
/// 3. Collect input adjoints for the requested var indices.
pub(super) fn evaluate_reverse(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    var_indices: &[u16],
    name: &'static str,
) -> Result<Value, EvalFault> {
    // ── Forward pass: record primals ──
    let mut primals: Vec<f64> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let primal = forward_primal(op, &primals, inputs, state, name)?;
        primals.push(primal);
    }

    // ── Backward pass: propagate adjoints ──
    let n_regs = primals.len();
    let mut adjoints = vec![0.0_f64; n_regs];
    let result_idx = program.result.0 as usize;
    if result_idx >= n_regs {
        return Err(EvalFault::BadRegister(program.result.0));
    }
    adjoints[result_idx] = 1.0;

    let mut input_adjoints = vec![0.0_f64; program.input_count as usize];

    for (idx, (op, _)) in program.ops.iter().enumerate().rev() {
        let adj = adjoints[idx];
        if adj == 0.0 {
            continue; // dead branch — no contribution
        }
        backward_step(
            op,
            idx,
            adj,
            &primals,
            &mut adjoints,
            &mut input_adjoints,
            name,
        )?;
    }

    // ── Collect requested gradients ──
    let grads: Vec<f64> = var_indices
        .iter()
        .map(|&vi| {
            let i = vi as usize;
            if i < input_adjoints.len() {
                input_adjoints[i]
            } else {
                0.0
            }
        })
        .collect();

    Ok(Value::Vector(grads))
}

/// Forward pass: compute the primal value of an op given prior primals.
fn forward_primal(
    op: &EmirOp,
    primals: &[f64],
    inputs: &[Value],
    state: &[Value],
    name: &'static str,
) -> Result<f64, EvalFault> {
    let f = |v: &EmirValue| -> Result<f64, EvalFault> {
        primals
            .get(v.0 as usize)
            .copied()
            .ok_or(EvalFault::TypeConfusion { register: v.0, op: name })
    };
    match op {
        EmirOp::ConstF64(bits) => Ok(f64::from_bits(*bits)),
        EmirOp::ConstI64(value) => Ok(*value as f64),
        EmirOp::ConstComplex(re, _im) => Ok(*re), // reverse-mode on real part
        EmirOp::LoadInput(idx) => {
            match inputs.get(*idx as usize) {
                Some(Value::F64(v)) => Ok(*v),
                _ => Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
            }
        }
        EmirOp::LoadState(idx) => {
            match state.get(*idx as usize) {
                Some(Value::F64(v)) => Ok(*v),
                _ => Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
            }
        }
        EmirOp::F64Add(a, b) => Ok(f(a)? + f(b)?),
        EmirOp::F64Sub(a, b) => Ok(f(a)? - f(b)?),
        EmirOp::F64Mul(a, b) => Ok(f(a)? * f(b)?),
        EmirOp::F64Div(a, b) => {
            let bv = f(b)?;
            if bv == 0.0 {
                return Err(EvalFault::Arithmetic { op: name, detail: "division by zero in reverse-mode forward pass" });
            }
            Ok(f(a)? / bv)
        }
        EmirOp::Neg(a) => Ok(-f(a)?),
        EmirOp::Exp(a) => Ok(f(a)?.exp()),
        EmirOp::Ln(a) => {
            let av = f(a)?;
            if av <= 0.0 {
                return Err(EvalFault::Arithmetic { op: name, detail: "ln of non-positive in reverse-mode forward pass" });
            }
            Ok(av.ln())
        }
        EmirOp::Sqrt(a) => {
            let av = f(a)?;
            if av < 0.0 {
                return Err(EvalFault::Arithmetic { op: name, detail: "sqrt of negative in reverse-mode forward pass" });
            }
            Ok(av.sqrt())
        }
        EmirOp::Sin(a) => Ok(f(a)?.sin()),
        EmirOp::Cos(a) => Ok(f(a)?.cos()),
        EmirOp::Tan(a) => Ok(f(a)?.tan()),
        EmirOp::Tanh(a) => Ok(f(a)?.tanh()),
        EmirOp::Abs(a) => Ok(f(a)?.abs()),
        EmirOp::Floor(a) => Ok(f(a)?.floor()),
        EmirOp::Ceil(a) => Ok(f(a)?.ceil()),
        EmirOp::Round(a) => Ok(f(a)?.round()),
        EmirOp::Sign(a) => Ok(f(a)?.signum()),
        EmirOp::Log2(a) => Ok(f(a)?.log2()),
        EmirOp::Log10(a) => Ok(f(a)?.log10()),
        EmirOp::Sinh(a) => Ok(f(a)?.sinh()),
        EmirOp::Cosh(a) => Ok(f(a)?.cosh()),
        EmirOp::Atan(a) => Ok(f(a)?.atan()),
        EmirOp::Cbrt(a) => Ok(f(a)?.cbrt()),
        EmirOp::Recip(a) => Ok(f(a)?.recip()),
        EmirOp::Fract(a) => Ok(f(a)?.fract()),
        EmirOp::Hypot(a, b) => Ok(f(a)?.hypot(f(b)?)),
        EmirOp::F64Pow(a, b) => Ok(f(a)?.powf(f(b)?)),
        EmirOp::Min(a, b) => Ok(f(a)?.min(f(b)?)),
        EmirOp::Max(a, b) => Ok(f(a)?.max(f(b)?)),
        EmirOp::Atan2(a, b) => Ok(f(a)?.atan2(f(b)?)),
        EmirOp::Mod(a, b) => {
            let bv = f(b)?;
            if bv == 0.0 {
                return Err(EvalFault::Arithmetic { op: name, detail: "mod by zero in reverse-mode forward pass" });
            }
            Ok(f(a)? % bv)
        }
        EmirOp::Select { condition: c, then_value: t, else_value: e } => {
            let cv = f(c)?;
            if cv != 0.0 { Ok(f(t)?) } else { Ok(f(e)?) }
        }
        EmirOp::IsFinite(a) => Ok(if f(a)?.is_finite() { 1.0 } else { 0.0 }),
        // Comparison and boolean ops: primal is 0.0 or 1.0, not differentiable.
        EmirOp::Eq(a, b) => Ok(if f(a)? == f(b)? { 1.0 } else { 0.0 }),
        EmirOp::Ne(a, b) => Ok(if f(a)? != f(b)? { 1.0 } else { 0.0 }),
        EmirOp::Lt(a, b) => Ok(if f(a)? < f(b)? { 1.0 } else { 0.0 }),
        EmirOp::Le(a, b) => Ok(if f(a)? <= f(b)? { 1.0 } else { 0.0 }),
        EmirOp::Gt(a, b) => Ok(if f(a)? > f(b)? { 1.0 } else { 0.0 }),
        EmirOp::Ge(a, b) => Ok(if f(a)? >= f(b)? { 1.0 } else { 0.0 }),
        EmirOp::And(a, b) => Ok(if f(a)? != 0.0 && f(b)? != 0.0 { 1.0 } else { 0.0 }),
        EmirOp::Or(a, b) => Ok(if f(a)? != 0.0 || f(b)? != 0.0 { 1.0 } else { 0.0 }),
        EmirOp::Not(a) => Ok(if f(a)? == 0.0 { 1.0 } else { 0.0 }),
        EmirOp::Imply(a, b) => Ok(if f(a)? == 0.0 || f(b)? != 0.0 { 1.0 } else { 0.0 }),
        EmirOp::Iff(a, b) => Ok(if (f(a)? != 0.0) == (f(b)? != 0.0) { 1.0 } else { 0.0 }),
        _ => Err(EvalFault::Arithmetic {
            op: name,
            detail: "unsupported op in reverse-mode forward pass",
        }),
    }
}

/// Backward pass: propagate adjoint from op output to its inputs.
fn backward_step(
    op: &EmirOp,
    idx: usize,
    adj: f64,
    primals: &[f64],
    adjoints: &mut [f64],
    input_adjoints: &mut [f64],
    name: &'static str,
) -> Result<(), EvalFault> {
    let p = |v: &EmirValue| -> Result<f64, EvalFault> {
        primals
            .get(v.0 as usize)
            .copied()
            .ok_or(EvalFault::TypeConfusion { register: v.0, op: name })
    };
    let push_adj = |adjoints: &mut [f64], v: &EmirValue, delta: f64| {
        if let Some(slot) = adjoints.get_mut(v.0 as usize) {
            *slot += delta;
        }
    };

    match op {
        EmirOp::ConstF64(_) | EmirOp::ConstI64(_) | EmirOp::ConstComplex(..) => {}
        EmirOp::LoadInput(i) => {
            let ii = *i as usize;
            if ii < input_adjoints.len() {
                input_adjoints[ii] += adj;
            }
        }
        EmirOp::LoadState(_) => {}
        EmirOp::F64Add(a, b) => {
            push_adj(adjoints, a, adj);
            push_adj(adjoints, b, adj);
        }
        EmirOp::F64Sub(a, b) => {
            push_adj(adjoints, a, adj);
            push_adj(adjoints, b, -adj);
        }
        EmirOp::F64Mul(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            push_adj(adjoints, a, adj * pb);
            push_adj(adjoints, b, adj * pa);
        }
        EmirOp::F64Div(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            push_adj(adjoints, a, adj / pb);
            push_adj(adjoints, b, -adj * pa / (pb * pb));
        }
        EmirOp::Neg(a) => push_adj(adjoints, a, -adj),
        EmirOp::Exp(a) => {
            // d/dx exp(x) = exp(x) = primal[output]
            let primal_out = primals[idx];
            push_adj(adjoints, a, adj * primal_out);
        }
        EmirOp::Ln(a) => {
            let pa = p(a)?;
            if pa != 0.0 {
                push_adj(adjoints, a, adj / pa);
            }
        }
        EmirOp::Sqrt(a) => {
            let primal_out = primals[idx];
            if primal_out != 0.0 {
                push_adj(adjoints, a, adj / (2.0 * primal_out));
            }
        }
        EmirOp::Sin(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, adj * pa.cos());
        }
        EmirOp::Cos(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, -adj * pa.sin());
        }
        EmirOp::Tan(a) => {
            let pa = p(a)?;
            let c = pa.cos();
            if c != 0.0 {
                push_adj(adjoints, a, adj / (c * c));
            }
        }
        EmirOp::Tanh(a) => {
            let primal_out = primals[idx];
            push_adj(adjoints, a, adj * (1.0 - primal_out * primal_out));
        }
        EmirOp::Abs(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, adj * pa.signum());
        }
        EmirOp::Floor(_) | EmirOp::Ceil(_) | EmirOp::Round(_) | EmirOp::Sign(_) => {}
        EmirOp::Log2(a) => {
            let pa = p(a)?;
            if pa != 0.0 {
                push_adj(adjoints, a, adj / (pa * std::f64::consts::LN_2));
            }
        }
        EmirOp::Log10(a) => {
            let pa = p(a)?;
            if pa != 0.0 {
                push_adj(adjoints, a, adj / (pa * std::f64::consts::LN_10));
            }
        }
        EmirOp::Sinh(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, adj * pa.cosh());
        }
        EmirOp::Cosh(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, adj * pa.sinh());
        }
        EmirOp::Atan(a) => {
            let pa = p(a)?;
            push_adj(adjoints, a, adj / (1.0 + pa * pa));
        }
        EmirOp::Cbrt(a) => {
            let primal_out = primals[idx];
            if primal_out != 0.0 {
                push_adj(adjoints, a, adj / (3.0 * primal_out * primal_out));
            }
        }
        EmirOp::Recip(a) => {
            let pa = p(a)?;
            if pa != 0.0 {
                push_adj(adjoints, a, -adj / (pa * pa));
            }
        }
        EmirOp::Fract(a) => push_adj(adjoints, a, adj),
        EmirOp::Hypot(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals[idx];
            if primal_out != 0.0 {
                push_adj(adjoints, a, adj * pa / primal_out);
                push_adj(adjoints, b, adj * pb / primal_out);
            }
        }
        EmirOp::F64Pow(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals[idx];
            // d/da [a^b] = b * a^(b-1) = primal_out * b / a
            if pa != 0.0 {
                push_adj(adjoints, a, adj * primal_out * pb / pa);
            }
            // d/db [a^b] = a^b * ln(a) = primal_out * ln(a)
            if pa > 0.0 {
                push_adj(adjoints, b, adj * primal_out * pa.ln());
            }
        }
        EmirOp::Min(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            if pa <= pb {
                push_adj(adjoints, a, adj);
            } else {
                push_adj(adjoints, b, adj);
            }
        }
        EmirOp::Max(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            if pa >= pb {
                push_adj(adjoints, a, adj);
            } else {
                push_adj(adjoints, b, adj);
            }
        }
        EmirOp::Atan2(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let denom = pa * pa + pb * pb;
            if denom != 0.0 {
                push_adj(adjoints, a, adj * pb / denom);
                push_adj(adjoints, b, -adj * pa / denom);
            }
        }
        EmirOp::Mod(a, _b) => {
            // d/da [a mod b] = 1 at non-boundary points
            push_adj(adjoints, a, adj);
        }
        EmirOp::Select { condition: c, then_value: t, else_value: e } => {
            let cv = primals
                .get(c.0 as usize)
                .copied()
                .unwrap_or(0.0);
            if cv != 0.0 {
                push_adj(adjoints, t, adj);
            } else {
                push_adj(adjoints, e, adj);
            }
        }
        EmirOp::IsFinite(_) | EmirOp::Eq(..) | EmirOp::Ne(..)
        | EmirOp::Lt(..) | EmirOp::Le(..) | EmirOp::Gt(..) | EmirOp::Ge(..)
        | EmirOp::And(..) | EmirOp::Or(..) | EmirOp::Not(_)
        | EmirOp::Imply(..) | EmirOp::Iff(..) => {
            // Not differentiable — no adjoint contribution.
        }
        _ => {
            return Err(EvalFault::Arithmetic {
                op: name,
                detail: "unsupported op in reverse-mode backward pass",
            });
        }
    }
    Ok(())
}
