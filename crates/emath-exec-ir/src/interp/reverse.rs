//! Reverse-mode autodiff (adjoint method) via Wengert tape: forward pass
//! records primals, backward pass propagates adjoints in one traversal
//! (O(cost) for any input count).

use std::collections::HashMap;

use super::{EvalFault, Value, eval_op};
use crate::{EmirOp, EmirProgram, EmirValue};

/// Gradients of `program` w.r.t. each `var_indices` entry: forward pass
/// records primals, backward pass propagates adjoints from the result.
pub(super) fn evaluate_reverse(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    var_indices: &[u16],
    name: &'static str,
) -> Result<Value, EvalFault> {
    // ── Forward pass: record primals (typed, including vectors) ──
    let mut primals: Vec<Value> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let primal = eval_op(op, &primals, inputs, state)?;
        primals.push(primal);
    }

    // ── Backward pass: propagate adjoints ──
    let n_regs = primals.len();
    let mut adjoints = vec![0.0_f64; n_regs];
    let mut vec_adjoints: HashMap<usize, Vec<f64>> = HashMap::new();
    let result_idx = program.result.0 as usize;
    if result_idx >= n_regs {
        return Err(EvalFault::BadRegister(program.result.0));
    }
    adjoints[result_idx] = 1.0;

    let mut input_adjoints = vec![0.0_f64; program.input_count as usize];

    for (idx, (op, _)) in program.ops.iter().enumerate().rev() {
        let adj = adjoints[idx];
        if adj == 0.0 && !vec_adjoints.contains_key(&idx) {
            continue; // dead branch — no contribution
        }
        backward_step(
            op,
            idx,
            adj,
            &primals,
            &mut adjoints,
            &mut vec_adjoints,
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

/// Backward pass: propagate adjoint from op output to its inputs.
fn backward_step(
    op: &EmirOp,
    idx: usize,
    adj: f64,
    primals: &[Value],
    adjoints: &mut [f64],
    vec_adjoints: &mut HashMap<usize, Vec<f64>>,
    input_adjoints: &mut [f64],
    name: &'static str,
) -> Result<(), EvalFault> {
    let p = |v: &EmirValue| -> Result<f64, EvalFault> {
        primals
            .get(v.0 as usize)
            .and_then(Value::as_real_f64)
            .ok_or(EvalFault::TypeConfusion {
                register: v.0,
                op: name,
            })
    };
    let push_adj = |adjoints: &mut [f64], v: &EmirValue, delta: f64| {
        if let Some(slot) = adjoints.get_mut(v.0 as usize) {
            *slot += delta;
        }
    };

    match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstComplex(..)
        | EmirOp::ConstBool(_) => {}
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
        EmirOp::UnaryBuiltin(id, a) => {
            let primal_in = p(a)?;
            let primal_out = primals.get(idx).and_then(Value::as_real_f64).unwrap_or(0.0);
            let input_adj = id.backward_unary(primal_in, primal_out, adj);
            push_adj(adjoints, a, input_adj);
        }
        EmirOp::F64Pow(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals.get(idx).and_then(Value::as_real_f64).unwrap_or(0.0);
            // d/da [a^b] = b * a^(b-1). Skipping at a==0 was wrong:
            // d/dx[x^1]|_0 = 1, not 0. x^0 is identically 1.
            if pb == 0.0 {
                // base adjoint is 0, including 0^0
            } else if pa != 0.0 {
                push_adj(adjoints, a, adj * primal_out * pb / pa);
            } else {
                push_adj(adjoints, a, adj * pb * pa.powf(pb - 1.0));
            }
            // d/db [a^b] = a^b * ln(a). IEEE ln of non-positive is NaN,
            // matching dual's general path (do not silently zero).
            push_adj(adjoints, b, adj * primal_out * pa.ln());
        }
        EmirOp::BinaryBuiltin(id, a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals.get(idx).and_then(Value::as_real_f64).unwrap_or(0.0);
            let (adj_a, adj_b) = id.backward_binary(pa, pb, primal_out, adj);
            push_adj(adjoints, a, adj_a);
            push_adj(adjoints, b, adj_b);
        }
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: e,
        } => {
            let cv = match primals.get(c.0 as usize) {
                Some(Value::Bool(flag)) => {
                    if *flag {
                        1.0
                    } else {
                        0.0
                    }
                }
                Some(value) => value.as_real_f64().unwrap_or(0.0),
                None => 0.0,
            };
            if cv != 0.0 {
                push_adj(adjoints, t, adj);
            } else {
                push_adj(adjoints, e, adj);
            }
        }
        EmirOp::VectorCreate(elems) => {
            if let Some(adj_vec) = vec_adjoints.get(&idx).cloned() {
                for (elem, delta) in elems.iter().zip(adj_vec.iter()) {
                    push_adj(adjoints, elem, *delta);
                }
            }
        }
        EmirOp::VectorAdd(a, b) => {
            if let Some(adj_vec) = vec_adjoints.get(&idx).cloned() {
                add_vec_adj(vec_adjoints, a.0 as usize, &adj_vec);
                add_vec_adj(vec_adjoints, b.0 as usize, &adj_vec);
            }
        }
        EmirOp::VectorSub(a, b) => {
            if let Some(adj_vec) = vec_adjoints.get(&idx).cloned() {
                add_vec_adj(vec_adjoints, a.0 as usize, &adj_vec);
                let neg: Vec<f64> = adj_vec.iter().map(|d| -d).collect();
                add_vec_adj(vec_adjoints, b.0 as usize, &neg);
            }
        }
        EmirOp::VectorScale(a, b) => {
            if let Some(adj_vec) = vec_adjoints.get(&idx).cloned() {
                let (vec_reg, scale_reg, vec_primal, scale_primal) =
                    match (primals.get(a.0 as usize), primals.get(b.0 as usize)) {
                        (Some(Value::Vector(v)), Some(s)) => {
                            (a, b, v.clone(), s.as_real_f64().unwrap_or(0.0))
                        }
                        (Some(s), Some(Value::Vector(v))) => {
                            (b, a, v.clone(), s.as_real_f64().unwrap_or(0.0))
                        }
                        _ => {
                            return Err(EvalFault::TypeConfusion {
                                register: a.0,
                                op: name,
                            });
                        }
                    };
                let scaled: Vec<f64> = adj_vec.iter().map(|d| d * scale_primal).collect();
                add_vec_adj(vec_adjoints, vec_reg.0 as usize, &scaled);
                let scale_adj: f64 = adj_vec
                    .iter()
                    .zip(vec_primal.iter())
                    .map(|(d, u)| d * u)
                    .sum();
                push_adj(adjoints, scale_reg, scale_adj);
            }
        }
        EmirOp::VectorDot(a, b) => {
            let ua = match primals.get(a.0 as usize) {
                Some(Value::Vector(v)) => v,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: a.0,
                        op: name,
                    });
                }
            };
            let va = match primals.get(b.0 as usize) {
                Some(Value::Vector(v)) => v,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: b.0,
                        op: name,
                    });
                }
            };
            let adj_a: Vec<f64> = va.iter().map(|v| adj * v).collect();
            let adj_b: Vec<f64> = ua.iter().map(|u| adj * u).collect();
            add_vec_adj(vec_adjoints, a.0 as usize, &adj_a);
            add_vec_adj(vec_adjoints, b.0 as usize, &adj_b);
        }
        EmirOp::IsFinite(_)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Not(_)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..) => {
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

fn add_vec_adj(vec_adjoints: &mut HashMap<usize, Vec<f64>>, idx: usize, delta: &[f64]) {
    let slot = vec_adjoints
        .entry(idx)
        .or_insert_with(|| vec![0.0; delta.len()]);
    if slot.len() != delta.len() {
        slot.resize(delta.len(), 0.0);
    }
    for (dst, src) in slot.iter_mut().zip(delta.iter()) {
        *dst += *src;
    }
}
