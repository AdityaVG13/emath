use std::collections::{BTreeSet, HashMap};

use emath_exec_ir::optimize::{is_total, operand_registers};
use emath_exec_ir::{
    EdgePolicy, EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine, ProbKind,
};
use crate::rust_ir::ast::{BinOp, Block, Expr, Stmt, Ty, UnOp, escape_ident};
use crate::rust_ir::render::render_expr;

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
        EmirOp::ConstText(_)
        | EmirOp::FormatText { .. }
        | EmirOp::TextNfc(_)
        | EmirOp::ReportSection { .. }
        | EmirOp::ReportDocument { .. }
        | EmirOp::ReportMarkdown(_)
        | EmirOp::ReportLatex(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::SetCreate { .. }
        | EmirOp::RecordCreate { .. } => ScalarKind::Other,
        EmirOp::TextLength(_) => ScalarKind::I64,
        EmirOp::SeriesSample { .. } => ScalarKind::F64,
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
        EmirOp::Factorial(_) | EmirOp::ModInv(_, _) | EmirOp::IntRem(_, _) | EmirOp::PolyEvalMod(..) => ScalarKind::I64,
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
        | EmirOp::ControlPolesStable(_)
        | EmirOp::SetContains { .. }
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
        // Option/Result carrier ops (aj8d): is_some/is_ok are Bool;
        // the unwrap honesty gate's scalar kind is the default's; the
        // carriers themselves are opaque (never cast by typed_operand).
        EmirOp::OptionIsSome(_) | EmirOp::ResultIsOk(_) => ScalarKind::Bool,
        EmirOp::OptionUnwrapOr(_, default) | EmirOp::ResultUnwrapOr(_, default) => {
            kind_at(kinds, *default)
        }
        EmirOp::OptionSome(_)
        | EmirOp::OptionNone
        | EmirOp::ResultOk(_)
        | EmirOp::ResultErr(_)
        | EmirOp::ResultErrorOf(_) => ScalarKind::Other,
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
    // Inside generated model rate/step methods (`value_expr_rate`), the
    // body is a plain (non-Result) fn: admission validated the model's
    // static shapes, so a checked-op fault there is unreachable. Render
    // it as an honest panic (precedent: the optimize hessian-vanished
    // panic) instead of `?`, which does not compile outside `Result`
    // (E0277 in generated model crates, e.g. heat-volume `der_u`).
    if rate_context() {
        return Expr::Raw(format!(
            "{call}.map_err(|e| e.to_string()).expect(\"internal: checked-op fault on admitted model\")"
        ));
    }
    Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
}

/// Exact-integer null-vector call body: a `Result<Vec<f64>, String>` in
/// a `{ ... }` block. Runtime parity with the interpreter's `IntNullspace`
/// arm: a non-integral or out-of-i64-range entry refuses with
/// E-NULLSPACE-001, a kernel failure (ragged rows / exact-integer
/// overflow) refuses with E-NULLSPACE-001, and a nullspace that is not
/// exactly one-dimensional refuses with E-NULLSPACE-002 (never a
/// fabricated output). The value on success is the kernel's canonical
/// primitive vector widened to f64 (entries are exact small integers,
/// first nonzero entry positive).
fn int_nullspace_call(program: &EmirProgram, matrix: EmirValue) -> String {
    format!(
        "{{ let __mat = &{};\n\
         let __rows: Vec<Vec<i64>> = __mat.iter().map(|__r| -> Result<Vec<i64>, String> {{\n\
         let mut __row = Vec::with_capacity(__r.len());\n\
         for &__x in __r {{\n\
         if __x.fract() != 0.0 || __x < -9223372036854775808.0 || __x >= 9223372036854775808.0 {{\n\
         return Err(\"E-NULLSPACE-001: non-integral entry in integer nullspace input\".to_string());\n\
         }}\n\
         __row.push(__x as i64);\n\
         }}\n\
         Ok(__row)\n\
         }}).collect::<Result<Vec<Vec<i64>>, String>>()\n\
         .and_then(|__rows| emath_rt::primitive_int_nullvector(&__rows).map_err(|_| \"E-NULLSPACE-001: exact-integer overflow in nullspace input\".to_string()))\n\
         .and_then(|__out| __out.ok_or_else(|| \"E-NULLSPACE-002: integer matrix has no exactly one-dimensional nullspace\".to_string()))\n\
         .map(|__out| __out.into_iter().map(|__v| __v as f64).collect::<Vec<f64>>()) }}",
        render_expr(&operand_ref(program, matrix)),
    )
}

/// Result-context surfacing of `int_nullspace_call`: `?` in `Result`
/// fns (constructor / CLI envelope paths), panic in plain-fn model
/// rate/step bodies — same split the checked ops use via `map_index_result`.
fn int_nullspace_result(call: String) -> Expr {
    if rate_context() {
        Expr::Raw(format!(
            "{call}.unwrap_or_else(|e| panic!(\"internal: int-nullspace fault on admitted model: {{e}}\"))"
        ))
    } else {
        Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
    }
}

/// Exact integer product-difference call body: a `Result<f64, String>`
/// in a `{ ... }` block. Runtime parity with the exact-rational
/// equality primitive: entries must be exact small nonnegative
/// integers (< 2^53, E-EXACT-001), vectors must agree in length
/// (E-EXACT-001), products run over u128 with overflow refusal
/// (E-EXACT-002), and products are COMPARED IN u128 BEFORE any f64
/// cast — distinct exact products near 1e18 must never compare equal
/// through a lossy cast (the interpreter's two-cast compare was a
/// false-zero defect; the generated path implements the fixed rule).
/// The result is `0.0` when the products are exactly equal, otherwise
/// the exact u128 difference widened to f64 with the sign of
/// `p_product - q_product`.
fn exact_product_delta_call(program: &EmirProgram, p: EmirValue, q: EmirValue) -> String {
    format!(
        "{{ let __p = &{};\n\
         let __q = &{};\n\
         (|| -> Result<f64, String> {{\n\
         let __exact = |__x: f64| -> Result<u128, String> {{\n\
         if __x.fract() != 0.0 || __x < 0.0 || __x >= 9007199254740992.0 {{\n\
         return Err(\"E-EXACT-001: entries must be exact small nonnegative integers\".to_string());\n\
         }}\n\
         Ok(__x as u128)\n\
         }};\n\
         if __p.len() != __q.len() {{\n\
         return Err(\"E-EXACT-001: numerator and denominator vectors differ in length\".to_string());\n\
         }}\n\
         let mut __pp = 1u128;\n\
         for &__x in __p {{ __pp = __pp.checked_mul(__exact(__x)?).ok_or_else(|| \"E-EXACT-002: exact product overflow (use reduced K_i)\".to_string())?; }}\n\
         let mut __qq = 1u128;\n\
         for &__x in __q {{ __qq = __qq.checked_mul(__exact(__x)?).ok_or_else(|| \"E-EXACT-002: exact product overflow (use reduced K_i)\".to_string())?; }}\n\
         if __pp == __qq {{ return Ok(0.0); }}\n\
         let (__big, __small) = if __pp > __qq {{ (__pp, __qq) }} else {{ (__qq, __pp) }};\n\
         Ok((__big - __small) as f64 * if __pp > __qq {{ 1.0 }} else {{ -1.0 }})\n\
         }})() }}",
        render_expr(&operand_ref(program, p)),
        render_expr(&operand_ref(program, q)),
    )
}

/// Result-context surfacing of `exact_product_delta_call` — same split
/// as `int_nullspace_result`.
fn exact_product_delta_result(call: String) -> Expr {
    if rate_context() {
        Expr::Raw(format!(
            "{call}.unwrap_or_else(|e| panic!(\"internal: exact-product-delta fault on admitted model: {{e}}\"))"
        ))
    } else {
        Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
    }
}

thread_local! {
    /// Set while rendering generated model rate/step method bodies, where
    /// checked-op faults are unreachable by admission and must not render
    /// as `?` (no `Result` return type there).
    static RATE_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn rate_context() -> bool {
    RATE_CONTEXT.with(std::cell::Cell::get)
}

/// [`value_expr`] for generated model rate methods: checked ops
/// (`Stencil3d`, tensor index/slice) render panic-on-unreachable instead
/// of `?`. See [`map_index_result`].
pub(crate) fn value_expr_rate(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    RATE_CONTEXT.with(|cell| cell.set(true));
    let out = value_expr(program, names, states, i64_names);
    RATE_CONTEXT.with(|cell| cell.set(false));
    out
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

/// Concrete Rust type of a non-carrier EMIR register — the payload slots
/// of Option/Result carriers. Scalars resolve by `ScalarKind`; vector /
/// matrix / tensor producers resolve structurally; everything else that
/// computes a plain number falls back to `f64`.
fn register_rust_ty(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ScalarKind],
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Option<String> {
    let i = value.0 as usize;
    let op: &EmirOp = &program.ops.get(i)?.0;
    match op {
        EmirOp::ConstI64(_)
        | EmirOp::Factorial(_)
        | EmirOp::ModInv(..)
        | EmirOp::Congruence(..)
        | EmirOp::PolyEvalMod(..)
        | EmirOp::TextLength(_)
        | EmirOp::HammingDistance(..)
        | EmirOp::RSEncode(..) => Some("i64".to_string()),
        EmirOp::ConstF64(_) => Some("f64".to_string()),
        EmirOp::ConstBool(_) | EmirOp::Fold {
            combine: FoldCombine::And | FoldCombine::Or,
            ..
        } => Some("bool".to_string()),
        EmirOp::ConstText(_) => Some("String".to_string()),
        EmirOp::VectorCreate(_) => Some("Vec<f64>".to_string()),
        EmirOp::MatrixCreate { .. } => Some("Vec<Vec<f64>>".to_string()),
        EmirOp::TensorCreate { .. } => Some("emath_rt::Tensor".to_string()),
        EmirOp::SeriesCreate { .. } => Some("Vec<(f64, f64)>".to_string()),
        EmirOp::Fold {
            combine: FoldCombine::Add | FoldCombine::Mul,
            init,
            loop_var_index,
            body,
            ..
        } => {
            if fold_is_i64(kinds, *init, *loop_var_index, body, names, states, i64_names) {
                Some("i64".to_string())
            } else {
                Some("f64".to_string())
            }
        }
        EmirOp::LoadInput(index) => {
            let name = names.get(*index as usize)?;
            if i64_names.contains(name) {
                Some("i64".to_string())
            } else {
                Some("f64".to_string())
            }
        }
        EmirOp::LoadState(index) => {
            let name = states.get(*index as usize)?;
            if i64_names.contains(name) {
                Some("i64".to_string())
            } else {
                Some("f64".to_string())
            }
        }
        _ => match kind_at(kinds, value) {
            ScalarKind::I64 => Some("i64".to_string()),
            ScalarKind::Bool => Some("bool".to_string()),
            _ => Some("f64".to_string()),
        },
    }
}

/// Payload Rust types of every Option/Result carrier register, resolved
/// by dataflow over the SSA program: producers (the payload of
/// `option_some`/`result_ok`/`result_err`) and consumers (the eager
/// default of the `unwrap_or` honesty gate; the error payload composed
/// by `result_error_of`) must agree. A conflict is a typed lowering
/// refusal (interp TypeConfusion parity), never a panic. A payload kind
/// the program never materializes (e.g. the Err slot of a `result_ok`
/// that is only `is_ok`-ed) defaults to the sibling slot so every
/// carrier register still gets one concrete Rust type.
struct CarrierPayloadTypes {
    /// Option carrier register → payload Rust type.
    opt: HashMap<u32, String>,
    /// Result carrier register → Ok payload Rust type.
    ok: HashMap<u32, String>,
    /// Result carrier register → Err payload Rust type.
    err: HashMap<u32, String>,
}

impl CarrierPayloadTypes {
    fn option(&self, register: u32) -> String {
        self.opt
            .get(&register)
            .cloned()
            .unwrap_or_else(|| "f64".to_string())
    }

    fn result_ok(&self, register: u32) -> String {
        self.ok
            .get(&register)
            .cloned()
            .unwrap_or_else(|| "f64".to_string())
    }

    fn result_err(&self, register: u32) -> String {
        self.err
            .get(&register)
            .cloned()
            .unwrap_or_else(|| "f64".to_string())
    }
}

/// Resolve a register's NATIVE Rust type, recursing through carrier
/// producers so NESTED carriers type correctly: `Option<Option<i64>>`,
/// `Result<Option<i64>, i64>`, and the error-as-option projection. A
/// non-carrier producer falls through to `register_rust_ty`. SSA is
/// acyclic, so recursion terminates (aj8d pass 4 nested parity).
fn nested_operand_ty(
    program: &EmirProgram,
    register: EmirValue,
    kinds: &[ScalarKind],
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Option<String> {
    let Some((op, _)) = program.ops.get(register.0 as usize) else {
        return None;
    };
    match op {
        EmirOp::OptionSome(payload) => Some(format!(
            "Option<{}>",
            nested_operand_ty(program, *payload, kinds, names, states, i64_names)
                .unwrap_or_else(|| "f64".to_string())
        )),
        EmirOp::OptionNone => Some("Option<f64>".to_string()),
        EmirOp::ResultOk(payload) | EmirOp::ResultErr(payload) => {
            let inner = nested_operand_ty(program, *payload, kinds, names, states, i64_names)
                .unwrap_or_else(|| "f64".to_string());
            Some(format!("Result<{inner}, {inner}>"))
        }
        EmirOp::ResultErrorOf(carrier) => {
            let err_ty = match program.ops.get(carrier.0 as usize) {
                Some((EmirOp::ResultErr(payload), _)) => {
                    nested_operand_ty(program, *payload, kinds, names, states, i64_names)
                        .unwrap_or_else(|| "f64".to_string())
                }
                _ => "f64".to_string(),
            };
            Some(format!("Option<{err_ty}>"))
        }
        EmirOp::OptionUnwrapOr(_, default) | EmirOp::ResultUnwrapOr(_, default) => {
            nested_operand_ty(program, *default, kinds, names, states, i64_names)
        }
        _ => register_rust_ty(program, register, kinds, names, states, i64_names),
    }
}

fn carrier_payload_types(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Result<CarrierPayloadTypes, BackendError> {
    let kinds = scalar_kinds(program, names, states, i64_names);
    let mut tys = CarrierPayloadTypes {
        opt: HashMap::new(),
        ok: HashMap::new(),
        err: HashMap::new(),
    };
    let bind = |map: &mut HashMap<u32, String>,
                register: u32,
                ty: String,
                op: &EmirOp|
     -> Result<bool, BackendError> {
        match map.get(&register) {
            Some(existing) if existing != &ty => Err(BackendError::Lowering(format!(
                "op `{}` carrier payload kind conflict: `{existing}` vs `{ty}` (interp TypeConfusion parity)",
                op.name()
            ))),
            Some(_) => Ok(false),
            None => {
                map.insert(register, ty);
                Ok(true)
            }
        }
    };
    let payload_ty = |register: EmirValue,
                      op: &EmirOp|
     -> Result<String, BackendError> {
        nested_operand_ty(program, register, &kinds, names, states, i64_names).ok_or_else(|| {
            BackendError::Lowering(format!(
                "op `{}` payload register {} out of range",
                op.name(),
                register.0
            ))
        })
    };
    // Producer-determined payload types.
    for (i, (op, _)) in program.ops.iter().enumerate() {
        match op {
            EmirOp::OptionSome(payload) => {
                bind(&mut tys.opt, i as u32, payload_ty(*payload, op)?, op)?;
            }
            EmirOp::ResultOk(payload) => {
                bind(&mut tys.ok, i as u32, payload_ty(*payload, op)?, op)?;
            }
            EmirOp::ResultErr(payload) => {
                bind(&mut tys.err, i as u32, payload_ty(*payload, op)?, op)?;
            }
            _ => {}
        }
    }
    // Consumer and error_of propagation to a fixpoint (SSA is acyclic, so
    // this terminates in at most the number of registers).
    loop {
        let mut changed = false;
        for (i, (op, _)) in program.ops.iter().enumerate() {
            match op {
                EmirOp::OptionUnwrapOr(carrier, default) => {
                    changed |= bind(
                        &mut tys.opt,
                        carrier.0,
                        payload_ty(*default, op)?,
                        op,
                    )?;
                }
                EmirOp::ResultUnwrapOr(carrier, default) => {
                    changed |= bind(&mut tys.ok, carrier.0, payload_ty(*default, op)?, op)?;
                }
                EmirOp::ResultErrorOf(carrier) => {
                    if let Some(err_ty) = tys.err.get(&carrier.0).cloned() {
                        changed |= bind(&mut tys.opt, i as u32, err_ty, op)?;
                    }
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }
    // Fill never-materialized slots; a Result carrier with one known
    // side mirrors it onto the other (both type params must be concrete
    // in Rust; the unknown side never carries a value).
    for (i, (op, _)) in program.ops.iter().enumerate() {
        match op {
            EmirOp::OptionSome(_) | EmirOp::OptionNone | EmirOp::ResultErrorOf(_) => {
                tys.opt.entry(i as u32).or_insert_with(|| "f64".to_string());
            }
            EmirOp::ResultOk(_) | EmirOp::ResultErr(_) => {
                let ok_entry = tys.ok.entry(i as u32).or_insert_with(|| "f64".to_string());
                let err_entry = tys.err.entry(i as u32).or_insert_with(|| "f64".to_string());
                if ok_entry == "f64" && err_entry != "f64" {
                    *ok_entry = err_entry.clone();
                }
                if err_entry == "f64" && ok_entry != "f64" {
                    *err_entry = ok_entry.clone();
                }
            }
            _ => {}
        }
    }
    Ok(tys)
}

/// Index of `op` inside `program.ops` (pointer identity; `op` is always
/// borrowed from that slice during rendering).
fn op_self_index(program: &EmirProgram, op: &EmirOp) -> Option<u32> {
    program
        .ops
        .iter()
        .position(|(produced, _)| std::ptr::eq(produced, op))
        .map(|i| i as u32)
}

/// Static carrier-shape check (interp TypeConfusion parity): a carrier
/// operand must be produced by a carrier op of the matching family,
/// otherwise the strict backend refuses typed — a `BackendError`, never
/// a Rust panic, never a silent scalar shadow.
fn expect_carrier(
    program: &EmirProgram,
    value: EmirValue,
    is_result: bool,
    consumer: &str,
) -> Result<(), BackendError> {
    let Some(producer) = program.ops.get(value.0 as usize).map(|(op, _)| op) else {
        return Err(BackendError::Lowering(format!(
            "op `{consumer}` carrier operand register {} out of range",
            value.0
        )));
    };
    let family_ok = match is_result {
        false => matches!(
            producer,
            EmirOp::OptionSome(_) | EmirOp::OptionNone | EmirOp::ResultErrorOf(_)
        ),
        true => matches!(producer, EmirOp::ResultOk(_) | EmirOp::ResultErr(_)),
    };
    if family_ok {
        Ok(())
    } else {
        Err(BackendError::Lowering(format!(
            "op `{consumer}` requires a {} carrier, got register {} produced by `{}` (interp TypeConfusion parity)",
            if is_result { "Result" } else { "Option" },
            value.0,
            producer.name()
        )))
    }
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
        EmirOp::ConstText(value) => Ok(Expr::Str(value.clone())),
        EmirOp::FormatText {
            template,
            arguments,
        } => {
            let mut args = Vec::with_capacity(arguments.len() + 1);
            args.push(Expr::Str(rust_format_template(template)));
            args.extend(arguments.iter().map(|argument| operand(program, *argument)));
            Ok(Expr::Macro {
                name: "format".to_string(),
                args,
            })
        }
        EmirOp::TextLength(text) => Ok(Expr::Raw(format!(
            "{}.chars().count() as i64",
            render_expr(&operand_ref(program, *text))
        ))),
        EmirOp::TextNfc(text) => Ok(Expr::Raw(format!(
            "{}.clone()",
            render_expr(&operand_ref(program, *text))
        ))),
        EmirOp::SpecialFunction {
            function,
            arguments,
            error_bound,
        } => {
            let arguments = arguments
                .iter()
                .map(|argument| render_expr(&operand_ref(program, *argument)))
                .collect::<Vec<_>>()
                .join(", ");
            let field = if *error_bound { "error_bound" } else { "value" };
            Ok(Expr::Raw(format!(
                "emath_rt::special::evaluate_strict(emath_rt::special::SpecialFn::{function:?}, &[{arguments}]).expect(\"special-function domain checked by emath\").{field}"
            )))
        }
        EmirOp::ReportSection { heading, body } => Ok(Expr::Raw(format!(
            "({}, {})",
            render_expr(&operand_ref(program, *heading)),
            render_expr(&operand_ref(program, *body))
        ))),
        EmirOp::ReportDocument { title, section } => Ok(Expr::Raw(format!(
            "({}, {})",
            render_expr(&operand_ref(program, *title)),
            render_expr(&operand_ref(program, *section))
        ))),
        EmirOp::ReportMarkdown(document) => {
            let document = render_expr(&operand_ref(program, *document));
            Ok(Expr::Raw(format!(
                "format!(\"# {{}}\\n\\n## {{}}\\n\\n{{}}\\n\", ({document}).0, (({document}).1).0, (({document}).1).1)"
            )))
        }
        EmirOp::ReportLatex(document) => {
            let document = render_expr(&operand_ref(program, *document));
            Ok(Expr::Raw(format!(
                "format!(\"\\\\section{{{{{{}}}}}}\\n\\\\subsection{{{{{{}}}}}}\\n{{}}\\n\", ({document}).0, (({document}).1).0, (({document}).1).1)"
            )))
        }
        EmirOp::SeriesCreate { points, .. } => Ok(Expr::Raw(format!(
            "vec![{}]",
            points
                .iter()
                .map(|(time, value)| format!("({time:?}f64, {value:?}f64)"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        EmirOp::SeriesSample { series, time } => {
            let source = program
                .ops
                .get(series.0 as usize)
                .map(|(op, _)| op)
                .ok_or_else(|| BackendError::Lowering("series register out of range".into()))?;
            let EmirOp::SeriesCreate {
                interpolation,
                extrapolation,
                ..
            } = source
            else {
                return Err(BackendError::Lowering(
                    "generated Rust currently samples literal series only".into(),
                ));
            };
            let outside = match extrapolation.as_str() {
                "refuse" => {
                    "assert!(__t >= __series[0].0 && __t <= __series[__series.len()-1].0, \"series sample outside support\");"
                }
                "clamp" => "let __t = __t.max(__series[0].0).min(__series[__series.len()-1].0);",
                "extend" => "",
                _ => {
                    return Err(BackendError::Lowering(
                        "unknown series extrapolation policy".into(),
                    ));
                }
            };
            let value = match interpolation.as_str() {
                "previous" | "pwc" => "__left.1",
                "nearest" => "if __alpha < 0.5 { __left.1 } else { __right.1 }",
                "linear" => "__left.1 + __alpha * (__right.1 - __left.1)",
                "monotone_cubic" => {
                    "{ let __secant = (__right.1-__left.1)/(__right.0-__left.0); let __left_slope = if __index == 0 { __secant } else { let __prior = (__left.1-__series[__index-1].1)/(__left.0-__series[__index-1].0); if __prior.signum() == __secant.signum() { 0.5*(__prior+__secant) } else { 0.0 } }; let __right_slope = if __index+2 == __series.len() { __secant } else { let __next = (__series[__index+2].1-__right.1)/(__series[__index+2].0-__right.0); if __next.signum() == __secant.signum() { 0.5*(__secant+__next) } else { 0.0 } }; let __h=__right.0-__left.0; let __a2=__alpha*__alpha; let __a3=__a2*__alpha; (2.0*__a3-3.0*__a2+1.0)*__left.1 + (__a3-2.0*__a2+__alpha)*__h*__left_slope + (-2.0*__a3+3.0*__a2)*__right.1 + (__a3-__a2)*__h*__right_slope }"
                }
                _ => {
                    return Err(BackendError::Lowering(
                        "unknown series interpolation policy".into(),
                    ));
                }
            };
            Ok(Expr::Raw(format!(
                "{{ let __series = &{}; let __t = {}; {} if __t == __series[__series.len()-1].0 {{ __series[__series.len()-1].1 }} else {{ let __index = __series.windows(2).position(|w| __t >= w[0].0 && __t < w[1].0).unwrap_or(if __t < __series[0].0 {{ 0 }} else {{ __series.len()-2 }}); let __left = __series[__index]; let __right = __series[__index+1]; let __alpha = (__t-__left.0)/(__right.0-__left.0); {} }} }}",
                render_expr(&operand_ref(program, *series)),
                render_expr(&operand_ref(program, *time)),
                outside,
                value
            )))
        }
        EmirOp::SetCreate { elements, guards } => {
            let mut source = String::from("{ let mut __set = Vec::new(); ");
            for (element, guard) in elements.iter().zip(guards) {
                let value = render_expr(&operand_ref(program, *element));
                if let Some(guard) = guard {
                    source.push_str(&format!(
                        "if {} {{ let __value = {}; if !__set.contains(&__value) {{ __set.push(__value); }} }} ",
                        render_expr(&operand_ref(program, *guard)),
                        value
                    ));
                } else {
                    source.push_str(&format!(
                        "let __value = {value}; if !__set.contains(&__value) {{ __set.push(__value); }} "
                    ));
                }
            }
            source.push_str("__set }");
            Ok(Expr::Raw(source))
        }
        EmirOp::SetContains { element, set } => Ok(Expr::Raw(format!(
            "{}.contains(&{})",
            render_expr(&operand_ref(program, *set)),
            render_expr(&operand_ref(program, *element))
        ))),
        EmirOp::RecordCreate { fields, .. } => {
            let fields = fields
                .iter()
                .map(|(name, value)| {
                    format!("({name:?}, {})", render_expr(&operand_ref(program, *value)))
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "std::collections::BTreeMap::from([{fields}])"
            )))
        }
        EmirOp::ConstComplex(re, im) => Ok(Expr::Raw(format!(
            "num_complex::Complex::new({re:?}, {im:?})"
        ))),
        // Exact-rational cells (emath-rat-real-types-p5cj): the Rust
        // backend has no exact-rational target type in this slice, so a
        // program containing one refuses codegen with a typed error —
        // never a silent f64 demotion of an exact value.
        EmirOp::RatConstruct { .. } | EmirOp::RatAdd(..) | EmirOp::RatNorm(_) => {
            Err(BackendError::Lowering(
                "exact-rational cells (rat/rat_add/rat_norm) are interpreter-only in this slice: no exact-rational Rust target type exists yet, and demoting to f64 would silently break exactness".into(),
            ))
        }
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
        // xx0x.2 richer linear algebra + r2-graphs-masa graph kernels:
        // the embedded emath_rt kernel set carries NESTED-carrier
        // adapters (generated matrices are `Vec<Vec<f64>>`, dims carried
        // by the structure), so the generated crate calls the same
        // algorithmic core as the interpreter through the same rt facade.
        EmirOp::EigenSymmetric(m) => Ok(rt_call("eig_values", vec![operand_ref(program, *m)])),
        EmirOp::EigenVectorsSymmetric(m) => {
            Ok(rt_call("eig_vectors", vec![operand_ref(program, *m)]))
        }
        EmirOp::SvdSingularValues(m) => Ok(rt_call("svd_values", vec![operand_ref(program, *m)])),
        EmirOp::SvdFactors(m) => Ok(rt_call("svd_factors", vec![operand_ref(program, *m)])),
        EmirOp::CgSolve(a, b) => Ok(rt_call(
            "cg_solve",
            vec![operand_ref(program, *a), operand_ref(program, *b)],
        )),
        EmirOp::LinearSolve(a, b) => Ok(rt_call(
            "linear_solve",
            vec![operand_ref(program, *a), operand_ref(program, *b)],
        )),
        EmirOp::LuFactors(matrix) => Ok(rt_call("lu_factors", vec![operand_ref(program, *matrix)])),
        EmirOp::QrFactors(matrix) => Ok(rt_call("qr_factors", vec![operand_ref(program, *matrix)])),
        EmirOp::OuterProduct(left, right) => Ok(rt_call(
            "outer_product",
            vec![operand_ref(program, *left), operand_ref(program, *right)],
        )),
        EmirOp::IntNullspace(matrix) => {
            // Exact integer null vector (generic primitive parity with the
            // interp arm): lowers through the same exact-integer kernel as
            // the reference VM (`emath_rt::primitive_int_nullvector`, no
            // domain naming). A statically non-matrix operand is a typed
            // refusal (interp TypeConfusion parity) rather than a
            // fabricated vector.
            let source = program
                .ops
                .get(matrix.0 as usize)
                .map(|(producer, _)| producer);
            let matrix_carrier = matches!(
                source,
                Some(
                    EmirOp::LoadInput(_)
                        | EmirOp::LoadState(_)
                        | EmirOp::MatrixCreate { .. }
                        | EmirOp::MatrixAdd(..)
                        | EmirOp::MatrixSub(..)
                        | EmirOp::MatrixScale(..)
                        | EmirOp::MatrixMulMatrix(..)
                        | EmirOp::MatrixTranspose(_)
                        | EmirOp::EigenVectorsSymmetric(_)
                        | EmirOp::SvdFactors(_)
                        | EmirOp::LuFactors(_)
                        | EmirOp::QrFactors(_)
                        | EmirOp::OuterProduct(..)
                        | EmirOp::GraphLaplacian(_)
                        | EmirOp::GraphSymmetrize(_)
                )
            );
            if !matrix_carrier {
                return Err(BackendError::Lowering(format!(
                    "int-nullspace op `{}` requires a matrix operand (E-NULLSPACE-001: non-matrix operand refused; interp TypeConfusion parity)",
                    op.name()
                )));
            }
            Ok(int_nullspace_result(int_nullspace_call(program, *matrix)))
        }
        EmirOp::ExactProductDelta(p_value, q_value) => {
            // Exact integer product difference (generic exact-rational
            // equality primitive, no domain naming). Operands are
            // vectors by construction (shape-checked at term compile);
            // the runtime block refuses typed on non-integral/out-of-
            // range entries (E-EXACT-001), length mismatch (E-EXACT-001),
            // and u128 product overflow (E-EXACT-002) — never a
            // fabricated scalar.
            Ok(exact_product_delta_result(exact_product_delta_call(
                program, *p_value, *q_value,
            )))
        }
        // Graph kernels (masa slice 1): invalid input yields the empty
        // result in generated code (the reference interpreter surfaces
        // the typed E-GRAPH codes); kernels never panic. The source
        // vertex passes by value (an f64 register; the kernel validates
        // wholeness and range).
        EmirOp::GraphReachable(adj, source) => Ok(rt_call(
            "graph_reachable",
            vec![
                operand_ref(program, *adj),
                Expr::Raw(render_expr(&operand(program, *source))),
            ],
        )),
        EmirOp::GraphBfsOrder(adj, source) => Ok(rt_call(
            "graph_bfs_order",
            vec![
                operand_ref(program, *adj),
                Expr::Raw(render_expr(&operand(program, *source))),
            ],
        )),
        EmirOp::GraphDijkstra(adj, source) => Ok(rt_call(
            "graph_dijkstra",
            vec![
                operand_ref(program, *adj),
                Expr::Raw(render_expr(&operand(program, *source))),
            ],
        )),
        EmirOp::GraphDegreeOut(adj) => Ok(rt_call(
            "graph_degree_out",
            vec![operand_ref(program, *adj)],
        )),
        EmirOp::GraphLaplacian(adj) => {
            Ok(rt_call("graph_laplacian", vec![operand_ref(program, *adj)]))
        }
        EmirOp::GraphSymmetrize(adj) => Ok(rt_call(
            "graph::symmetrize",
            vec![operand_ref(program, *adj)],
        )),
        EmirOp::GraphBellmanFord(adj, source) => Ok(rt_call(
            "graph::bellman_ford",
            vec![operand_ref(program, *adj), operand_ref(program, *source)],
        )),
        EmirOp::GraphSparseTriplets(adj) => Ok(rt_call(
            "graph::sparse_triplets",
            vec![operand_ref(program, *adj)],
        )),
        EmirOp::GraphSparseFromTriplets(n, triplets) => Ok(rt_call(
            "graph::sparse_from_triplets",
            vec![operand_ref(program, *n), operand_ref(program, *triplets)],
        )),
        // Optimization kernels (r3-lp-milp-wlif slice 1): invalid input
        // yields the empty result in generated code (the reference
        // interpreter surfaces the typed E-LP/E-PARETO codes).
        EmirOp::LpMinimize(a, b, c) => Ok(rt_call(
            "lp_minimize",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *b),
                operand_ref(program, *c),
            ],
        )),
        EmirOp::ParetoFront(points) => {
            Ok(rt_call("pareto_front", vec![operand_ref(program, *points)]))
        }
        // Polynomial kernels (r3-funcspaces-poly-hjor slice 1).
        EmirOp::PolyMul(a, b) => Ok(rt_call(
            "poly_mul",
            vec![operand_ref(program, *a), operand_ref(program, *b)],
        )),
        EmirOp::PolyEval(p, x) => Ok(rt_call(
            "poly_eval",
            vec![operand_ref(program, *p), operand_ref(program, *x)],
        )),
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => Ok(rt_call(
            "sequence_generate",
            vec![
                operand_ref(program, *initial),
                operand_ref(program, *recurrence),
                operand_ref(program, *budget),
            ],
        )),
        EmirOp::SequenceConvolve { left, right, count } => Ok(rt_call(
            "sequence_convolve",
            vec![
                operand_ref(program, *left),
                operand_ref(program, *right),
                operand_ref(program, *count),
            ],
        )),
        // ODE stepping kernels (xx0x.3 thin nucleus): typed wrappers
        // in `emath_rt::dynamics` (Newton backward Euler, velocity
        // Verlet), same refusal surface as the LP/Pareto renders.
        EmirOp::OdeBackwardEuler(rate, y0, h) => Ok(rt_call(
            "dynamics::ode_backward_euler",
            vec![
                operand_ref(program, *rate),
                operand_ref(program, *y0),
                operand_ref(program, *h),
            ],
        )),
        EmirOp::OdeVelocityVerlet(a, q, v, h) => Ok(rt_call(
            "dynamics::ode_velocity_verlet",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *q),
                operand_ref(program, *v),
                operand_ref(program, *h),
            ],
        )),
        // Spectral Poisson (xx0x.4 thin nucleus): typed wrapper in
        // `emath_rt::pde` (Dirichlet sine diagonalization).
        EmirOp::PoissonDirichletSine(load) => Ok(rt_call(
            "pde::poisson_dirichlet_sine",
            vec![operand_ref(program, *load)],
        )),
        // Control surface (zxkl thin B43): raw kernels in `emath_rt`
        // (Routh–Hurwitz stability, Faddeev–LeVerrier characteristic
        // polynomial, pivoted solve); refusals surface through the
        // reference interpreter's typed E-CONTROL codes.
        EmirOp::ControlTransferEval(num, den, x) => Ok(rt_call(
            "control_transfer_eval",
            vec![
                operand_ref(program, *num),
                operand_ref(program, *den),
                operand_ref(program, *x),
            ],
        )),
        EmirOp::ControlDcGain(a, b, c) => Ok(rt_call(
            "control_state_space_dc_gain",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *b),
                operand_ref(program, *c),
            ],
        )),
        EmirOp::ControlPolesStable(den) => Ok(rt_call(
            "control_poles_stable",
            vec![operand_ref(program, *den)],
        )),
        // Finite-category kernels (88wo thin B39): raw kernels in
        // `emath_rt` (law gate over the dense composition table, face
        // path-pair commutativity); refusals surface through the
        // reference interpreter's typed E-CAT codes.
        EmirOp::CategoryCheck(dom, cod, comp) => Ok(rt_call(
            "category_check",
            vec![
                operand_ref(program, *dom),
                operand_ref(program, *cod),
                operand_ref(program, *comp),
            ],
        )),
        EmirOp::CategoryDiagramCommutative(dom, cod, comp, faces) => Ok(rt_call(
            "category_diagram_commutative",
            vec![
                operand_ref(program, *dom),
                operand_ref(program, *cod),
                operand_ref(program, *comp),
                operand_ref(program, *faces),
            ],
        )),
        // Probability nucleus (xx0x.5 thin slice): typed wrappers in
        // `emath_rt::probability` (SplitMix64 stream + exact
        // densities); the family code is the stable kernel encoding.
        EmirOp::ProbSample {
            kind,
            params,
            seed,
            draws,
            stream,
        } => Ok(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_sample_in_stream".to_string(),
            ],
            args: vec![
                Expr::Raw(format!(
                    "emath_rt::probability::Family::{}",
                    match kind {
                        ProbKind::Normal => "Normal",
                        ProbKind::Uniform => "Uniform",
                        ProbKind::Bernoulli => "Bernoulli",
                    }
                )),
                operand_ref(program, *params),
                operand_ref(program, *seed),
                operand_ref(program, *draws),
                stream
                    .map(|value| {
                        Expr::Raw(format!("&{}", render_expr(&operand_ref(program, value))))
                    })
                    .unwrap_or_else(|| Expr::Str(String::new())),
            ],
        }),
        EmirOp::ProbDensity { kind, params, x } => Ok(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_density".to_string(),
            ],
            args: vec![
                Expr::Raw(format!("{} /* {} */", kind.code(), kind.name())),
                operand_ref(program, *params),
                operand_ref(program, *x),
            ],
        }),
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
        // Universal exact-Euclidean remainder (aj8d pass 6). Mirrors
        // ModInv's parity posture: the interpreter enforces the positive
        // modulus as a typed EvalFault; the generated Rust emits exact
        // rem_euclid on admitted (positive-modulus) programs. rem_euclid
        // is total for a positive i64 modulus — no panic path, exact i64.
        EmirOp::IntRem(a, m) => Ok(Expr::Raw(format!(
            "((__e{} as i64).rem_euclid(__e{} as i64))",
            a.0, m.0
        ))),
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
        // Capability application is data dispatched from the capability
        // layer (fjxh.6 contract): the Rust backend does not inline cell
        // bodies; the interp world evaluates them. Refuse typed rather
        // than emit a silent identity.
        op @ EmirOp::ApplyCapability { .. } => Err(BackendError::Lowering(format!(
            "capability application `{}` is not lowered by the strict Rust backend; run in the interp world",
            op.name()
        ))),
        // Generic reference-term bytecode (fjxh.5): compiled cells run in
        // the interp world; the strict backend refuses typed rather than
        // emit a silent identity (same contract as ApplyCapability).
        op @ (EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. }
        | EmirOp::VectorAllFinite(_)) => Err(BackendError::Lowering(format!(
            "reference-term bytecode op `{}` is not lowered by the strict Rust backend; run in the interp world",
            op.name()
        ))),
        // Certified-interval ops (8pjn) run in the interp world; the
        // strict backend refuses typed rather than emit a silent identity.
        op @ (EmirOp::IntervalCreate(..) | EmirOp::IntervalIntersect(..)) => {
            Err(BackendError::Lowering(format!(
                "interval op `{}` is not lowered by the strict Rust backend; run in the interp world",
                op.name()
            )))
        }
        // Option/Result value semantics (aj8d): the nine carrier ops lower
        // to native Rust `Option<T>` / `Result<T, E>` with payload slots
        // typed by dataflow (f64/i64/Vec<f64> per ScalarKind). `unwrap_or`
        // is the honesty gate — an eager by-value default, never a
        // panicking unwrap. A wrong carrier shape is a typed lowering
        // refusal (interp TypeConfusion parity), surfaced as
        // `BackendError`, never a Rust panic and never a silent scalar
        // shadow.
        EmirOp::OptionSome(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Option::<{}>::Some({})",
                tys.option(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::OptionNone => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!("Option::<{}>::None", tys.option(idx))))
        }
        EmirOp::OptionIsSome(carrier) => {
            expect_carrier(program, *carrier, false, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.is_some()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::OptionUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, false, op.name())?;
            // `.unwrap_or(default)` takes the default by value: eager and
            // unconditional, exactly the interp honesty-gate ordering.
            Ok(Expr::Raw(format!(
                "{}.unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultOk(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Ok({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultErr(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Err({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultIsOk(carrier) => {
            expect_carrier(program, *carrier, true, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.is_ok()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::ResultUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, true, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultErrorOf(carrier) => {
            expect_carrier(program, *carrier, true, op.name())?;
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let err_ty = tys.result_err(carrier.0);
            Ok(Expr::Raw(format!(
                "match {} {{ Ok(_) => Option::<{err_ty}>::None, Err(__opt_err) => Option::<{err_ty}>::Some(__opt_err) }}",
                render_expr(&operand(program, *carrier))
            )))
        }
    }
}

fn rust_format_template(template: &str) -> String {
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                output.push_str("{{");
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                output.push_str("}}");
            }
            '{' => {
                let mut field = String::new();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    field.push(next);
                }
                output.push('{');
                if let Some((_, spec)) = field.split_once(':') {
                    if let Some(precision) = spec
                        .strip_prefix('.')
                        .and_then(|value| value.strip_suffix('f'))
                    {
                        output.push_str(":.");
                        output.push_str(precision);
                    }
                }
                output.push('}');
            }
            _ => output.push(ch),
        }
    }
    output
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

