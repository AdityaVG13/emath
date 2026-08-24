use emath_exec_ir::{EdgePolicy, EmirOp, EmirProgram, EmirValue, FoldCombine};
use emath_rust_ir::ast::{escape_ident, BinOp, Block, Expr, Stmt, Ty, UnOp};
use emath_rust_ir::render::render_expr;

use crate::BackendError;
use crate::codegen_helpers::{binary_method, comparison, unary_method};

/// Render the program as an expression. Multi-op programs become a block
/// `{ let __e0 = ...; ...; __eN }`; single-op programs inline directly.
pub(crate) fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
) -> Result<Expr, BackendError> {
    if program.ops.len() == 1 {
        return op_expr(&program.ops[0].0, program, names, states);
    }
    let mut statements = Vec::new();
    for (index, (op, _)) in program.ops.iter().enumerate() {
        let expr = op_expr(op, program, names, states)?;
        if index == program.ops.len() - 1 {
            // Tail: the final value is the block expression itself.
            statements.push(Stmt::Expr(expr));
        } else {
            statements.push(Stmt::Let {
                pattern: format!("__e{index}"),
                value: Box::new(expr),
            });
        }
    }
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}

/// Operand reference: every op is materialized as `__e<i>`.
pub(crate) fn operand(_program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Var(format!("__e{}", value.0))
}

pub(crate) fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) => Ok(Expr::F64((*value as f64).to_bits())),
        EmirOp::ConstComplex(re, im) => Ok(Expr::Raw(format!(
            "num_complex::Complex::new({re:?}, {im:?})"
        ))),
        EmirOp::LoadInput(index) => {
            let name = names
                .get(*index as usize)
                .ok_or_else(|| BackendError::Lowering("load-input out of range".into()))?;
            if let Some((base, field)) = name.split_once('.') {
                Ok(Expr::Field {
                    receiver: Box::new(Expr::Var(escape_ident(base))),
                    field: field.to_string(),
                })
            } else {
                Ok(Expr::Var(escape_ident(name)))
            }
        }
        EmirOp::LoadState(index) => {
            let name = states
                .get(*index as usize)
                .ok_or_else(|| BackendError::Lowering("load-state out of range".into()))?;
            Ok(Expr::Field {
                receiver: Box::new(Expr::SelfValue),
                field: name.clone(),
            })
        }
        EmirOp::F64Add(l, r) => Ok(Expr::Bin {
            op: BinOp::Add,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Sub(l, r) => Ok(Expr::Bin {
            op: BinOp::Sub,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Mul(l, r) => Ok(Expr::Bin {
            op: BinOp::Mul,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Div(l, r) => Ok(Expr::Bin {
            op: BinOp::Div,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::Neg(value) => Ok(Expr::Un {
            op: UnOp::Neg,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::Exp(value) => Ok(unary_method("exp", *value, program)),
        EmirOp::Ln(value) => Ok(unary_method("ln", *value, program)),
        EmirOp::Sqrt(value) => Ok(unary_method("sqrt", *value, program)),
        EmirOp::Sin(value) => Ok(unary_method("sin", *value, program)),
        EmirOp::Cos(value) => Ok(unary_method("cos", *value, program)),
        EmirOp::Tan(value) => Ok(unary_method("tan", *value, program)),
        EmirOp::Tanh(value) => Ok(unary_method("tanh", *value, program)),
        EmirOp::Abs(value) => Ok(unary_method("abs", *value, program)),
        EmirOp::Floor(value) => Ok(unary_method("floor", *value, program)),
        EmirOp::Ceil(value) => Ok(unary_method("ceil", *value, program)),
        EmirOp::Round(value) => Ok(unary_method("round", *value, program)),
        EmirOp::Sign(value) => Ok(unary_method("signum", *value, program)),
        EmirOp::Log2(value) => Ok(unary_method("log2", *value, program)),
        EmirOp::Log10(value) => Ok(unary_method("log10", *value, program)),
        EmirOp::Sinh(value) => Ok(unary_method("sinh", *value, program)),
        EmirOp::Cosh(value) => Ok(unary_method("cosh", *value, program)),
        EmirOp::Atan(value) => Ok(unary_method("atan", *value, program)),
        EmirOp::Cbrt(value) => Ok(unary_method("cbrt", *value, program)),
        EmirOp::Recip(value) => Ok(unary_method("recip", *value, program)),
        EmirOp::Fract(value) => Ok(unary_method("fract", *value, program)),
        EmirOp::Hypot(l, r) => Ok(binary_method("hypot", *l, *r, program)),
        EmirOp::Min(l, r) => Ok(binary_method("min", *l, *r, program)),
        EmirOp::Max(l, r) => Ok(binary_method("max", *l, *r, program)),
        EmirOp::Atan2(l, r) => Ok(binary_method("atan2", *l, *r, program)),
        EmirOp::Mod(l, r) => Ok(Expr::Bin {
            op: BinOp::Rem,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(operand(program, *value)),
            method: "is_finite".to_string(),
            args: Vec::new(),
        }),
        EmirOp::Lt(l, r) => Ok(comparison(BinOp::Lt, *l, *r, program)),
        EmirOp::Le(l, r) => Ok(comparison(BinOp::Le, *l, *r, program)),
        EmirOp::Gt(l, r) => Ok(comparison(BinOp::Gt, *l, *r, program)),
        EmirOp::Ge(l, r) => Ok(comparison(BinOp::Ge, *l, *r, program)),
        EmirOp::Eq(l, r) => Ok(comparison(BinOp::Eq, *l, *r, program)),
        EmirOp::Ne(l, r) => Ok(comparison(BinOp::Ne, *l, *r, program)),
        EmirOp::And(l, r) => Ok(comparison(BinOp::And, *l, *r, program)),
        EmirOp::Or(l, r) => Ok(comparison(BinOp::Or, *l, *r, program)),
        // `==>` = `!l || r`
        EmirOp::Imply(l, r) => Ok(Expr::Bin {
            op: BinOp::Or,
            left: Box::new(Expr::Un {
                op: UnOp::Not,
                value: Box::new(operand(program, *l)),
            }),
            right: Box::new(operand(program, *r)),
        }),
        // `<==>` = `l == r` for Bool
        EmirOp::Iff(l, r) => Ok(comparison(BinOp::Eq, *l, *r, program)),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => Ok(Expr::IfElse {
            condition: Box::new(operand(program, *condition)),
            then: Box::new(Stmt::Expr(operand(program, *then_value))),
            else_value: Box::new(Stmt::Expr(operand(program, *else_value))),
        }),
        EmirOp::VectorCreate(elements) => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements.iter().map(|e| operand(program, *e)).collect(),
        }),
        EmirOp::MatrixCreate {
            rows,
            cols,
            elements,
        } => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: (0..*rows)
                .map(|r| Expr::Macro {
                    name: "vec".to_string(),
                    args: (0..*cols)
                        .map(|c| operand(program, elements[r * cols + c]))
                        .collect(),
                })
                .collect(),
        }),
        EmirOp::VectorIndex { vector, index } => Ok(Expr::Index {
            target: Box::new(operand(program, *vector)),
            index: Box::new(Expr::Cast {
                value: Box::new(operand(program, *index)),
                target: Ty::Named("usize".to_string()),
            }),
        }),
        EmirOp::MatrixIndex { matrix, row, col } => Ok(Expr::Index {
            target: Box::new(Expr::Index {
                target: Box::new(operand(program, *matrix)),
                index: Box::new(Expr::Cast {
                    value: Box::new(operand(program, *row)),
                    target: Ty::Named("usize".to_string()),
                }),
            }),
            index: Box::new(Expr::Cast {
                value: Box::new(operand(program, *col)),
                target: Ty::Named("usize".to_string()),
            }),
        }),
        EmirOp::VectorAdd(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorSub(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a - b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorScale(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().map(|x| x * {}).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorDot(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a * b).sum::<f64>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorNorm(v) => Ok(Expr::Raw(format!(
            "{}.iter().map(|x| x * x).sum::<f64>().sqrt()",
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::VectorLength(v) => Ok(Expr::Raw(format!(
            "({}.len() as f64)",
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::Stencil1d {
            input,
            weights,
            center,
            edge,
        } => {
            let src = render_expr(&operand(program, *input));
            let w_lit = weights
                .iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            let tap = match *edge {
                EdgePolicy::Clamp => {
                    format!("w * {src}[raw.clamp(0, last) as usize]")
                }
                EdgePolicy::Neumann => format!(
                    "w * {src}[(if raw < 0 {{ -raw }} else if raw > last {{ 2 * last - raw }} else {{ raw }}).clamp(0, last) as usize]"
                ),
                EdgePolicy::Dirichlet { left, right } => format!(
                    "w * if raw < 0 {{ {left:?} }} else if raw > last {{ {right:?} }} else {{ {src}[raw as usize] }}"
                ),
            };
            Ok(Expr::Raw(format!(
                "(0..{src}.len()).map(|i| {{ let n = {src}.len(); let last = (n - 1) as isize; [{w}].iter().enumerate().map(|(k, &w)| {{ let raw = i as isize + k as isize - {c} as isize; {tap} }}).sum::<f64>() }}).collect::<Vec<f64>>()",
                src = src,
                w = w_lit,
                c = *center,
                tap = tap
            )))
        }
        EmirOp::Stencil2d {
            input,
            weights,
            center,
            edge,
        } => {
            let src = render_expr(&operand(program, *input));
            let w_lit = weights
                .iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            let (cr, cc) = *center;
            let tap = match *edge {
                EdgePolicy::Clamp => format!(
                    "w[kr * 3 + kc] * m[(raw_r).clamp(0, lr) as usize][(raw_c).clamp(0, lc) as usize]"
                ),
                EdgePolicy::Neumann => format!(
                    "w[kr * 3 + kc] * m[(if raw_r < 0 {{ -raw_r }} else if raw_r > lr {{ 2 * lr - raw_r }} else {{ raw_r }}).clamp(0, lr) as usize][(if raw_c < 0 {{ -raw_c }} else if raw_c > lc {{ 2 * lc - raw_c }} else {{ raw_c }}).clamp(0, lc) as usize]"
                ),
                EdgePolicy::Dirichlet { .. } => {
                    return Err(BackendError::Lowering(
                        "2D Dirichlet boundary is not yet supported for Stencil2d".to_string(),
                    ));
                }
            };
            Ok(Expr::Raw(format!(
                "{{ let m = &{src}; let nr = m.len(); let nc = if nr == 0 {{ 0 }} else {{ m[0].len() }}; let lr = (nr - 1) as isize; let lc = (nc - 1) as isize; let w = [{w}]; (0..nr).map(|r| (0..nc).map(|c| (0..3).flat_map(|kr| (0..3).map(move |kc| {{ let raw_r = r as isize + kr as isize - {cr} as isize; let raw_c = c as isize + kc as isize - {cc} as isize; {tap} }})).sum::<f64>()).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>() }}",
                src = src,
                w = w_lit,
                cr = cr,
                cc = cc,
                tap = tap
            )))
        }
        EmirOp::MatrixAdd(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a + b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixSub(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a - b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixScale(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().map(|row| row.iter().map(|x| x * {}).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixMulVector(m, v) => Ok(Expr::Raw(format!(
            "{{ let m = &{}; let v = &{}; m.iter().map(|row| row.iter().zip(v.iter()).map(|(a, b)| a * b).sum::<f64>()).collect::<Vec<f64>>() }}",
            render_expr(&operand(program, *m)),
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::MatrixMulMatrix(l, r) => Ok(Expr::Raw(format!(
            "{{ let m1 = &{}; let m2 = &{}; let r1 = m1.len(); let c2 = if m2.is_empty() {{ 0 }} else {{ m2[0].len() }}; let c1 = if m1.is_empty() {{ 0 }} else {{ m1[0].len() }}; (0..r1).map(|i| (0..c2).map(|j| (0..c1).map(|k| m1[i][k] * m2[k][j]).sum::<f64>()).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>() }}",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixTranspose(m) => Ok(Expr::Raw(format!(
            "{{ let m = &{}; if m.is_empty() {{ vec![] }} else {{ let rows = m.len(); let cols = m[0].len(); (0..cols).map(|c| (0..rows).map(|r| m[r][c]).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>() }} }}",
            render_expr(&operand(program, *m)),
        ))),
        EmirOp::TensorCreate { elements, .. } => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements.iter().map(|elem| operand(program, *elem)).collect(),
        }),
        EmirOp::TensorIndex { tensor, indices } => {
            let mut expr = operand(program, *tensor);
            for index in indices {
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(Expr::Cast {
                        value: Box::new(operand(program, *index)),
                        target: Ty::Named("usize".to_string()),
                    }),
                };
            }
            Ok(expr)
        }
        EmirOp::TensorSlice { tensor, axes } => Ok(Expr::Raw(format!(
            "{{ let t = &{}; /* tensor slice axes={} */ t.clone() }}",
            render_expr(&operand(program, *tensor)),
            axes.len()
        ))),
        EmirOp::TensorAdd(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::TensorSub(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a - b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        // einsum codegen: the interp handles evaluation; Rust codegen
        // emits a placeholder that panics at runtime. Full tensor
        // codegen is a Stage 2 concern.
        EmirOp::Einsum { subscripts, .. } => Ok(Expr::Raw(format!(
            "panic!(\"einsum({subscripts:?}) codegen not yet implemented\")"
        ))),
        EmirOp::Factorial(n) => {
            Ok(Expr::Raw(format!(
                "(1..=__e{} as i64).fold(1i64, |a, b| a * b)",
                n.0
            )))
        }
        EmirOp::ModInv(a, m) => {
            Ok(Expr::Raw(format!(
                "emath_runtime::mod_inv(__e{} as i64, __e{} as i64)",
                a.0, m.0
            )))
        }
        EmirOp::Congruence(a, b, m) => {
            Ok(Expr::Raw(format!(
                "(((__e{} as i64) - (__e{} as i64)).rem_euclid(__e{} as i64) == 0)",
                a.0, b.0, m.0
            )))
        }
        EmirOp::PolyEvalMod(coeffs, x, p) => {
            Ok(Expr::Raw(format!(
                "emath_runtime::poly_eval_mod(&__e{}, __e{} as i64, __e{} as i64)",
                coeffs.0, x.0, p.0
            )))
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            Ok(Expr::Raw(format!(
                "emath_runtime::rs_encode(&__e{}, __e{} as i64, __e{} as i64)",
                coeffs.0, n.0, p.0
            )))
        }
        EmirOp::HammingDistance(a, b) => {
            Ok(Expr::Raw(format!(
                "emath_runtime::hamming_distance(&__e{}, &__e{})",
                a.0, b.0
            )))
        }
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            body,
        } => {
            let mut body_names = names.to_vec();
            let lv_idx = *loop_var_index as usize;
            while body_names.len() <= lv_idx {
                body_names.push(String::new());
            }
            body_names[lv_idx] = "__fold_var".to_string();
            let body_expr = value_expr(body, &body_names, states)?;
            let body_code = render_expr(&body_expr);
            let (init_str, acc_op) = match combine {
                FoldCombine::Add => (render_expr(&operand(program, *init)), "+"),
                FoldCombine::Mul => (render_expr(&operand(program, *init)), "*"),
                FoldCombine::And => ("true".to_string(), "&&"),
                FoldCombine::Or => ("false".to_string(), "||"),
            };
            Ok(Expr::Raw(format!(
                "{{ let mut __fold_acc = {}; for __fold_iter in ({} as i64)..({} as i64) {{ let __fold_var = __fold_iter as f64; __fold_acc = __fold_acc {} {}; }} __fold_acc }}",
                init_str,
                render_expr(&operand(program, *start)),
                render_expr(&operand(program, *end)),
                acc_op,
                body_code,
            )))
        }
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            integrand,
        } => {
            let mut body_names = names.to_vec();
            let lv_idx = *loop_var_index as usize;
            while body_names.len() <= lv_idx {
                body_names.push(String::new());
            }
            body_names[lv_idx] = "__int_var".to_string();
            let body_expr = value_expr(integrand, &body_names, states)?;
            let body_code = render_expr(&body_expr);
            Ok(Expr::Raw(format!(
                "{{ let __a = {}; let __b = {}; let __n = {} as i64; let __h = (__b - __a) / __n as f64; let mut __int_acc = 0.0; for __i in 0..=__n {{ let __int_var = __a + __i as f64 * __h; let __w = if __i == 0 || __i == __n {{ 1.0 }} else if __i % 2 == 0 {{ 2.0 }} else {{ 4.0 }}; __int_acc += __w * {}; }} __int_acc * __h / 3.0 }}",
                render_expr(&operand(program, *start)),
                render_expr(&operand(program, *end)),
                steps,
                body_code,
            )))
        }
        EmirOp::Differentiate { body, var_index } => {
            let mut statements = Vec::new();
            for (index, (op, _)) in body.ops.iter().enumerate() {
                let primal = op_expr(op, body, names, states)?;
                statements.push(Stmt::Let {
                    pattern: format!("__e{index}"),
                    value: Box::new(primal),
                });
                let tangent = Expr::Raw(dual_tangent_str(op, *var_index, index));
                statements.push(Stmt::Let {
                    pattern: format!("__d{index}"),
                    value: Box::new(tangent),
                });
            }
            statements.push(Stmt::Expr(Expr::Var(format!(
                "__d{}",
                body.result.0
            ))));
            Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
        }
        EmirOp::Solve {
            body,
            var_index,
            tolerance,
            max_iter,
        } => {
            // Newton's method: x_new = x_old - f(x) / f'(x)
            // Generate primal (__e{N}) and tangent (__d{N}) let bindings
            // inside a for loop, using __x for the variable input.
            let init = op_expr(&EmirOp::LoadInput(*var_index), program, names, states)?;
            let mut solve_names = names.to_vec();
            let vi = *var_index as usize;
            while solve_names.len() <= vi {
                solve_names.push(String::new());
            }
            solve_names[vi] = "__x".to_string();
            let mut inner = String::new();
            for (index, (op, _)) in body.ops.iter().enumerate() {
                let primal = op_expr(op, body, &solve_names, states)?;
                inner.push_str(&format!("let __e{index} = {};\n", render_expr(&primal)));
                let tangent = dual_tangent_str(op, *var_index, index);
                inner.push_str(&format!("let __d{index} = {tangent};\n"));
            }
            let result_idx = body.result.0;
            // Match interpreter: vanish/exhaustion panic; final Newton
            // update is re-checked so a last-step root still succeeds.
            Ok(Expr::Raw(format!(
                "{{ let mut __x = {};\nlet mut __converged = false;\n\
                 for _ in 0..{max_iter} {{\n{inner}\
                 let __f = __e{result_idx};\nlet __df = __d{result_idx};\n\
                 if __f.abs() < {tolerance} {{ __converged = true; break; }}\n\
                 if __df.abs() < 1e-30 {{ panic!(\"solve derivative vanished before convergence\"); }}\n\
                 __x -= __f / __df;\n}}\n\
                 if !__converged {{\n{inner}\
                 if __e{result_idx}.abs() < {tolerance} {{ __converged = true; }}\n}}\n\
                 if !__converged {{ panic!(\"solve did not converge within max_iter\"); }}\n\
                 __x }}",
                render_expr(&init),
            )))
        }
        EmirOp::Optimize {
            body,
            var_indices,
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        } => {
            // Multi-variable gradient descent (or ascent).
            // One primal/tangent pass per variable gives each partial.
            // Match interpreter: refuse max_iter exhaustion (panic — evaluate is f64).
            let sign = if *maximize { "" } else { "-" };
            let mut block = String::from("{ let mut __converged = false;\n");
            // Initialize __x{i} for each variable.
            for (i, vi) in var_indices.iter().enumerate() {
                let init = op_expr(&EmirOp::LoadInput(*vi), program, names, states)?;
                block.push_str(&format!("let mut __x{i} = {};\n", render_expr(&init)));
            }
            // Shared dual-number body used in-loop and for the final check.
            let mut grad_body = String::new();
            let mut grads = Vec::new();
            for (i, vi) in var_indices.iter().enumerate() {
                let mut opt_names = names.to_vec();
                let viu = *vi as usize;
                while opt_names.len() <= viu {
                    opt_names.push(String::new());
                }
                opt_names[viu] = format!("__x{i}");
                for (index, (op, _)) in body.ops.iter().enumerate() {
                    let primal = op_expr(op, body, &opt_names, states)?;
                    grad_body.push_str(&format!(
                        "let __e_{i}_{index} = {};\n",
                        render_expr(&primal)
                    ));
                    let tangent = dual_tangent_str_multi(op, *vi, i, index);
                    grad_body.push_str(&format!("let __d_{i}_{index} = {tangent};\n"));
                }
                let result_idx = body.result.0;
                grads.push(format!("__d_{i}_{result_idx}"));
            }
            let max_grad = grads
                .iter()
                .map(|g| format!("{g}.abs()"))
                .collect::<Vec<_>>()
                .join(".max(");
            let max_grad_expr = if grads.len() == 1 {
                format!("{}.abs()", grads[0])
            } else {
                format!("{max_grad})")
            };
            block.push_str(&format!("for _ in 0..{max_iter} {{\n"));
            block.push_str(&grad_body);
            block.push_str(&format!(
                "if {max_grad_expr} < {tolerance} {{ __converged = true; break; }}\n"
            ));
            for (i, g) in grads.iter().enumerate() {
                block.push_str(&format!("__x{i} += {sign} {learning_rate} * {g};\n"));
            }
            block.push_str("}\n");
            // Final stationarity check after the last gradient step.
            block.push_str("if !__converged {\n");
            block.push_str(&grad_body);
            block.push_str(&format!(
                "if {max_grad_expr} < {tolerance} {{ __converged = true; }}\n}}\n"
            ));
            block.push_str(
                "if !__converged { panic!(\"optimize did not converge within max_iter\"); }\n",
            );
            block.push_str("__x0 }");
            Ok(Expr::Raw(block))
        }
    }
}

/// Generate the tangent expression string for an EMIR op in forward-mode
/// autodiff.  Uses `__e{N}` for primal references and `__d{N}` for tangent
/// references of earlier registers.  `idx` is the current register index.
pub(crate) fn dual_tangent_str(op: &EmirOp, var_index: u16, idx: usize) -> String {
    match op {
        EmirOp::ConstF64(_) => "0.0".to_string(),
        EmirOp::ConstI64(_) => "0.0".to_string(),
        EmirOp::LoadInput(i) => {
            if *i == var_index {
                "1.0".to_string()
            } else {
                "0.0".to_string()
            }
        }
        EmirOp::LoadState(_) => "0.0".to_string(),
        EmirOp::F64Add(a, b) => format!("__d{} + __d{}", a.0, b.0),
        EmirOp::F64Sub(a, b) => format!("__d{} - __d{}", a.0, b.0),
        EmirOp::F64Mul(a, b) => {
            format!("__d{} * __e{} + __e{} * __d{}", a.0, b.0, a.0, b.0)
        }
        EmirOp::F64Div(a, b) => format!(
            "(__d{} * __e{} - __e{} * __d{}) / (__e{} * __e{})",
            a.0, b.0, a.0, b.0, b.0, b.0
        ),
        EmirOp::Neg(a) => format!("-__d{}", a.0),
        EmirOp::Exp(a) => format!("__e{} * __d{}", idx, a.0),
        EmirOp::Ln(a) => format!("__d{} / __e{}", a.0, a.0),
        EmirOp::Sqrt(a) => format!("__d{} / (2.0 * __e{})", a.0, idx),
        EmirOp::Sin(a) => format!("__e{}.cos() * __d{}", a.0, a.0),
        EmirOp::Cos(a) => format!("-__e{}.sin() * __d{}", a.0, a.0),
        EmirOp::Tan(a) => format!(
            "__d{} / (__e{}.cos() * __e{}.cos())",
            a.0, a.0, a.0
        ),
        EmirOp::Tanh(a) => format!("(1.0 - __e{} * __e{}) * __d{}", idx, idx, a.0),
        EmirOp::Abs(a) => format!("__e{}.signum() * __d{}", a.0, a.0),
        EmirOp::Floor(_) | EmirOp::Ceil(_) | EmirOp::Round(_) | EmirOp::Sign(_) => "0.0".to_string(),
        EmirOp::Log2(a) => format!("__d{} / (__e{} * std::f64::consts::LN_2)", a.0, a.0),
        EmirOp::Log10(a) => format!("__d{} / (__e{} * std::f64::consts::LN_10)", a.0, a.0),
        EmirOp::Sinh(a) => format!("__e{}.cosh() * __d{}", a.0, a.0),
        EmirOp::Cosh(a) => format!("__e{}.sinh() * __d{}", a.0, a.0),
        EmirOp::Atan(a) => format!("__d{} / (1.0 + __e{} * __e{})", a.0, a.0, a.0),
        EmirOp::Cbrt(a) => {
            let idx_s = idx.to_string();
            format!("__d{} / (3.0 * __e{} * __e{})", a.0, idx_s, idx_s)
        }
        EmirOp::Recip(a) => format!("-__d{} / (__e{} * __e{})", a.0, a.0, a.0),
        EmirOp::Fract(a) => format!("__d{}", a.0),
        EmirOp::Hypot(a, b) => {
            let idx_s = idx.to_string();
            format!(
                "if __e{idx_s} == 0.0 {{ 0.0 }} else {{ (__e{} * __d{} + __e{} * __d{}) / __e{idx_s} }}",
                a.0, a.0, b.0, b.0
            )
        }
        // Match interpreter: constant-exponent form when db==0 (avoids ln
        // for a<=0); otherwise general a^b * (b*a'/a + b'*ln(a)).
        EmirOp::F64Pow(a, b) => format!(
            "if __d{} == 0.0 {{ __e{} * __e{}.powf(__e{} - 1.0) * __d{} }} else {{ __e{} * (__e{} * __d{} / __e{} + __d{} * __e{}.ln()) }}",
            b.0, b.0, a.0, b.0, a.0, idx, b.0, a.0, a.0, b.0, a.0
        ),
        EmirOp::Min(a, b) => format!(
            "if __e{} < __e{} {{ __d{} }} else {{ __d{} }}",
            a.0, b.0, a.0, b.0
        ),
        EmirOp::Max(a, b) => format!(
            "if __e{} > __e{} {{ __d{} }} else {{ __d{} }}",
            a.0, b.0, a.0, b.0
        ),
        EmirOp::Atan2(a, b) => format!(
            "(__e{} * __d{} - __e{} * __d{}) / (__e{} * __e{} + __e{} * __e{})",
            b.0, a.0, a.0, b.0, a.0, a.0, b.0, b.0
        ),
        EmirOp::Mod(a, _) => format!("__d{}", a.0),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: e,
        } => format!("if __e{} != 0.0 {{ __d{} }} else {{ __d{} }}", c.0, t.0, e.0),
        EmirOp::IsFinite(_) => "0.0".to_string(),
        EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Not(..) => "0.0".to_string(),
        _ => "0.0".to_string(),
    }
}

/// Like `dual_tangent_str` but uses `__e_{pass}_{N}` and `__d_{pass}_{N}`
/// naming so multiple evaluation passes (one per variable) can coexist
/// in the same scope without name collisions.
pub(crate) fn dual_tangent_str_multi(op: &EmirOp, var_index: u16, pass: usize, idx: usize) -> String {
    let e = |n: u32| format!("__e_{pass}_{n}");
    let d = |n: u32| format!("__d_{pass}_{n}");
    match op {
        EmirOp::ConstF64(_) => "0.0".to_string(),
        EmirOp::ConstI64(_) => "0.0".to_string(),
        EmirOp::LoadInput(i) => {
            if *i == var_index {
                "1.0".to_string()
            } else {
                "0.0".to_string()
            }
        }
        EmirOp::LoadState(_) => "0.0".to_string(),
        EmirOp::F64Add(a, b) => format!("{} + {}", d(a.0), d(b.0)),
        EmirOp::F64Sub(a, b) => format!("{} - {}", d(a.0), d(b.0)),
        EmirOp::F64Mul(a, b) => {
            format!("{} * {} + {} * {}", d(a.0), e(b.0), e(a.0), d(b.0))
        }
        EmirOp::F64Div(a, b) => format!(
            "({} * {} - {} * {}) / ({} * {})",
            d(a.0), e(b.0), e(a.0), d(b.0), e(b.0), e(b.0)
        ),
        EmirOp::Neg(a) => format!("-{}", d(a.0)),
        EmirOp::Exp(a) => format!("{} * {}", e(idx as u32), d(a.0)),
        EmirOp::Ln(a) => format!("{} / {}", d(a.0), e(a.0)),
        EmirOp::Sqrt(a) => format!("{} / (2.0 * {})", d(a.0), e(idx as u32)),
        EmirOp::Sin(a) => format!("{}.cos() * {}", e(a.0), d(a.0)),
        EmirOp::Cos(a) => format!("-{}.sin() * {}", e(a.0), d(a.0)),
        EmirOp::Tan(a) => format!("{} / ({}.cos() * {}.cos())", d(a.0), e(a.0), e(a.0)),
        EmirOp::Tanh(a) => format!("(1.0 - {} * {}) * {}", e(idx as u32), e(idx as u32), d(a.0)),
        EmirOp::Abs(a) => format!("{}.signum() * {}", e(a.0), d(a.0)),
        EmirOp::Floor(_) | EmirOp::Ceil(_) | EmirOp::Round(_) | EmirOp::Sign(_) => "0.0".to_string(),
        EmirOp::Log2(a) => format!("{} / ({} * std::f64::consts::LN_2)", d(a.0), e(a.0)),
        EmirOp::Log10(a) => format!("{} / ({} * std::f64::consts::LN_10)", d(a.0), e(a.0)),
        EmirOp::Sinh(a) => format!("{}.cosh() * {}", e(a.0), d(a.0)),
        EmirOp::Cosh(a) => format!("{}.sinh() * {}", e(a.0), d(a.0)),
        EmirOp::Atan(a) => format!("{} / (1.0 + {} * {})", d(a.0), e(a.0), e(a.0)),
        EmirOp::Cbrt(a) => {
            format!("{} / (3.0 * {} * {})", d(a.0), e(idx as u32), e(idx as u32))
        }
        EmirOp::Recip(a) => format!("-{} / ({} * {})", d(a.0), e(a.0), e(a.0)),
        EmirOp::Fract(a) => format!("{}", d(a.0)),
        EmirOp::Hypot(a, b) => {
            let h = e(idx as u32);
            format!(
                "if {h} == 0.0 {{ 0.0 }} else {{ ({} * {} + {} * {}) / {h} }}",
                e(a.0),
                d(a.0),
                e(b.0),
                d(b.0)
            )
        }
        EmirOp::F64Pow(a, b) => format!(
            "if {} == 0.0 {{ {} * {}.powf({} - 1.0) * {} }} else {{ {} * ({} * {} / {} + {} * {}.ln()) }}",
            d(b.0),
            e(b.0),
            e(a.0),
            e(b.0),
            d(a.0),
            e(idx as u32),
            e(b.0),
            d(a.0),
            e(a.0),
            d(b.0),
            e(a.0)
        ),
        EmirOp::Min(a, b) => format!("if {} < {} {{ {} }} else {{ {} }}", e(a.0), e(b.0), d(a.0), d(b.0)),
        EmirOp::Max(a, b) => format!("if {} > {} {{ {} }} else {{ {} }}", e(a.0), e(b.0), d(a.0), d(b.0)),
        EmirOp::Atan2(a, b) => format!(
            "({} * {} - {} * {}) / ({} * {} + {} * {})",
            e(b.0), d(a.0), e(a.0), d(b.0), e(a.0), e(a.0), e(b.0), e(b.0)
        ),
        EmirOp::Mod(a, _) => format!("{}", d(a.0)),
        EmirOp::Select { condition: c, then_value: t, else_value: ev } => {
            format!("if {} != 0.0 {{ {} }} else {{ {} }}", e(c.0), d(t.0), d(ev.0))
        }
        EmirOp::IsFinite(_) => "0.0".to_string(),
        EmirOp::Eq(..) | EmirOp::Ne(..) | EmirOp::Lt(..) | EmirOp::Le(..)
        | EmirOp::Gt(..) | EmirOp::Ge(..) | EmirOp::And(..) | EmirOp::Or(..)
        | EmirOp::Imply(..) | EmirOp::Iff(..) | EmirOp::Not(..) => "0.0".to_string(),
        _ => "0.0".to_string(),
    }
}
