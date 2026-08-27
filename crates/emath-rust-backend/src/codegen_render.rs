use std::collections::{BTreeSet, HashMap};

use emath_exec_ir::optimize::{is_total, operand_registers};
use emath_exec_ir::{EdgePolicy, EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};
use emath_rust_ir::ast::{BinOp, Block, Expr, Stmt, Ty, UnOp, escape_ident};
use emath_rust_ir::render::render_expr;

use crate::BackendError;
use crate::codegen_helpers::comparison;

/// Scalar kind of an EMIR register in generated Rust. Mirrors interp:
/// I64×I64 add/sub/mul/neg and integer folds stay `i64`; everything else
/// that computes a number widens to `f64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScalarKind {
    I64,
    F64,
    Bool,
    Other,
}

pub(crate) fn program_kind(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> ScalarKind {
    if program.ops.is_empty() {
        return ScalarKind::Other;
    }
    let kinds = scalar_kinds(program, names, states, i64_names);
    kinds
        .get(program.result.0 as usize)
        .copied()
        .unwrap_or(ScalarKind::Other)
}

fn scalar_kinds(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Vec<ScalarKind> {
    let n = program.ops.len();
    let mut kinds = vec![ScalarKind::Other; n];
    for (i, (op, _)) in program.ops.iter().enumerate() {
        kinds[i] = kind_of_op(op, &kinds, names, states, i64_names);
    }
    kinds
}

fn kind_at(kinds: &[ScalarKind], value: EmirValue) -> ScalarKind {
    kinds
        .get(value.0 as usize)
        .copied()
        .unwrap_or(ScalarKind::Other)
}

fn kind_of_op(
    op: &EmirOp,
    kinds: &[ScalarKind],
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> ScalarKind {
    match op {
        EmirOp::ConstI64(_) => ScalarKind::I64,
        EmirOp::ConstF64(_) => ScalarKind::F64,
        EmirOp::ConstBool(_) => ScalarKind::Bool,
        EmirOp::LoadInput(index) => names
            .get(*index as usize)
            .map(|name| {
                if i64_names.contains(name) {
                    ScalarKind::I64
                } else {
                    ScalarKind::F64
                }
            })
            .unwrap_or(ScalarKind::F64),
        EmirOp::LoadState(index) => states
            .get(*index as usize)
            .map(|name| {
                if i64_names.contains(name) {
                    ScalarKind::I64
                } else {
                    ScalarKind::F64
                }
            })
            .unwrap_or(ScalarKind::F64),
        EmirOp::F64Add(l, r) | EmirOp::F64Sub(l, r) | EmirOp::F64Mul(l, r) => {
            match (kind_at(kinds, *l), kind_at(kinds, *r)) {
                (ScalarKind::I64, ScalarKind::I64) => ScalarKind::I64,
                _ => ScalarKind::F64,
            }
        }
        EmirOp::Neg(x) => kind_at(kinds, *x),
        EmirOp::Factorial(_) | EmirOp::ModInv(_, _) | EmirOp::PolyEvalMod(..) => ScalarKind::I64,
        EmirOp::HammingDistance(..) => ScalarKind::I64,
        EmirOp::Congruence(..)
        | EmirOp::IsFinite(_)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(_) => ScalarKind::Bool,
        EmirOp::Select {
            then_value,
            else_value,
            ..
        } => {
            let t = kind_at(kinds, *then_value);
            let e = kind_at(kinds, *else_value);
            if t == e { t } else { ScalarKind::F64 }
        }
        EmirOp::Fold {
            combine,
            init,
            loop_var_index,
            body,
            ..
        } => match combine {
            FoldCombine::And | FoldCombine::Or => ScalarKind::Bool,
            FoldCombine::Add | FoldCombine::Mul => {
                if fold_is_i64(
                    kinds,
                    *init,
                    *loop_var_index,
                    body,
                    names,
                    states,
                    i64_names,
                ) {
                    ScalarKind::I64
                } else {
                    ScalarKind::F64
                }
            }
        },
        _ => ScalarKind::F64,
    }
}

fn fold_is_i64(
    kinds: &[ScalarKind],
    init: EmirValue,
    loop_var_index: u16,
    body: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> bool {
    if kind_at(kinds, init) != ScalarKind::I64 {
        return false;
    }
    let mut body_names = names.to_vec();
    let lv = loop_var_index as usize;
    while body_names.len() <= lv {
        body_names.push(String::new());
    }
    body_names[lv] = "__fold_var".to_string();
    let mut body_i64 = i64_names.clone();
    body_i64.insert("__fold_var".to_string());
    program_kind(body, &body_names, states, &body_i64) == ScalarKind::I64
}

fn as_f64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as f64", render_expr(&expr)))
}

fn as_i64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as i64", render_expr(&expr)))
}

fn operand_kind<'a>(kinds: &'a [ScalarKind], value: EmirValue) -> ScalarKind {
    kind_at(kinds, value)
}

fn typed_operand(
    program: &EmirProgram,
    value: EmirValue,
    want: ScalarKind,
    kinds: &[ScalarKind],
) -> Expr {
    let expr = operand(program, value);
    match (operand_kind(kinds, value), want) {
        (ScalarKind::I64, ScalarKind::F64) => as_f64(expr),
        (ScalarKind::F64, ScalarKind::I64) => as_i64(expr),
        _ => expr,
    }
}

fn i64_checked_bin(method: &str, left: Expr, right: Expr) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(Expr::MethodCall {
            receiver: Box::new(left),
            method: method.to_string(),
            args: vec![right],
        }),
        method: "expect".to_string(),
        args: vec![Expr::Str("i64 overflow".to_string())],
    }
}

fn cmp_expr(
    op: BinOp,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ScalarKind],
) -> Expr {
    let lk = operand_kind(kinds, left);
    let rk = operand_kind(kinds, right);
    match (lk, rk) {
        (ScalarKind::I64, ScalarKind::I64) | (ScalarKind::Bool, ScalarKind::Bool) => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
        (ScalarKind::I64, ScalarKind::F64) => mixed_i64_f64_cmp(
            op,
            operand(program, left),
            typed_operand(program, right, ScalarKind::F64, kinds),
            true,
        ),
        (ScalarKind::F64, ScalarKind::I64) => mixed_i64_f64_cmp(
            op,
            operand(program, right),
            typed_operand(program, left, ScalarKind::F64, kinds),
            false,
        ),
        (ScalarKind::I64, _) => Expr::Bin {
            op,
            left: Box::new(as_f64(operand(program, left))),
            right: Box::new(typed_operand(program, right, ScalarKind::F64, kinds)),
        },
        (_, ScalarKind::I64) => Expr::Bin {
            op,
            left: Box::new(typed_operand(program, left, ScalarKind::F64, kinds)),
            right: Box::new(as_f64(operand(program, right))),
        },
        _ => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
    }
}

/// Mixed Int/Float64 compare must not widen through `as f64` (2^53 lie).
fn mixed_i64_f64_cmp(op: BinOp, int_expr: Expr, float_expr: Expr, int_on_left: bool) -> Expr {
    let eq = rt_call("eq_i64_f64", vec![int_expr.clone(), float_expr.clone()]);
    let cmp = render_expr(&rt_call("cmp_i64_f64", vec![int_expr, float_expr]));
    let (lt, gt) = if int_on_left {
        ("Less", "Greater")
    } else {
        ("Greater", "Less")
    };
    match op {
        BinOp::Eq => eq,
        BinOp::Ne => Expr::Un {
            op: UnOp::Not,
            value: Box::new(eq),
        },
        BinOp::Lt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{lt})")),
        BinOp::Gt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{gt})")),
        BinOp::Le => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{lt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Ge => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{gt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Rem
        | BinOp::Pow
        | BinOp::And
        | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
    }
}

fn i64_or_f64_bin(
    f64_op: BinOp,
    i64_method: &str,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ScalarKind],
) -> Expr {
    if operand_kind(kinds, left) == ScalarKind::I64 && operand_kind(kinds, right) == ScalarKind::I64
    {
        i64_checked_bin(i64_method, operand(program, left), operand(program, right))
    } else {
        Expr::Bin {
            op: f64_op,
            left: Box::new(typed_operand(program, left, ScalarKind::F64, kinds)),
            right: Box::new(typed_operand(program, right, ScalarKind::F64, kinds)),
        }
    }
}

pub(crate) fn coerce_to_ty(expr: Expr, from: ScalarKind, to: &Ty) -> Expr {
    match (from, to) {
        (ScalarKind::I64, Ty::F64) => as_f64(expr),
        (ScalarKind::F64, Ty::I64) => as_i64(expr),
        _ => expr,
    }
}

/// Render the program as an expression. Multi-op programs become a block
/// `{ let __e0 = ...; ...; __eN }`; single-op programs inline directly.
pub(crate) fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    if program.ops.len() == 1 {
        return op_expr(&program.ops[0].0, program, names, states, i64_names);
    }
    let flat = flat_ssa(program, names, states, i64_names, None)?;
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
                            let replacement = match (
                                kind,
                                self.inline_e.get(idx as usize),
                                self.inline_d.get(idx as usize),
                            ) {
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
        let name = name
            .strip_prefix("__e")
            .or_else(|| name.strip_prefix("__d"));
        matches!(name, Some(rest) if rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    };
    names.iter().any(|n| is_like(n)) || states.iter().any(|n| is_like(n))
}

/// SSA use count: each operand mention plus the program result. Token
/// scans of rendered source over-count (`sign` mentions its arg twice)
/// and under-count operands that do not appear as `__eN` (nested bodies),
/// which made multi-use registers look single-use.
fn count_ssa_uses(program: &EmirProgram) -> Vec<u32> {
    let n = program.ops.len();
    let mut uses = vec![0u32; n];
    let mut operands = Vec::new();
    for (op, _) in &program.ops {
        operands.clear();
        operand_registers(op, &mut operands);
        for v in &operands {
            if (v.0 as usize) < n {
                uses[v.0 as usize] += 1;
            }
        }
    }
    if (program.result.0 as usize) < n {
        uses[program.result.0 as usize] += 1;
    }
    uses
}

fn has_nested_body(op: &EmirOp) -> bool {
    matches!(
        op,
        EmirOp::Fold { .. }
            | EmirOp::Integral { .. }
            | EmirOp::Differentiate { .. }
            | EmirOp::Solve { .. }
            | EmirOp::Optimize { .. }
            | EmirOp::SampleLimit { .. }
            | EmirOp::ReverseMode { .. }
    )
}

/// Flatten an SSA body; see [`FlatSsa`].
pub(crate) fn flat_ssa(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
    var_index: Option<u16>,
) -> Result<FlatSsa, BackendError> {
    let n = program.ops.len();
    // Primal sources for every register.
    let mut e_src = Vec::with_capacity(n);
    for (op, _) in &program.ops {
        e_src.push(render_expr(&op_expr(
            op, program, names, states, i64_names,
        )?));
    }
    let e_direct = count_ssa_uses(program);
    let collision = reg_token_collision(names, states);
    // Nested bodies already flatten with the same `__eN` namespace. Outer
    // token substitution would rewrite those inner names as outer
    // registers, so inlining is disabled for any body that embeds one.
    let nested = program.ops.iter().any(|(op, _)| has_nested_body(op));
    let mut inline_e = vec![false; n];
    for i in 0..n {
        inline_e[i] =
            !collision && !nested && e_direct[i] <= 1 && is_total(&program.ops[i].0, program);
    }
    // Tangent sources (AD torso sites only).
    let mut d_src = Vec::new();
    let mut inline_d = vec![false; n];
    if let Some(vi) = var_index {
        d_src = (0..n)
            .map(|i| dual_tangent_str(&program.ops[i].0, vi, i))
            .collect();
        for i in 0..n {
            inline_d[i] = !collision && !nested && e_direct[i] <= 1;
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
        if !inline_e[i] {
            e_lets.push((format!("__e{i}"), resolver.e(i as u32)?));
        }
        if var_index.is_some() && !inline_d[i] {
            d_lets.push((format!("__d{i}"), resolver.d(i as u32)?));
        }
    }
    let result_idx = result.0 as usize;
    let e_tail = if result_idx < n && inline_e[result_idx] {
        resolver.e(result.0)?
    } else {
        format!("__e{}", result.0)
    };
    let d_tail = if var_index.is_some() {
        if result_idx < n && inline_d[result_idx] {
            resolver.d(result.0)?
        } else {
            format!("__d{}", result.0)
        }
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

/// True when evaluating this program can raise a typed index/slice fault.
pub(crate) fn program_may_index_fault(program: &EmirProgram) -> bool {
    program.ops.iter().any(|(op, _)| op_may_index_fault(op))
}

fn op_may_index_fault(op: &EmirOp) -> bool {
    match op {
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::Stencil3d { .. } => true,
        EmirOp::Fold { body, .. }
        | EmirOp::Differentiate { body, .. }
        | EmirOp::Solve { body, .. }
        | EmirOp::Optimize { body, .. }
        | EmirOp::SampleLimit { body, .. }
        | EmirOp::ReverseMode { body, .. } => program_may_index_fault(body),
        EmirOp::Integral { integrand, .. } => program_may_index_fault(integrand),
        _ => false,
    }
}

fn index_f64(program: &EmirProgram, value: EmirValue, kinds: &[ScalarKind]) -> String {
    render_expr(&typed_operand(program, value, ScalarKind::F64, kinds))
}

fn map_index_result(call: String) -> Expr {
    Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
}

fn render_slice_axis(program: &EmirProgram, axis: &EmirSliceAxis, kinds: &[ScalarKind]) -> String {
    match *axis {
        EmirSliceAxis::Point(v) => {
            format!(
                "emath_rt::SliceAxis::Point({})",
                index_f64(program, v, kinds)
            )
        }
        EmirSliceAxis::Range { start, end } => format!(
            "emath_rt::SliceAxis::Range {{ start: {}, end: {} }}",
            index_f64(program, start, kinds),
            index_f64(program, end, kinds)
        ),
    }
}

fn slice_helper(axes: &[EmirSliceAxis]) -> &'static str {
    match axes
        .iter()
        .filter(|axis| matches!(axis, EmirSliceAxis::Range { .. }))
        .count()
    {
        0 => "tensor_slice_as_scalar",
        1 => "tensor_slice_as_vector",
        2 => "tensor_slice_as_matrix",
        _ => "tensor_slice_as_tensor",
    }
}

fn tensor_index_call(
    program: &EmirProgram,
    tensor: EmirValue,
    indices: &[EmirValue],
    kinds: &[ScalarKind],
) -> Expr {
    let idx = indices
        .iter()
        .map(|i| index_f64(program, *i, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_index_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::tensor_index_checked(&__s, &__d, &[{idx}]) }}",
        render_expr(&operand(program, tensor)),
    ))
}

fn tensor_slice_call(
    program: &EmirProgram,
    tensor: EmirValue,
    axes: &[EmirSliceAxis],
    kinds: &[ScalarKind],
) -> Expr {
    let helper = slice_helper(axes);
    let axes_src = axes
        .iter()
        .map(|axis| render_slice_axis(program, axis, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_index_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::{helper}(&__s, &__d, &[{axes_src}]) }}",
        render_expr(&operand(program, tensor)),
    ))
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
        EdgePolicy::OneSided => "emath_rt::EdgePolicy::OneSided".to_string(),
        EdgePolicy::Dirichlet { left, right } => {
            format!("emath_rt::EdgePolicy::Dirichlet {{ left: {left:?}, right: {right:?} }}")
        }
    };
    Expr::Raw(text)
}

/// Gaussian elimination of `H d = g` for generated Newton optimize steps.
/// Reads `__h_{i}_{j}` and `__g{i}`; writes `__x{i} -= d`.
fn render_optimize_newton_step(n: usize, maximize: bool) -> String {
    let mut block = String::new();
    let rows: Vec<String> = (0..n)
        .map(|i| {
            let cols = (0..n)
                .map(|j| format!("__h_{i}_{j}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("vec![{cols}]")
        })
        .collect();
    block.push_str(&format!("let mut __a = vec![{}];\n", rows.join(", ")));
    let gs = (0..n)
        .map(|i| format!("__g{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    block.push_str(&format!("let mut __b = vec![{gs}];\n"));
    block.push_str(&format!(
        "for __col in 0..{n} {{\n\
         let mut __pivot = __col;\n\
         let mut __best = __a[__col][__col].abs();\n\
         for __row in (__col + 1)..{n} {{\n\
         let __cand = __a[__row][__col].abs();\n\
         if __cand > __best {{ __best = __cand; __pivot = __row; }}\n\
         }}\n\
         if __best < 1e-30_f64 {{ panic!(\"optimize hessian vanished before stationarity\"); }}\n\
         __a.swap(__col, __pivot);\n\
         __b.swap(__col, __pivot);\n\
         for __row in (__col + 1)..{n} {{\n\
         let __fac = __a[__row][__col] / __a[__col][__col];\n\
         if __fac == 0.0 {{ continue; }}\n\
         for __k in __col..{n} {{ __a[__row][__k] -= __fac * __a[__col][__k]; }}\n\
         __b[__row] -= __fac * __b[__col];\n\
         }}\n\
         }}\n\
         let mut __d = vec![0.0_f64; {n}];\n\
         for __row in (0..{n}).rev() {{\n\
         let mut __acc = __b[__row];\n\
         for __k in (__row + 1)..{n} {{ __acc -= __a[__row][__k] * __d[__k]; }}\n\
         __d[__row] = __acc / __a[__row][__row];\n\
         }}\n"
    ));
    let dot = (0..n)
        .map(|i| format!("__g{i} * __d[{i}]"))
        .collect::<Vec<_>>()
        .join(" + ");
    if maximize {
        block.push_str(&format!(
            "if ({dot}) >= 0.0 {{ panic!(\"optimize hessian has the wrong curvature for maximize\"); }}\n"
        ));
    } else {
        block.push_str(&format!(
            "if ({dot}) <= 0.0 {{ panic!(\"optimize hessian has the wrong curvature for minimize\"); }}\n"
        ));
    }
    for i in 0..n {
        block.push_str(&format!("__x{i} -= __d[{i}];\n"));
    }
    block
}

pub(crate) fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    let kinds = scalar_kinds(program, names, states, i64_names);
    match op {
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) => Ok(Expr::Int(*value)),
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
        EmirOp::F64Add(l, r) => Ok(i64_or_f64_bin(
            BinOp::Add,
            "checked_add",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Sub(l, r) => Ok(i64_or_f64_bin(
            BinOp::Sub,
            "checked_sub",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Mul(l, r) => Ok(i64_or_f64_bin(
            BinOp::Mul,
            "checked_mul",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Div(l, r) => Ok(Expr::Bin {
            op: BinOp::Div,
            left: Box::new(typed_operand(program, *l, ScalarKind::F64, &kinds)),
            right: Box::new(typed_operand(program, *r, ScalarKind::F64, &kinds)),
        }),
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(typed_operand(program, *l, ScalarKind::F64, &kinds)),
            right: Box::new(typed_operand(program, *r, ScalarKind::F64, &kinds)),
        }),
        EmirOp::Neg(value) => {
            if operand_kind(&kinds, *value) == ScalarKind::I64 {
                Ok(Expr::MethodCall {
                    receiver: Box::new(Expr::MethodCall {
                        receiver: Box::new(operand(program, *value)),
                        method: "checked_neg".to_string(),
                        args: Vec::new(),
                    }),
                    method: "expect".to_string(),
                    args: vec![Expr::Str("i64 overflow".to_string())],
                })
            } else {
                Ok(Expr::Un {
                    op: UnOp::Neg,
                    value: Box::new(typed_operand(program, *value, ScalarKind::F64, &kinds)),
                })
            }
        }
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::UnaryBuiltin(id, value) => {
            let arg = render_expr(&typed_operand(program, *value, ScalarKind::F64, &kinds));
            Ok(Expr::Raw(id.rust_unary(&arg)))
        }
        EmirOp::BinaryBuiltin(id, l, r) => {
            let arg_l = render_expr(&typed_operand(program, *l, ScalarKind::F64, &kinds));
            let arg_r = render_expr(&typed_operand(program, *r, ScalarKind::F64, &kinds));
            Ok(Expr::Raw(id.rust_binary(&arg_l, &arg_r)))
        }
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(typed_operand(program, *value, ScalarKind::F64, &kinds)),
            method: "is_finite".to_string(),
            args: Vec::new(),
        }),
        EmirOp::Lt(l, r) => Ok(cmp_expr(BinOp::Lt, program, *l, *r, &kinds)),
        EmirOp::Le(l, r) => Ok(cmp_expr(BinOp::Le, program, *l, *r, &kinds)),
        EmirOp::Gt(l, r) => Ok(cmp_expr(BinOp::Gt, program, *l, *r, &kinds)),
        EmirOp::Ge(l, r) => Ok(cmp_expr(BinOp::Ge, program, *l, *r, &kinds)),
        EmirOp::Eq(l, r) => Ok(cmp_expr(BinOp::Eq, program, *l, *r, &kinds)),
        EmirOp::Ne(l, r) => Ok(cmp_expr(BinOp::Ne, program, *l, *r, &kinds)),
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
        } => {
            let t = operand_kind(&kinds, *then_value);
            let e = operand_kind(&kinds, *else_value);
            let want = if t == e { t } else { ScalarKind::F64 };
            Ok(Expr::IfElse {
                condition: Box::new(operand(program, *condition)),
                then: Box::new(Stmt::Expr(typed_operand(
                    program,
                    *then_value,
                    want,
                    &kinds,
                ))),
                else_value: Box::new(Stmt::Expr(typed_operand(
                    program,
                    *else_value,
                    want,
                    &kinds,
                ))),
            })
        }
        EmirOp::VectorCreate(elements) => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements
                .iter()
                .map(|e| typed_operand(program, *e, ScalarKind::F64, &kinds))
                .collect(),
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
                        .map(|c| {
                            typed_operand(program, elements[r * cols + c], ScalarKind::F64, &kinds)
                        })
                        .collect(),
                })
                .collect(),
        }),
        EmirOp::VectorIndex { vector, index } => Ok(map_index_result(format!(
            "emath_rt::vec_index_checked({}, {})",
            render_expr(&operand_ref(program, *vector)),
            index_f64(program, *index, &kinds),
        ))),
        EmirOp::MatrixIndex { matrix, row, col } => Ok(map_index_result(format!(
            "emath_rt::mat_index_checked({}, {}, {})",
            render_expr(&operand_ref(program, *matrix)),
            index_f64(program, *row, &kinds),
            index_f64(program, *col, &kinds),
        ))),
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
                typed_operand(program, *r, ScalarKind::F64, &kinds),
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
        EmirOp::Stencil3d {
            input,
            weights,
            center,
            edge,
        } => {
            if matches!(edge, EdgePolicy::Dirichlet { .. }) {
                return Err(BackendError::Lowering(
                    "3D Dirichlet boundary is not yet supported for Stencil3d".to_string(),
                ));
            }
            let weights = weights
                .iter()
                .map(|weight| format!("{weight:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(map_index_result(format!(
                "emath_rt::stencil_3d_checked({}, &[{weights}], ({}, {}, {}), {})",
                render_expr(&operand_ref(program, *input)),
                center.0,
                center.1,
                center.2,
                render_expr(&edge_policy_literal(edge)),
            )))
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
            vec![
                operand_ref(program, *l),
                typed_operand(program, *r, ScalarKind::F64, &kinds),
            ],
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
        EmirOp::TensorCreate { shape, elements } => {
            let shape_lits = shape
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let data = elements
                .iter()
                .map(|elem| render_expr(&typed_operand(program, *elem, ScalarKind::F64, &kinds)))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "emath_rt::Tensor {{ shape: vec![{shape_lits}], data: vec![{data}] }}"
            )))
        }
        EmirOp::TensorIndex { tensor, indices } => {
            Ok(tensor_index_call(program, *tensor, indices, &kinds))
        }
        EmirOp::TensorSlice { tensor, axes } => {
            Ok(tensor_slice_call(program, *tensor, axes, &kinds))
        }
        EmirOp::TensorAdd(l, r) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {left}.shape.clone(), data: emath_rt::tensor_add(&{left}.data, &{right}.data) }}",
            left = render_expr(&operand(program, *l)),
            right = render_expr(&operand(program, *r)),
        ))),
        EmirOp::TensorSub(l, r) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {left}.shape.clone(), data: emath_rt::tensor_sub(&{left}.data, &{right}.data) }}",
            left = render_expr(&operand(program, *l)),
            right = render_expr(&operand(program, *r)),
        ))),
        EmirOp::TensorScale(tensor, scale) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {tensor}.shape.clone(), data: emath_rt::tensor_scale(&{tensor}.data, {scale}) }}",
            tensor = render_expr(&operand(program, *tensor)),
            scale = render_expr(&typed_operand(program, *scale, ScalarKind::F64, &kinds)),
        ))),
        EmirOp::Einsum { subscripts, inputs } => {
            let operands = inputs
                .iter()
                .map(|v| {
                    format!(
                        "emath_rt::EinsumIn::einsum_operand(&{})",
                        render_expr(&operand(program, *v))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut escaped = String::with_capacity(subscripts.len());
            for ch in subscripts.chars() {
                match ch {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    ch => escaped.push(ch),
                }
            }
            let reshape = match emath_rt::einsum_output_rank(subscripts) {
                0 => "einsum_as_scalar",
                1 => "einsum_as_vector",
                2 => "einsum_as_matrix",
                _ => "einsum_as_tensor",
            };
            Ok(Expr::Raw(format!(
                "emath_rt::{reshape}(\"{escaped}\", &[{operands}])"
            )))
        }
        EmirOp::Factorial(n) => Ok(rt_call(
            "factorial",
            vec![typed_operand(program, *n, ScalarKind::I64, &kinds)],
        )),
        EmirOp::ModInv(a, m) => Ok(rt_call(
            "mod_inv",
            vec![
                typed_operand(program, *a, ScalarKind::I64, &kinds),
                typed_operand(program, *m, ScalarKind::I64, &kinds),
            ],
        )),
        EmirOp::Congruence(a, b, m) => Ok(Expr::Raw(format!(
            "(((__e{} as i64) - (__e{} as i64)).rem_euclid(__e{} as i64) == 0)",
            a.0, b.0, m.0
        ))),
        EmirOp::PolyEvalMod(coeffs, x, p) => Ok(Expr::Raw(format!(
            "emath_rt::poly_eval_mod(&__e{}, {} as i64, {} as i64)",
            coeffs.0,
            render_expr(&operand(program, *x)),
            render_expr(&operand(program, *p)),
        ))),
        EmirOp::RSEncode(coeffs, n, p) => Ok(Expr::Raw(format!(
            "emath_rt::rs_encode(&__e{}, __e{} as i64, __e{} as i64)",
            coeffs.0, n.0, p.0
        ))),
        EmirOp::HammingDistance(a, b) => Ok(Expr::Raw(format!(
            "emath_rt::hamming_distance(&__e{}, &__e{})",
            a.0, b.0
        ))),
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
            let i64_fold = matches!(combine, FoldCombine::Add | FoldCombine::Mul)
                && fold_is_i64(
                    &kinds,
                    *init,
                    *loop_var_index,
                    body,
                    names,
                    states,
                    i64_names,
                );
            let mut body_i64 = i64_names.clone();
            if i64_fold {
                body_i64.insert("__fold_var".to_string());
            }
            let body_expr = value_expr(body, &body_names, states, &body_i64)?;
            let body_code = render_expr(&body_expr);
            let start_i = format!("({}) as i64", render_expr(&operand(program, *start)));
            let end_i = format!("({}) as i64", render_expr(&operand(program, *end)));
            let index_fault = program_may_index_fault(body);
            if i64_fold {
                let fn_name = match combine {
                    FoldCombine::Add => "fold_add_i64",
                    FoldCombine::Mul => "fold_mul_i64",
                    FoldCombine::And | FoldCombine::Or => unreachable!("bool fold is not i64"),
                };
                Ok(rt_call(
                    fn_name,
                    vec![
                        Expr::Raw(format!("&|__fold_var: i64| {body_code}")),
                        Expr::Raw(start_i),
                        Expr::Raw(end_i),
                        typed_operand(program, *init, ScalarKind::I64, &kinds),
                    ],
                ))
            } else if index_fault {
                let (fn_name, init_arg, ret_ty) = match combine {
                    FoldCombine::Add => (
                        "fold_add_checked",
                        render_expr(&typed_operand(program, *init, ScalarKind::F64, &kinds)),
                        "f64",
                    ),
                    FoldCombine::Mul => (
                        "fold_mul_checked",
                        render_expr(&typed_operand(program, *init, ScalarKind::F64, &kinds)),
                        "f64",
                    ),
                    FoldCombine::And => ("fold_all_checked", "true".to_string(), "bool"),
                    FoldCombine::Or => ("fold_any_checked", "false".to_string(), "bool"),
                };
                Ok(Expr::Raw(format!(
                    "emath_rt::{fn_name}(&|__fold_var: f64| -> Result<{ret_ty}, String> {{ Ok({body_code}) }}, {start_i}, {end_i}, {init_arg})?"
                )))
            } else {
                let (fn_name, init_arg) = match combine {
                    FoldCombine::Add => (
                        "fold_add",
                        render_expr(&typed_operand(program, *init, ScalarKind::F64, &kinds)),
                    ),
                    FoldCombine::Mul => (
                        "fold_mul",
                        render_expr(&typed_operand(program, *init, ScalarKind::F64, &kinds)),
                    ),
                    FoldCombine::And => ("fold_all", "true".to_string()),
                    FoldCombine::Or => ("fold_any", "false".to_string()),
                };
                Ok(rt_call(
                    fn_name,
                    vec![
                        Expr::Raw(format!("&|__fold_var: f64| {body_code}")),
                        Expr::Raw(start_i),
                        Expr::Raw(end_i),
                        Expr::Raw(init_arg),
                    ],
                ))
            }
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
            let body_expr = value_expr(integrand, &body_names, states, i64_names)?;
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
            let flat = flat_ssa(body, names, states, i64_names, Some(*var_index))?;
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
            let init = op_expr(
                &EmirOp::LoadInput(*var_index),
                program,
                names,
                states,
                i64_names,
            )?;
            let mut solve_names = names.to_vec();
            let vi = *var_index as usize;
            while solve_names.len() <= vi {
                solve_names.push(String::new());
            }
            solve_names[vi] = "__x".to_string();
            let flat = flat_ssa(body, &solve_names, states, i64_names, Some(*var_index))?;
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
            learning_rate: _,
            tolerance,
            max_iter,
        } => {
            // Newton's method on ∇f = 0. One primal/tangent pass per
            // variable gives each partial; Hessian columns are
            // forward-differenced dual gradients. Match interpreter:
            // vanished Hessian / wrong curvature / max_iter exhaustion
            // panic (evaluate is f64).
            if var_indices.is_empty() {
                return Ok(Expr::Raw(
                    "panic!(\"optimize requires at least one variable\")".to_string(),
                ));
            }
            let mut block = String::from("{ let mut __converged = false;\n");
            for (i, vi) in var_indices.iter().enumerate() {
                let init = op_expr(&EmirOp::LoadInput(*vi), program, names, states, i64_names)?;
                block.push_str(&format!("let mut __x{i} = {};\n", render_expr(&init)));
            }
            let mut pass_bodies = Vec::new();
            for (i, vi) in var_indices.iter().enumerate() {
                let mut opt_names = names.to_vec();
                let viu = *vi as usize;
                while opt_names.len() <= viu {
                    opt_names.push(String::new());
                }
                opt_names[viu] = format!("__x{i}");
                let flat = flat_ssa(body, &opt_names, states, i64_names, Some(*vi))?;
                let mut inner = String::new();
                for (pattern, src) in flat.e_lets {
                    inner.push_str(&format!("let {pattern} = {src}; "));
                }
                for (pattern, src) in flat.d_lets {
                    inner.push_str(&format!("let {pattern} = {src}; "));
                }
                inner.push_str(&flat.d_tail);
                pass_bodies.push(inner);
            }
            let n = var_indices.len();
            let emit_grads = |prefix: &str, bodies: &[String]| -> String {
                let mut out = String::new();
                for (i, body) in bodies.iter().enumerate() {
                    out.push_str(&format!("let {prefix}{i} = {{ {body} }};\n"));
                }
                out
            };
            let max_grad_expr = if n == 1 {
                format!("__g0.abs()")
            } else {
                let chain = (0..n)
                    .map(|i| format!("__g{i}.abs()"))
                    .collect::<Vec<_>>()
                    .join(".max(");
                format!("{chain})")
            };
            block.push_str(&format!("for _ in 0..{max_iter} {{\n"));
            block.push_str(&emit_grads("__g", &pass_bodies));
            block.push_str(&format!(
                "if {max_grad_expr} < {tolerance} {{ __converged = true; break; }}\n"
            ));
            for i in 0..n {
                for j in 0..n {
                    block.push_str(&format!("let mut __h_{i}_{j} = 0.0_f64;\n"));
                }
            }
            for j in 0..n {
                block.push_str(&format!("{{\n__x{j} += 1e-8_f64;\n"));
                block.push_str(&emit_grads("__gp", &pass_bodies));
                block.push_str(&format!("__x{j} -= 1e-8_f64;\n"));
                for i in 0..n {
                    block.push_str(&format!("__h_{i}_{j} = (__gp{i} - __g{i}) / 1e-8_f64;\n"));
                }
                block.push_str("}\n");
            }
            block.push_str(&render_optimize_newton_step(n, *maximize));
            block.push_str("}\n");
            block.push_str("if !__converged {\n");
            block.push_str(&emit_grads("__g", &pass_bodies));
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
            let body_expr = value_expr(body, &lim_names, states, i64_names)?;
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
                let e = op_expr(op, body, names, states, i64_names)?;
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
            let grads: Vec<String> = var_indices.iter().map(|vi| format!("__ria{vi}")).collect();
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
    // Primal registers may be i64 (`ConstI64`); tangent math is always f64.
    tangent_str(op, var_index, idx, &|n| format!("(__e{n} as f64)"), &|n| {
        format!("__d{n}")
    })
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
            d(a.0),
            e(b.0),
            e(a.0),
            d(b.0),
            e(b.0),
            e(b.0)
        ),
        EmirOp::Neg(a) => format!("-{}", d(a.0)),
        EmirOp::UnaryBuiltin(id, a) => id.rust_tangent_unary(e, d, idx as u32, a.0),
        // Match interpreter: constant-exponent form when db==0 (avoids ln
        // for a<=0); otherwise general a^b * (b*a'/a + b'*ln(a)).
        EmirOp::F64Pow(a, b) => format!(
            "if {} == 0.0 {{ if {} == 0.0 {{ 0.0 }} else {{ {} * {}.powf({} - 1.0) * {} }} }} else {{ {} * ({} * {} / {} + {} * {}.ln()) }}",
            d(b.0),
            e(b.0),
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
        EmirOp::BinaryBuiltin(id, a, b) => id.rust_tangent_binary(e, d, idx as u32, a.0, b.0),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: ev,
        } => format!(
            "if {} != 0.0 {{ {} }} else {{ {} }}",
            e(c.0),
            d(t.0),
            d(ev.0)
        ),
        EmirOp::IsFinite(_) => "0.0".to_string(),
        EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(..) => "0.0".to_string(),
        _ => "0.0".to_string(),
    }
}

/// Backward-pass adjoint update statements for an EMIR op, accumulating
/// into `__ra{N}` operand adjoints and `__ria{N}` input adjoints.
pub(crate) fn reverse_adjoint_str(op: &EmirOp, idx: usize) -> String {
    let adj = format!("__ra{idx}");
    let p = |n: u32| format!("(__re{n} as f64)");
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
            a(x.0),
            p(y.0),
            a(y.0),
            p(x.0)
        ),
        EmirOp::F64Div(x, y) => format!(
            "{} += {adj} / {};\n{} -= {adj} * {} / ({} * {});\n",
            a(x.0),
            p(y.0),
            a(y.0),
            p(x.0),
            p(y.0),
            p(y.0)
        ),
        EmirOp::Neg(x) => format!("{} -= {adj};\n", a(x.0)),
        EmirOp::UnaryBuiltin(id, x) => id
            .rust_adjoint_unary(&adj, &p, idx as u32, x.0)
            .unwrap_or_default(),
        EmirOp::F64Pow(x, y) => format!(
            "if {} != 0.0 {{\n\
                 if {} != 0.0 {{ {} += {adj} * __re{idx} * {} / {}; }}\n\
                 else {{ {} += {adj} * {} * {}.powf({} - 1.0); }}\n\
             }}\n\
             {} += {adj} * __re{idx} * {}.ln();\n",
            p(y.0),
            p(x.0),
            a(x.0),
            p(y.0),
            p(x.0),
            a(x.0),
            p(y.0),
            p(x.0),
            p(y.0),
            a(y.0),
            p(x.0)
        ),
        EmirOp::BinaryBuiltin(id, x, y) => id
            .rust_adjoint_binary(&adj, &p, idx as u32, x.0, y.0)
            .unwrap_or_default(),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: ev,
        } => format!(
            "if {} != 0.0 {{ {} += {adj}; }} else {{ {} += {adj}; }}\n",
            p(c.0),
            a(t.0),
            a(ev.0)
        ),
        // Non-differentiable ops: no adjoint contribution.
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
        | EmirOp::Iff(..) => String::new(),
        _ => String::new(),
    };
    if updates.is_empty() {
        String::new()
    } else {
        format!("if {adj} != 0.0 {{\n{updates}}}\n")
    }
}
