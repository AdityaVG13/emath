//! Control-flow op evaluation: folds, integrals, solves, optimization, sample limits, reverse mode.

use super::*;

pub(super) fn eval_flow_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => {
            // I64 bounds stay exact; F64 bounds must be finite whole numbers
            // (bare `as i64` maps NaN→0 and Inf→saturating extremes).
            let start_i = fold_bound(registers, start, name)?;
            let end_i = fold_bound(registers, end, name)?;
            match combine {
                FoldCombine::Add | FoldCombine::Mul => {
                    let mut acc_i: Option<i64> = match register(registers, init)? {
                        Value::I64(n) => Some(*n),
                        Value::F64(_) => None,
                        _ => {
                            return Err(EvalFault::TypeConfusion {
                                register: init.0,
                                op: name,
                            });
                        }
                    };
                    let mut acc_f: f64 = if acc_i.is_none() {
                        f64_of(registers, init, name)?
                    } else {
                        0.0
                    };
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = if acc_i.is_some() {
                            Value::I64(i)
                        } else {
                            Value::F64(i as f64)
                        };
                        match evaluate(body, &body_inputs, state)? {
                            Value::I64(term) => {
                                if let Some(ref mut acc) = acc_i {
                                    *acc = match combine {
                                        FoldCombine::Add => {
                                            acc.checked_add(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
                                        FoldCombine::Mul => {
                                            acc.checked_mul(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                } else {
                                    let term_f = term as f64;
                                    acc_f = match combine {
                                        FoldCombine::Add => acc_f + term_f,
                                        FoldCombine::Mul => acc_f * term_f,
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                }
                            }
                            Value::F64(term) => {
                                if let Some(acc) = acc_i.take() {
                                    acc_f = acc as f64;
                                }
                                acc_f = match combine {
                                    FoldCombine::Add => acc_f + term,
                                    FoldCombine::Mul => acc_f * term,
                                    FoldCombine::And | FoldCombine::Or => {
                                        return Err(EvalFault::Arithmetic {
                                            op: name,
                                            detail: "numeric fold got bool combine",
                                        });
                                    }
                                };
                            }
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                });
                            }
                        }
                    }
                    Ok(match acc_i {
                        Some(n) => Value::I64(n),
                        None => Value::F64(acc_f),
                    })
                }
                FoldCombine::And | FoldCombine::Or => {
                    // `bool_of` admits Bool and numeric 0/≠0; bare `f64_of`
                    // wrongly refused a Bool vacuous init for forall/exists.
                    let mut acc = bool_of(registers, init, name)?;
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = Value::F64(i as f64);
                        let term = match evaluate(body, &body_inputs, state)? {
                            Value::Bool(b) => b,
                            Value::F64(f) => f != 0.0,
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                });
                            }
                        };
                        acc = match combine {
                            FoldCombine::And => acc && term,
                            FoldCombine::Or => acc || term,
                            FoldCombine::Add | FoldCombine::Mul => {
                                return Err(EvalFault::Arithmetic {
                                    op: name,
                                    detail: "bool fold got numeric combine",
                                });
                            }
                        };
                    }
                    Ok(Value::Bool(acc))
                }
            }
        }
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            ref integrand,
        } => {
            // Composite Simpson requires a positive even panel count; steps==0
            // is `/ 0.0` → Inf, and odd n is a silently wrong quadrature.
            if steps == 0 || steps % 2 != 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "integral steps must be positive and even",
                });
            }
            let a = f64_of(registers, start, name)?;
            let b = f64_of(registers, end, name)?;
            let n = i64::from(steps);
            let h = (b - a) / n as f64;
            let mut acc = 0.0;
            for i in 0..=n {
                let x = a + i as f64 * h;
                let weight = if i == 0 || i == n {
                    1.0
                } else if i % 2 == 0 {
                    2.0
                } else {
                    4.0
                };
                let mut body_inputs = inputs.to_vec();
                let idx = usize::from(loop_var_index);
                while body_inputs.len() <= idx {
                    body_inputs.push(Value::F64(0.0));
                }
                body_inputs[idx] = Value::F64(x);
                match evaluate(integrand, &body_inputs, state)? {
                    Value::F64(fx) => acc += weight * fx,
                    _ => {
                        return Err(EvalFault::TypeConfusion {
                            register: integrand.result.0,
                            op: name,
                        });
                    }
                }
            }
            Ok(Value::F64(acc * h / 3.0))
        }
        EmirOp::Differentiate {
            ref body,
            var_index,
        } => {
            let dual = evaluate_dual(body, inputs, state, var_index, name)?;
            Ok(Value::F64(dual.tangent))
        }
        EmirOp::Solve {
            ref body,
            var_index,
            tolerance,
            max_iter,
        } => {
            // Newton's method: x_new = x_old - f(x) / f'(x)
            // Uses dual-number evaluation for both f and f' in one
            // pass. When Newton is unreliable — the derivative
            // vanishes, or the residual/step becomes non-finite — the
            // solver falls back to a deterministic bracket scan around
            // the seed followed by bisection.
            // The fallback only reports a root whose residual is below
            // tolerance; no bracket (or a divergent bisection) still
            // refuses with a typed fault — never a hang, never a
            // silently invented root.
            let mut x = match inputs.get(var_index as usize).and_then(Value::as_real_f64) {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: var_index as u32,
                        op: name,
                    });
                }
            };
            let seed = x;
            let mut work_inputs = inputs.to_vec();
            let mut unreliable = None;
            for _ in 0..max_iter {
                work_inputs[var_index as usize] = Value::F64(x);
                let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
                let f = dual.primal;
                let df = dual.tangent;
                if f.abs() < tolerance {
                    return Ok(Value::F64(x));
                }
                // A vanished derivative is not convergence: Newton
                // cannot step, so returning `x` would silently invent
                // a root — fall back to bisection instead.
                if df.abs() < 1e-30 {
                    unreliable = Some("derivative vanished");
                    break;
                }
                if !f.is_finite() || !df.is_finite() {
                    unreliable = Some("nonfinite value");
                    break;
                }
                x -= f / df;
                if !x.is_finite() {
                    unreliable = Some("nonfinite step");
                    break;
                }
            }
            if let Some(reason) = unreliable {
                return match solve_bracket_fallback(
                    body,
                    &work_inputs,
                    state,
                    var_index,
                    seed,
                    tolerance,
                    name,
                )? {
                    Some(root) => Ok(Value::F64(root)),
                    None => Err(EvalFault::Arithmetic {
                        op: name,
                        detail: match reason {
                            "derivative vanished" => "solve derivative vanished before convergence",
                            _ => {
                                "solve produced a nonfinite value and found no sign-changing bracket in the deterministic scan"
                            }
                        },
                    }),
                };
            }
            // Accept a root landed by the final Newton update; otherwise
            // refuse rather than invent one (same rule as causal_newton).
            work_inputs[var_index as usize] = Value::F64(x);
            let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
            if dual.primal.abs() < tolerance {
                return Ok(Value::F64(x));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "solve did not converge within max_iter",
            })
        }
        EmirOp::Optimize {
            ref body,
            ref var_indices,
            maximize,
            learning_rate: _,
            tolerance,
            max_iter,
        } => {
            // Newton's method on ∇f = 0. A claimed min/max must be a
            // stationary point: x -= H^{-1} ∇f, with H from a
            // forward-difference of the dual gradient. Fixed-step
            // gradient descent with a small penalty weight could stop
            // at a point that was neither stationary for f nor feasible.
            if var_indices.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize requires at least one variable",
                });
            }
            let mut work_inputs = inputs.to_vec();
            let mut x: Vec<f64> = Vec::with_capacity(var_indices.len());
            for &vi in var_indices {
                match inputs.get(vi as usize).and_then(Value::as_real_f64) {
                    Some(v) => x.push(v),
                    None => {
                        return Err(if inputs.get(vi as usize).is_none() {
                            EvalFault::MissingInput(vi)
                        } else {
                            EvalFault::TypeConfusion {
                                register: u32::from(vi),
                                op: name,
                            }
                        });
                    }
                }
            }
            const FD_EPS: f64 = 1e-8;
            for _ in 0..max_iter {
                let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
                if max_grad < tolerance {
                    return Ok(Value::F64(x[0]));
                }
                let n = x.len();
                let mut hess = vec![vec![0.0_f64; n]; n];
                for j in 0..n {
                    x[j] += FD_EPS;
                    let perturbed =
                        optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                    x[j] -= FD_EPS;
                    for i in 0..n {
                        hess[i][j] = (perturbed[i] - grads[i]) / FD_EPS;
                    }
                }
                let delta = dense_solve(&hess, &grads).map_err(|_| EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize hessian vanished before stationarity",
                })?;
                let dot: f64 = grads.iter().zip(delta.iter()).map(|(g, d)| g * d).sum();
                // Newton on ∇f = 0 finds any stationary point. Refuse a
                // min returned as a max (or vice versa): g·(H^{-1}g) is
                // positive iff H is positive definite along g.
                if maximize {
                    if dot >= 0.0 {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "optimize hessian has the wrong curvature for maximize",
                        });
                    }
                } else if dot <= 0.0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "optimize hessian has the wrong curvature for minimize",
                    });
                }
                for (xi, d) in x.iter_mut().zip(delta.iter()) {
                    *xi -= d;
                }
            }
            let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
            let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
            if max_grad < tolerance {
                return Ok(Value::F64(x[0]));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "optimize did not converge within max_iter",
            })
        }
        EmirOp::SampleLimit {
            ref body,
            var_index,
            target,
            direction,
        } => {
            // Numerical limit approximation: sample the body at points
            // approaching the target along a geometric sequence of step
            // sizes (0.1, 0.01, ..., 1e-12). Return the last finite value
            // whose predecessor was also finite and within 1% of it
            // (convergence check). If no pair converges, return the last
            // finite sample.
            let target_val = match registers
                .get(target.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: target.0,
                        op: name,
                    });
                }
            };
            let dir_val = match registers
                .get(direction.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: direction.0,
                        op: name,
                    });
                }
            };
            let mut work_inputs = inputs.to_vec();
            while work_inputs.len() <= var_index as usize {
                work_inputs.push(Value::F64(0.0));
            }
            let directions: &[f64] = match dir_val as i64 {
                0 => &[1.0, -1.0], // two-sided
                1 => &[1.0],       // from above
                -1 => &[-1.0],     // from below
                _ => &[1.0, -1.0], // fallback: two-sided
            };
            let mut best = f64::NAN;
            let mut prev = f64::NAN;
            for step_exp in 1..=12u32 {
                let h = 10f64.powi(-(step_exp as i32));
                for &d in directions {
                    let x = target_val + d * h;
                    work_inputs[var_index as usize] = Value::F64(x);
                    match evaluate(body, &work_inputs, state) {
                        Ok(val) => {
                            if let Some(fx) = val.as_real_f64() {
                                if fx.is_finite() {
                                    if prev.is_finite()
                                        && (fx - prev).abs() <= fx.abs() * 0.01 + 1e-14
                                    {
                                        // Converged: successive samples agree to 1%.
                                        return Ok(Value::F64(fx));
                                    }
                                    prev = fx;
                                    best = fx;
                                }
                            }
                        }
                        _ => {} // non-finite or wrong type: skip
                    }
                }
            }
            if best.is_finite() {
                Ok(Value::F64(best))
            } else {
                Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "sample_limit produced no finite values",
                })
            }
        }
        EmirOp::ReverseMode {
            ref body,
            ref var_indices,
        } => evaluate_reverse(body, inputs, state, var_indices, name),
        _ => unreachable!("eval_flow_op routed a non-matching op"),
    }
}
