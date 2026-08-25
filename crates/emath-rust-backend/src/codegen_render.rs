use std::collections::HashMap;

use emath_exec_ir::optimize::is_total;
use emath_exec_ir::{EdgePolicy, EmirOp, EmirProgram, EmirValue, FoldCombine};
use emath_rust_ir::ast::{escape_ident, BinOp, Block, Expr, Stmt, Ty, UnOp};
use emath_rust_ir::render::render_expr;

use crate::BackendError;
use crate::codegen_helpers::comparison;

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
    let flat = flat_ssa(program, names, states, None)?;
    let mut statements: Vec<Stmt> = Vec::with_capacity(flat.e_lets.len() + 1);
    for (pattern, src) in flat.e_lets {
        statements.push(Stmt::Let {
            pattern,
            value: Box::new(Expr::Raw(src)),
        });
    }
    statements.push(Stmt::Expr(Expr::Raw(flat.e_tail)));
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}

/// Register-inlined SSA body renderer: single-use, provably-total
/// registers inline into their consumer; multi-use/fault-capable ops stay
/// bound as lets, preserving strict eager fault timing. `var_index` also
/// renders the tangent (`__d`) space for the AD torsos.
pub(crate) struct FlatSsa {
    /// `let __eN = <src>;` lines for registers that must stay bound, in
    /// register order.
    pub e_lets: Vec<(String, String)>,
    /// `let __dN = <src>;` tangent lines (same rule, tangent space).
    pub d_lets: Vec<(String, String)>,
    /// Fully resolved primal source of the result register.
    pub e_tail: String,
    /// Fully resolved tangent source of the result register (empty when
    /// `var_index` was `None`).
    pub d_tail: String,
}

/// Scratch state for one body's flattening; resolves a register to fully
/// inlined source on demand, memoized.
struct Resolver<'a> {
    program: &'a EmirProgram,
    e_src: &'a [String],
    d_src: &'a [String],
    inline_e: &'a [bool],
    inline_d: &'a [bool],
    e_memo: HashMap<u32, String>,
    d_memo: HashMap<u32, String>,
}

impl Resolver<'_> {
    fn e(&mut self, i: u32) -> Result<String, BackendError> {
        if let Some(s) = self.e_memo.get(&i) {
            return Ok(s.clone());
        }
        let src = self
            .e_src
            .get(i as usize)
            .ok_or_else(|| BackendError::Lowering("flat e-register out of range".into()))?
            .clone();
        let out = self.substitute(&src)?;
        self.e_memo.insert(i, out.clone());
        Ok(out)
    }

    fn d(&mut self, i: u32) -> Result<String, BackendError> {
        if let Some(s) = self.d_memo.get(&i) {
            return Ok(s.clone());
        }
        let src = self
            .d_src
            .get(i as usize)
            .ok_or_else(|| BackendError::Lowering("flat d-register out of range".into()))?
            .clone();
        let out = self.substitute(&src)?;
        self.d_memo.insert(i, out.clone());
        Ok(out)
    }

    /// Expand `__e{N}`/`__d{N}` tokens for inlined registers to their
    /// (parenthesized) defining expression; others keep their bound name.
    fn substitute(&mut self, src: &str) -> Result<String, BackendError> {
        let mut out = String::with_capacity(src.len());
        let mut i = 0usize;
        while i < src.len() {
            let token_len = if src[i..].starts_with("__e") || src[i..].starts_with("__d") {
                let (kind, start) = if src[i..].starts_with("__e") {
                    ('e', i + 3)
                } else {
                    ('d', i + 3)
                };
                let digits_len = src[start..]
                    .bytes()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                if digits_len > 0 {
                    if let Ok(idx) = src[start..(start + digits_len)].parse::<u32>() {
                        if (idx as usize) < self.program.ops.len() {
                            let replacement =
                                match (kind, self.inline_e.get(idx as usize), self.inline_d.get(idx as usize)) {
                                    ('e', Some(true), _) => format!("({})", self.e(idx)?),
                                    ('d', _, Some(true)) => format!("({})", self.d(idx)?),
                                    _ => src[i..(start + digits_len)].to_string(),
                                };
                            out.push_str(&replacement);
                            start + digits_len - i
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0
            };
            if token_len > 0 {
                i += token_len;
                continue;
            }
            let ch = src[i..]
                .chars()
                .next()
                .expect("index i always lands on a char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
        Ok(out)
    }
}

/// Whether a user-facing name could collide with an internal `__e\d+`
/// register token (the flattening scanner rewrites those tokens in the
/// rendered source). Such programs fall back to non-flat rendering.
fn reg_token_collision(names: &[String], states: &[String]) -> bool {
    let is_like = |name: &str| {
        let name = name.strip_prefix("__e").or_else(|| name.strip_prefix("__d"));
        matches!(name, Some(rest) if rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    };
    names.iter().any(|n| is_like(n)) || states.iter().any(|n| is_like(n))
}

/// Count how many times each register token appears across the rendered
/// sources.
fn count_reg_tokens(srcs: &[String], kind: char) -> Vec<u32> {
    let mut uses = vec![0u32; srcs.len()];
    for src in srcs {
        let bytes = src.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let prefix = if kind == 'e' {
                src[i..].starts_with("__e")
            } else {
                src[i..].starts_with("__d")
            };
            if prefix {
                let start = i + 3;
                let digits_len = src[start..]
                    .bytes()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                if digits_len > 0 {
                    if let Ok(idx) = src[start..(start + digits_len)].parse::<usize>() {
                        if idx < uses.len() {
                            uses[idx] += 1;
                        }
                    }
                    i = start + digits_len;
                    continue;
                }
            }
            i += src[i..]
                .chars()
                .next()
                .expect("index i always lands on a char boundary")
                .len_utf8();
        }
    }
    uses
}

/// Flatten an SSA body; see [`FlatSsa`].
pub(crate) fn flat_ssa(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    var_index: Option<u16>,
) -> Result<FlatSsa, BackendError> {
    let n = program.ops.len();
    // Primal sources for every register.
    let mut e_src = Vec::with_capacity(n);
    for (op, _) in &program.ops {
        e_src.push(render_expr(&op_expr(op, program, names, states)?));
    }
    let e_direct = count_reg_tokens(&e_src, 'e');
    let collision = reg_token_collision(names, states);
    let mut inline_e = vec![false; n];
    for i in 0..n {
        inline_e[i] = !collision && e_direct[i] <= 1 && is_total(&program.ops[i].0, program);
    }
    // Tangent sources (AD torso sites only).
    let mut d_src = Vec::new();
    let mut inline_d = vec![false; n];
    if let Some(vi) = var_index {
        d_src = (0..n)
            .map(|i| dual_tangent_str(&program.ops[i].0, vi, i))
            .collect();
        let d_direct = count_reg_tokens(&d_src, 'd');
        for i in 0..n {
            inline_d[i] = !collision && d_direct[i] <= 1;
        }
    }
    let mut resolver = Resolver {
        program,
        e_src: &e_src,
        d_src: &d_src,
        inline_e: &inline_e,
        inline_d: &inline_d,
        e_memo: HashMap::new(),
        d_memo: HashMap::new(),
    };
    let mut e_lets = Vec::new();
    let mut d_lets = Vec::new();
    let result = program.result;
    for i in 0..n {
        if i == result.0 as usize {
            continue;
        }
        if !inline_e[i] {
            e_lets.push((format!("__e{i}"), resolver.e(i as u32)?));
        }
        if var_index.is_some() && !inline_d[i] {
            d_lets.push((format!("__d{i}"), resolver.d(i as u32)?));
        }
    }
    let e_tail = resolver.e(result.0)?;
    let d_tail = if var_index.is_some() {
        resolver.d(result.0)?
    } else {
        String::new()
    };
    // e_lets must precede d_lets, then the inferred result bindings follow
    // register order within each space; fault order is unchanged because
    // d-sources never fault and e-lets keep relative order.
    Ok(FlatSsa {
        e_lets,
        d_lets,
        e_tail,
        d_tail,
    })
}

/// Operand reference: every op is materialized as `__e<i>`.
pub(crate) fn operand(_program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Var(format!("__e{}", value.0))
}

/// A call into the embedded runtime module: `emath_rt::<name>(args)`.
fn rt_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        path: vec!["emath_rt".to_string(), name.to_string()],
        args,
    }
}

/// Reference to a register operand: `&__e<i>` (runtime kernels take
/// slices).
fn operand_ref(program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Raw(format!("&{}", render_expr(&operand(program, value))))
}

/// Render an `EdgePolicy` literal as a constructed enum value in the
/// embedded runtime module.
fn edge_policy_literal(edge: &EdgePolicy) -> Expr {
    let text = match *edge {
        EdgePolicy::Clamp => "emath_rt::EdgePolicy::Clamp".to_string(),
        EdgePolicy::Neumann => "emath_rt::EdgePolicy::Neumann".to_string(),
        EdgePolicy::Dirichlet { left, right } => format!(
            "emath_rt::EdgePolicy::Dirichlet {{ left: {left:?}, right: {right:?} }}"
        ),
    };
    Expr::Raw(text)
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
        EmirOp::ConstBool(value) => Ok(Expr::Bool(*value)),
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
        EmirOp::UnaryBuiltin(id, value) => {
            let arg = render_expr(&operand(program, *value));
            Ok(Expr::Raw(id.rust_unary(&arg)))
        }
        EmirOp::BinaryBuiltin(id, l, r) => {
            let arg_l = render_expr(&operand(program, *l));
            let arg_r = render_expr(&operand(program, *r));
            Ok(Expr::Raw(id.rust_binary(&arg_l, &arg_r)))
        }
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
        EmirOp::VectorAdd(l, r) => Ok(rt_call(
            "vec_add",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::VectorSub(l, r) => Ok(rt_call(
            "vec_sub",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::VectorScale(l, r) => Ok(rt_call(
            "vec_scale",
            vec![
                operand_ref(program, *l),
                operand(program, *r),
            ],
        )),
        EmirOp::VectorDot(l, r) => Ok(rt_call(
            "vec_dot",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::VectorNorm(v) => Ok(rt_call("vec_norm", vec![operand_ref(program, *v)])),
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
            let w_lit = weights
                .iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(rt_call(
                "stencil_1d",
                vec![
                    operand_ref(program, *input),
                    Expr::Raw(format!("&[{w_lit}]")),
                    Expr::Raw(format!("{center}")),
                    edge_policy_literal(edge),
                ],
            ))
        }
        EmirOp::Stencil2d {
            input,
            weights,
            center,
            edge,
        } => {
            if matches!(edge, EdgePolicy::Dirichlet { .. }) {
                return Err(BackendError::Lowering(
                    "2D Dirichlet boundary is not yet supported for Stencil2d".to_string(),
                ));
            }
            let w_lit = weights
                .iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(rt_call(
                "stencil_2d",
                vec![
                    operand_ref(program, *input),
                    Expr::Raw(format!("&[{w_lit}]")),
                    Expr::Raw(format!("({}, {})", center.0, center.1)),
                    edge_policy_literal(edge),
                ],
            ))
        }
        EmirOp::MatrixAdd(l, r) => Ok(rt_call(
            "mat_add",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::MatrixSub(l, r) => Ok(rt_call(
            "mat_sub",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::MatrixScale(l, r) => Ok(rt_call(
            "mat_scale",
            vec![operand_ref(program, *l), operand(program, *r)],
        )),
        EmirOp::MatrixMulVector(m, v) => Ok(rt_call(
            "mat_mul_vec",
            vec![operand_ref(program, *m), operand_ref(program, *v)],
        )),
        EmirOp::MatrixMulMatrix(l, r) => Ok(rt_call(
            "mat_mul_mat",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::MatrixTranspose(m) => Ok(rt_call("mat_transpose", vec![operand_ref(program, *m)])),
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
        EmirOp::TensorAdd(l, r) => Ok(rt_call(
            "tensor_add",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        EmirOp::TensorSub(l, r) => Ok(rt_call(
            "tensor_sub",
            vec![operand_ref(program, *l), operand_ref(program, *r)],
        )),
        // einsum codegen: the interp handles evaluation; Rust codegen
        // emits a placeholder that panics at runtime. Full tensor
        // codegen is a Stage 2 concern.
        EmirOp::Einsum { subscripts, .. } => Ok(Expr::Raw(format!(
            "panic!(\"einsum({subscripts:?}) codegen not yet implemented\")"
        ))),
        EmirOp::Factorial(n) => {
            Ok(Expr::Raw(format!(
                "(emath_rt::factorial(__e{} as i64)) as f64",
                n.0
            )))
        }
        EmirOp::ModInv(a, m) => {
            Ok(Expr::Raw(format!(
                "(emath_rt::mod_inv(__e{} as i64, __e{} as i64)) as f64",
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
                "(emath_rt::poly_eval_mod(&__e{}, __e{} as i64, __e{} as i64)) as f64",
                coeffs.0, x.0, p.0
            )))
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            Ok(Expr::Raw(format!(
                "emath_rt::rs_encode(&__e{}, __e{} as i64, __e{} as i64)",
                coeffs.0, n.0, p.0
            )))
        }
        EmirOp::HammingDistance(a, b) => {
            Ok(Expr::Raw(format!(
                "(emath_rt::hamming_distance(&__e{}, &__e{})) as f64",
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
            let (fn_name, init_arg) = match combine {
                FoldCombine::Add => ("fold_add", render_expr(&operand(program, *init))),
                FoldCombine::Mul => ("fold_mul", render_expr(&operand(program, *init))),
                FoldCombine::And => ("fold_all", "true".to_string()),
                FoldCombine::Or => ("fold_any", "false".to_string()),
            };
            Ok(rt_call(
                fn_name,
                vec![
                    Expr::Raw(format!("&|__fold_var: f64| {body_code}")),
                    Expr::Raw(format!(
                        "({} as i64)",
                        render_expr(&operand(program, *start))
                    )),
                    Expr::Raw(format!("({} as i64)", render_expr(&operand(program, *end)))),
                    Expr::Raw(init_arg),
                ],
            ))
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
            Ok(rt_call(
                "simpson",
                vec![
                    Expr::Raw(format!("&|__int_var: f64| {body_code}")),
                    operand(program, *start),
                    operand(program, *end),
                    Expr::Raw(format!("{steps} as i64")),
                ],
            ))
        }
        EmirOp::Differentiate { body, var_index } => {
            let flat = flat_ssa(body, names, states, Some(*var_index))?;
            let mut statements = Vec::with_capacity(flat.e_lets.len() + flat.d_lets.len() + 1);
            for (pattern, src) in flat.e_lets {
                statements.push(Stmt::Let {
                    pattern,
                    value: Box::new(Expr::Raw(src)),
                });
            }
            for (pattern, src) in flat.d_lets {
                statements.push(Stmt::Let {
                    pattern,
                    value: Box::new(Expr::Raw(src)),
                });
            }
            statements.push(Stmt::Expr(Expr::Raw(flat.d_tail)));
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
            let flat = flat_ssa(body, &solve_names, states, Some(*var_index))?;
            let mut inner = String::new();
            for (pattern, src) in flat.e_lets {
                inner.push_str(&format!("let {pattern} = {src};\n"));
            }
            for (pattern, src) in flat.d_lets {
                inner.push_str(&format!("let {pattern} = {src};\n"));
            }
            let e_result = flat.e_tail;
            let d_result = flat.d_tail;
            // Match interpreter: vanish/exhaustion panic; final Newton
            // update is re-checked so a last-step root still succeeds.
            Ok(Expr::Raw(format!(
                "{{ let mut __x = {};\nlet mut __converged = false;\n\
                 for _ in 0..{max_iter} {{\n{inner}\
                 let __f = {e_result};\nlet __df = {d_result};\n\
                 if __f.abs() < {tolerance} {{ __converged = true; break; }}\n\
                 if __df.abs() < 1e-30 {{ panic!(\"solve derivative vanished before convergence\"); }}\n\
                 __x -= __f / __df;\n}}\n\
                 if !__converged {{\n{inner}\
                 if ({e_result}).abs() < {tolerance} {{ __converged = true; }}\n}}\n\
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
            // Each pass runs in its own scope with base register naming
            // (`__e{N}`/`__d{N}`, what `op_expr` emits), so passes cannot
            // collide; the result tangent is captured into `__g{i}`.
            // Match interpreter: refuse max_iter exhaustion (panic — evaluate is f64).
            let sign = if *maximize { "" } else { "-" };
            let mut block = String::from("{ let mut __converged = false;\n");
            // Initialize __x{i} for each variable.
            for (i, vi) in var_indices.iter().enumerate() {
                let init = op_expr(&EmirOp::LoadInput(*vi), program, names, states)?;
                block.push_str(&format!("let mut __x{i} = {};\n", render_expr(&init)));
            }
            // One scoped primal/tangent pass per variable.
            let mut passes = Vec::new();
            let mut grads = Vec::new();
            for (i, vi) in var_indices.iter().enumerate() {
                let mut opt_names = names.to_vec();
                let viu = *vi as usize;
                while opt_names.len() <= viu {
                    opt_names.push(String::new());
                }
                opt_names[viu] = format!("__x{i}");
                let flat = flat_ssa(body, &opt_names, states, Some(*vi))?;
                let mut code = format!("let __g{i} = {{ ");
                for (pattern, src) in flat.e_lets {
                    code.push_str(&format!("let {pattern} = {src}; "));
                }
                for (pattern, src) in flat.d_lets {
                    code.push_str(&format!("let {pattern} = {src}; "));
                }
                code.push_str(&format!("{} }};\n", flat.d_tail));
                passes.push(code);
                grads.push(format!("__g{i}"));
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
            for pass in &passes {
                block.push_str(pass);
            }
            block.push_str(&format!(
                "if {max_grad_expr} < {tolerance} {{ __converged = true; break; }}\n"
            ));
            for (i, g) in grads.iter().enumerate() {
                block.push_str(&format!("__x{i} += {sign} {learning_rate} * {g};\n"));
            }
            block.push_str("}\n");
            // Final stationarity check after the last gradient step.
            block.push_str("if !__converged {\n");
            for pass in &passes {
                block.push_str(pass);
            }
            block.push_str(&format!(
                "if {max_grad_expr} < {tolerance} {{ __converged = true; }}\n}}\n"
            ));
            block.push_str(
                "if !__converged { panic!(\"optimize did not converge within max_iter\"); }\n",
            );
            block.push_str("__x0 }");
            Ok(Expr::Raw(block))
        }
        EmirOp::SampleLimit {
            body,
            var_index,
            target,
            direction,
        } => {
            // Numerical limit: sample body at target ± h for decreasing h.
            // The sampling driver lives in the embedded runtime
            // (`sample_limit`); the generated closure is the body with the
            // limit variable substituted.
            let vi = *var_index as usize;
            let mut lim_names = names.to_vec();
            while lim_names.len() <= vi {
                lim_names.push(String::new());
            }
            lim_names[vi] = "__lv".to_string();
            let body_expr = value_expr(body, &lim_names, states)?;
            let body_code = render_expr(&body_expr);
            Ok(rt_call(
                "sample_limit",
                vec![
                    Expr::Raw(format!("&|__lv: f64| {body_code}")),
                    operand(program, *target),
                    operand(program, *direction),
                ],
            ))
        }
        EmirOp::ReverseMode { body, var_indices } => {
            // Forward pass: compute all primals.
            let mut forward = String::new();
            for (index, (op, _)) in body.ops.iter().enumerate() {
                let e = op_expr(op, body, names, states)?;
                forward.push_str(&format!("let __re{index} = {};\n", render_expr(&e)));
            }
            // Initialize adjoints to 0.0.
            let n = body.ops.len();
            let mut init_adj = String::new();
            for index in 0..n {
                init_adj.push_str(&format!("let mut __ra{index}: f64 = 0.0;\n"));
            }
            // Seed the output adjoint.
            let result_idx = body.result.0;
            init_adj.push_str(&format!("__ra{result_idx} = 1.0;\n"));
            // Input adjoint accumulators.
            let input_count = body.input_count;
            let mut init_ia = String::new();
            for i in 0..input_count {
                init_ia.push_str(&format!("let mut __ria{i}: f64 = 0.0;\n"));
            }
            // Backward pass: traverse ops in reverse.
            let mut backward = String::new();
            for (index, (op, _)) in body.ops.iter().enumerate().rev() {
                backward.push_str(&reverse_adjoint_str(op, index));
            }
            // Collect requested gradients.
            let grads: Vec<String> = var_indices
                .iter()
                .map(|vi| format!("__ria{vi}"))
                .collect();
            Ok(Expr::Raw(format!(
                "{{\n\
                 {forward}\
                 {init_adj}\
                 {init_ia}\
                 {backward}\
                 vec![{}]\n\
                 }}",
                grads.join(", "),
            )))
        }
    }
}

/// Forward-mode tangent source for an EMIR op; `__e{N}`/`__d{N}` naming.
pub(crate) fn dual_tangent_str(op: &EmirOp, var_index: u16, idx: usize) -> String {
    tangent_str(op, var_index, idx, &|n| format!("__e{n}"), &|n| format!("__d{n}"))
}

/// Shared tangent generator: `e`/`d` map register → primal/tangent refs.
fn tangent_str(
    op: &EmirOp,
    var_index: u16,
    idx: usize,
    e: &dyn Fn(u32) -> String,
    d: &dyn Fn(u32) -> String,
) -> String {
    match op {
        EmirOp::ConstF64(_) => "0.0".to_string(),
        EmirOp::ConstI64(_) => "0.0".to_string(),
        EmirOp::ConstBool(_) => "0.0".to_string(),
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
        EmirOp::UnaryBuiltin(id, a) => id.rust_tangent_unary(e, d, idx as u32, a.0),
        // Match interpreter: constant-exponent form when db==0 (avoids ln
        // for a<=0); otherwise general a^b * (b*a'/a + b'*ln(a)).
        EmirOp::F64Pow(a, b) => format!(
            "if {} == 0.0 {{ {} * {}.powf({} - 1.0) * {} }} else {{ {} * ({} * {} / {} + {} * {}.ln()) }}",
            d(b.0), e(b.0), e(a.0), e(b.0), d(a.0), e(idx as u32), e(b.0), d(a.0), e(a.0), d(b.0), e(a.0)
        ),
        EmirOp::BinaryBuiltin(id, a, b) => id.rust_tangent_binary(e, d, idx as u32, a.0, b.0),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: ev,
        } => format!("if {} != 0.0 {{ {} }} else {{ {} }}", e(c.0), d(t.0), d(ev.0)),
        EmirOp::IsFinite(_) => "0.0".to_string(),
        EmirOp::Eq(..) | EmirOp::Ne(..) | EmirOp::Lt(..) | EmirOp::Le(..)
        | EmirOp::Gt(..) | EmirOp::Ge(..) | EmirOp::And(..) | EmirOp::Or(..)
        | EmirOp::Imply(..) | EmirOp::Iff(..) | EmirOp::Not(..) => "0.0".to_string(),
        _ => "0.0".to_string(),
    }
}

/// Backward-pass adjoint update statements for an EMIR op, accumulating
/// into `__ra{N}` operand adjoints and `__ria{N}` input adjoints.
pub(crate) fn reverse_adjoint_str(op: &EmirOp, idx: usize) -> String {
    let adj = format!("__ra{idx}");
    let p = |n: u32| format!("__re{n}");
    let a = |n: u32| format!("__ra{n}");
    let updates = match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => String::new(),
        EmirOp::LoadInput(i) => format!("__ria{i} += {adj};\n"),
        EmirOp::LoadState(_) => String::new(),
        EmirOp::F64Add(x, y) => format!("{} += {adj};\n{} += {adj};\n", a(x.0), a(y.0)),
        EmirOp::F64Sub(x, y) => format!("{} += {adj};\n{} -= {adj};\n", a(x.0), a(y.0)),
        EmirOp::F64Mul(x, y) => format!(
            "{} += {adj} * {};\n{} += {adj} * {};\n",
            a(x.0), p(y.0), a(y.0), p(x.0)
        ),
        EmirOp::F64Div(x, y) => format!(
            "{} += {adj} / {};\n{} -= {adj} * {} / ({} * {});\n",
            a(x.0), p(y.0), a(y.0), p(x.0), p(y.0), p(y.0)
        ),
        EmirOp::Neg(x) => format!("{} -= {adj};\n", a(x.0)),
        EmirOp::UnaryBuiltin(id, x) => {
            id.rust_adjoint_unary(&adj, &p, idx as u32, x.0).unwrap_or_default()
        }
        EmirOp::F64Pow(x, y) => format!(
            "if {} != 0.0 {{ {} += {adj} * __re{idx} * {} / {}; }}\n\
             if {} > 0.0 {{ {} += {adj} * __re{idx} * {}.ln(); }}\n",
            p(x.0), a(x.0), p(y.0), p(x.0),
            p(x.0), a(y.0), p(x.0)
        ),
        EmirOp::BinaryBuiltin(id, x, y) => {
            id.rust_adjoint_binary(&adj, &p, idx as u32, x.0, y.0).unwrap_or_default()
        }
        EmirOp::Select { condition: c, then_value: t, else_value: ev } => format!(
            "if {} != 0.0 {{ {} += {adj}; }} else {{ {} += {adj}; }}\n",
            p(c.0), a(t.0), a(ev.0)
        ),
        // Non-differentiable ops: no adjoint contribution.
        EmirOp::IsFinite(_) | EmirOp::Eq(..) | EmirOp::Ne(..)
        | EmirOp::Lt(..) | EmirOp::Le(..) | EmirOp::Gt(..) | EmirOp::Ge(..)
        | EmirOp::And(..) | EmirOp::Or(..) | EmirOp::Not(_)
        | EmirOp::Imply(..) | EmirOp::Iff(..) => String::new(),
        _ => String::new(),
    };
    if updates.is_empty() {
        String::new()
    } else {
        format!("if {adj} != 0.0 {{\n{updates}}}\n")
    }
}
