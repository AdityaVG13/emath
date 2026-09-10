//! Codegen for capability kernels whose generated form is more than one
//! rt call: the program-carrier forward-difference dual walk, the einsum
//! contraction (operand-carrier conversion + static output rank), and
//! the dual-lane Reed-Solomon encode. Each render mirrors the interp
//! handler's semantics and refusal payloads op-for-op — generated code
//! and the interpreter must agree byte-for-byte on the same admitted
//! program (the cross-engine conformance claim the examples carry).
//!
//! No-claim boundary: the dual walk covers exactly the op set with a
//! ported forward dual rule (`native_kernels/calculus.rs`); anything
//! else refuses `None` and the caller reports the typed unsupported
//! carriers, matching the kernel module's "codegen is an explicit
//! no-claim" for unported rules.

use std::collections::HashMap;

use emath_exec_ir::BuiltinId;
use emath_exec_ir::{EmirOp, EmirProgram, EmirValue};

use super::*;

/// Forward-mode tangent of a `ProgramLiteral` body at the caller's point,
/// w.r.t. a constant differentiation slot. Mirrors
/// `native_kernels/calculus.rs::evaluate_dual`: (primal, tangent) pairs
/// per register, tangent seed 1.0 on the differentiation slot, scalar
/// Float64 result.
pub(crate) fn forward_difference_expr(
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Option<Expr> {
    let [program_value, point, slot] = args else {
        return None;
    };
    let EmirOp::ProgramLiteral {
        body,
        captures,
        vector_input,
    } = &program.ops.get(program_value.0 as usize)?.0
    else {
        return None;
    };
    // The kernel ABI passes no captures and no vector-input packing
    // (`program_carrier` in calculus.rs refuses both).
    if !captures.is_empty() || *vector_input {
        return None;
    }
    if !matches!(kind_at(kinds, *point), ValueKind::Vector(_)) {
        return None;
    }
    // State-carrying programs are outside the carrier contract.
    if body.state_count != 0 {
        return None;
    }
    let EmirOp::ConstI64(slot_index) = &program.ops.get(slot.0 as usize)?.0 else {
        return None;
    };
    if *slot_index < 0 || *slot_index > u16::MAX as i64 {
        return None;
    }
    let slot = *slot_index as u16;
    if slot >= body.input_count {
        return None;
    }
    let point_expr = render_expr(&operand_ref(program, *point));

    // registers: per-op (primal, tangent) expression strings; vec
    // registers (VectorCreate outputs) live separately as
    // Vec<(f64, f64)> expression names, mirroring `vec_regs`.
    let mut regs: Vec<Option<(String, String)>> = vec![None; body.ops.len()];
    let mut vecs: HashMap<usize, String> = HashMap::new();
    let mut code = format!("let __fd_point = ({point_expr}).clone(); ");
    for (index, (op, _)) in body.ops.iter().enumerate() {
        let mut pre = String::new();
        let (primal, tangent) =
            dual_step(op, index, &regs, &mut vecs, &mut pre, slot, &point_binding_name())?;
        code.push_str(&pre);
        code.push_str(&format!(
            "let __fd_p{index} = {primal}; let __fd_t{index} = {tangent}; "
        ));
        regs[index] = Some((format!("__fd_p{index}"), format!("__fd_t{index}")));
    }
    let result_slot = body.result.0 as usize;
    if vecs.contains_key(&result_slot) {
        // "program value result must be a scalar Float64 register"
        return None;
    }
    let (_, tangent) = regs.get(result_slot).cloned().flatten()?;
    Some(map_runtime_result(format!(
        "(|| -> Result<f64, String> {{ {code} Ok({tangent}) }})()"
    )))
}

fn point_binding_name() -> &'static str {
    "__fd_point"
}

fn scalar(regs: &[Option<(String, String)>], value: &EmirValue) -> Option<(String, String)> {
    regs.get(value.0 as usize).cloned().flatten()
}

/// One op's dual rule: optional pre-statements (fault checks), the
/// primal expression, and the tangent expression. The tangent may
/// reference the op's own primal through `__fd_p{index}` (the caller
/// binds it before the tangent let).
#[allow(clippy::too_many_lines)]
fn dual_step(
    op: &EmirOp,
    index: usize,
    regs: &[Option<(String, String)>],
    vecs: &mut HashMap<usize, String>,
    pre: &mut String,
    slot: u16,
    point: &str,
) -> Option<(String, String)> {
    let own_primal = format!("__fd_p{index}");
    match op {
        EmirOp::ConstF64(bits) => Some((format!("f64::from_bits({bits})"), "0.0".into())),
        EmirOp::ConstI64(value) => Some((format!("({value}) as f64"), "0.0".into())),
        EmirOp::ConstBool(value) => Some((
            format!("if {value} {{ 1.0 }} else {{ 0.0 }}"),
            "0.0".into(),
        )),
        EmirOp::LoadInput(idx) => {
            let i = *idx as usize;
            Some((
                format!(
                    "{point}.get({i}).copied().ok_or_else(|| format!(\"E-TYPE-012: program reads input {i} but the evaluation point has {{}} slots\", {point}.len()))?"
                ),
                format!("if {i} == {slot} {{ 1.0 }} else {{ 0.0 }}"),
            ))
        }
        EmirOp::LoadState(_) => None,
        EmirOp::F64Add(a, b) => {
            let (pa, ta) = scalar(regs, a)?;
            let (pb, tb) = scalar(regs, b)?;
            Some((
                format!("({pa}) + ({pb})"),
                format!("({ta}) + ({tb})"),
            ))
        }
        EmirOp::F64Sub(a, b) => {
            let (pa, ta) = scalar(regs, a)?;
            let (pb, tb) = scalar(regs, b)?;
            Some((
                format!("({pa}) - ({pb})"),
                format!("({ta}) - ({tb})"),
            ))
        }
        EmirOp::F64Mul(a, b) => {
            let (pa, ta) = scalar(regs, a)?;
            let (pb, tb) = scalar(regs, b)?;
            Some((
                format!("({pa}) * ({pb})"),
                format!("(({ta}) * ({pb})) + (({pa}) * ({tb}))"),
            ))
        }
        EmirOp::F64Div(a, b) => {
            let (pa, ta) = scalar(regs, a)?;
            let (pb, tb) = scalar(regs, b)?;
            Some((
                format!("({pa}) / ({pb})"),
                format!(
                    "((({ta}) * ({pb})) - (({pa}) * ({tb}))) / (({pb}) * ({pb}))"
                ),
            ))
        }
        EmirOp::F64Pow(a, b) => {
            let (pa, ta) = scalar(regs, a)?;
            let (pb, tb) = scalar(regs, b)?;
            Some((
                format!("({pa}).powf(({pb}))"),
                format!(
                    "if ({tb}) == 0.0 {{ if ({pb}) == 0.0 {{ 0.0 }} else {{ ({pb}) * ({pa}).powf((({pb})) - 1.0) * ({ta}) }} }} else {{ {own_primal} * (((({pb}) * ({ta})) / ({pa})) + (({tb}) * ({pa}).ln()))) }}"
                ),
            ))
        }
        EmirOp::Neg(a) => {
            let (pa, ta) = scalar(regs, a)?;
            Some((format!("-({pa})"), format!("-({ta})")))
        }
        EmirOp::UnaryBuiltin(id, a) => {
            let (pa, ta) = scalar(regs, a)?;
            let x = format!("({pa})");
            let f = format!("({x}).{}()", unary_method(*id)?);
            let df = unary_dual(*id, &x)?;
            Some((f, format!("({ta}) * ({df})")))
        }
        EmirOp::BinaryBuiltin(id, l, r) => {
            let (pl, tl) = scalar(regs, l)?;
            let (pr, tr) = scalar(regs, r)?;
            let (l, r) = (format!("({pl})"), format!("({pr})"));
            let f = binary_builtin_expr(*id, &l, &r)?;
            let t = match id {
                BuiltinId::Hypot => format!(
                    "((({tl}) * ({l})) + (({tr}) * ({r}))) / {own_primal}"
                ),
                BuiltinId::Min => {
                    format!("if ({l}) <= ({r}) {{ ({tl}) }} else {{ ({tr}) }}")
                }
                BuiltinId::Max => {
                    format!("if ({l}) >= ({r}) {{ ({tl}) }} else {{ ({tr}) }}")
                }
                BuiltinId::Atan2 => format!(
                    "((({tl}) * ({r})) - (({tr}) * ({l}))) / ((({l}) * ({l})) + (({r}) * ({r})))"
                ),
                BuiltinId::Mod => format!(
                    "({tl}) - (({tr}) * (((({l}) / ({r}))).trunc()))"
                ),
                _ => return None,
            };
            Some((f, t))
        }
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: e,
        } => {
            let (pc, _) = scalar(regs, c)?;
            let (pt, tt) = scalar(regs, t)?;
            let (pe, te) = scalar(regs, e)?;
            Some((
                format!("if ({pc}) != 0.0 {{ ({pt}) }} else {{ ({pe}) }}"),
                format!("if ({pc}) != 0.0 {{ ({tt}) }} else {{ ({te}) }}"),
            ))
        }
        EmirOp::IsFinite(a) => {
            let (pa, _) = scalar(regs, a)?;
            Some((
                format!("if ({pa}).is_finite() {{ 1.0 }} else {{ 0.0 }}"),
                "0.0".into(),
            ))
        }
        EmirOp::Eq(a, b) => bool_dual(regs, a, b, "=="),
        EmirOp::Ne(a, b) => bool_dual(regs, a, b, "!="),
        EmirOp::Lt(a, b) => bool_dual(regs, a, b, "<"),
        EmirOp::Le(a, b) => bool_dual(regs, a, b, "<="),
        EmirOp::Gt(a, b) => bool_dual(regs, a, b, ">"),
        EmirOp::Ge(a, b) => bool_dual(regs, a, b, ">="),
        EmirOp::And(a, b) => Some((
            format!(
                "if (({pl}) != 0.0) && (({pr}) != 0.0) {{ 1.0 }} else {{ 0.0 }}",
                pl = scalar(regs, a)?.0,
                pr = scalar(regs, b)?.0,
            ),
            "0.0".into(),
        )),
        EmirOp::Or(a, b) => Some((
            format!(
                "if (({pl}) != 0.0) || (({pr}) != 0.0) {{ 1.0 }} else {{ 0.0 }}",
                pl = scalar(regs, a)?.0,
                pr = scalar(regs, b)?.0,
            ),
            "0.0".into(),
        )),
        EmirOp::Imply(a, b) => Some((
            format!(
                "if (({pl}) == 0.0) || (({pr}) != 0.0) {{ 1.0 }} else {{ 0.0 }}",
                pl = scalar(regs, a)?.0,
                pr = scalar(regs, b)?.0,
            ),
            "0.0".into(),
        )),
        EmirOp::Iff(a, b) => Some((
            format!(
                "if (({pl}) != 0.0) == (({pr}) != 0.0) {{ 1.0 }} else {{ 0.0 }}",
                pl = scalar(regs, a)?.0,
                pr = scalar(regs, b)?.0,
            ),
            "0.0".into(),
        )),
        EmirOp::Not(a) => {
            let (pa, _) = scalar(regs, a)?;
            Some((
                format!("if ({pa}) == 0.0 {{ 1.0 }} else {{ 0.0 }}"),
                "0.0".into(),
            ))
        }
        EmirOp::VectorCreate(elems) => {
            let mut items = Vec::with_capacity(elems.len());
            for elem in elems {
                let (p, t) = scalar(regs, elem)?;
                items.push(format!("({p}, {t})"));
            }
            let name = format!("__fd_v{index}");
            pre.push_str(&format!("let {name} = vec![{}]; ", items.join(", ")));
            vecs.insert(index, name);
            // The op also writes a scalar (0, 0) register, like the
            // interp's `registers.push(dual)` for vector carriers.
            Some(("0.0".into(), "0.0".into()))
        }
        EmirOp::ApplyCapability {
            capability, args, ..
        } => {
            if capability != "std.capability.geometry.inner-product" {
                return None;
            }
            let [l, r] = args.as_slice() else {
                return None;
            };
            let l = vecs.get(&(l.0 as usize))?;
            let r = vecs.get(&(r.0 as usize))?;
            pre.push_str(&format!(
                "if ({l}).len() != ({r}).len() {{ return Err(String::from(\"E-SHAPE-001: std.capability.geometry.inner-product requires equal vector lengths\")); }}"
            ));
            Some((
                format!(
                    "({l}).iter().zip(({r}).iter()).map(|(l, r)| l.0 * r.0).sum::<f64>()"
                ),
                format!(
                    "({l}).iter().zip(({r}).iter()).map(|(l, r)| (l.1 * r.0) + (l.0 * r.1)).sum::<f64>()"
                ),
            ))
        }
        // No ported forward dual rule (constants beyond numeric scalars,
        // vector/control carriers, nested programs): refuse typed.
        _ => None,
    }
}

/// Comparison/boolean ops are piecewise-constant: primal 1.0/0.0 by the
/// same truthiness as the scalar admission, tangent exactly 0.0
/// (`bool_dual` in calculus.rs).
fn bool_dual(
    regs: &[Option<(String, String)>],
    a: &EmirValue,
    b: &EmirValue,
    keep: &str,
) -> Option<(String, String)> {
    let (pa, _) = scalar(regs, a)?;
    let (pb, _) = scalar(regs, b)?;
    Some((
        format!("if ({pa}) {keep} ({pb}) {{ 1.0 }} else {{ 0.0 }}"),
        "0.0".into(),
    ))
}

fn unary_method(id: BuiltinId) -> Option<&'static str> {
    match id {
        BuiltinId::Exp => Some("exp"),
        BuiltinId::Ln => Some("ln"),
        BuiltinId::Sqrt => Some("sqrt"),
        BuiltinId::Sin => Some("sin"),
        BuiltinId::Cos => Some("cos"),
        BuiltinId::Tan => Some("tan"),
        BuiltinId::Tanh => Some("tanh"),
        BuiltinId::Abs => Some("abs"),
        BuiltinId::Floor => Some("floor"),
        BuiltinId::Ceil => Some("ceil"),
        BuiltinId::Round => Some("round"),
        BuiltinId::Log2 => Some("log2"),
        BuiltinId::Log10 => Some("log10"),
        BuiltinId::Sinh => Some("sinh"),
        BuiltinId::Cosh => Some("cosh"),
        BuiltinId::Atan => Some("atan"),
        BuiltinId::Cbrt => Some("cbrt"),
        BuiltinId::Recip => Some("recip"),
        BuiltinId::Fract => Some("fract"),
        BuiltinId::Sign | BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max
        | BuiltinId::Atan2 | BuiltinId::Mod => None,
    }
}

/// Closed dual derivative expressions in the input primal `x`, ported
/// from `dual_unary` in calculus.rs. `Sign` keeps its if-form primal,
/// so its derivative is the zero constant.
fn unary_dual(id: BuiltinId, x: &str) -> Option<String> {
    Some(match id {
        BuiltinId::Exp => format!("({x}).exp()"),
        BuiltinId::Ln => format!("1.0 / ({x})"),
        BuiltinId::Recip => format!("-1.0 / (({x}) * ({x}))"),
        BuiltinId::Sqrt => format!("0.5 / ({x}).sqrt()"),
        BuiltinId::Sin => format!("({x}).cos()"),
        BuiltinId::Cos => format!("-({x}).sin()"),
        BuiltinId::Tan => format!("1.0 + ({x}).tan() * ({x}).tan()"),
        BuiltinId::Tanh => format!("1.0 - ({x}).tanh() * ({x}).tanh()"),
        BuiltinId::Abs => format!("if ({x}) == 0.0 {{ 0.0 }} else {{ ({x}).signum() }}"),
        BuiltinId::Floor | BuiltinId::Ceil | BuiltinId::Round | BuiltinId::Sign => "0.0".into(),
        BuiltinId::Log2 => format!("1.0 / (({x}) * std::f64::consts::LN_2)"),
        BuiltinId::Log10 => format!("1.0 / (({x}) * std::f64::consts::LN_10)"),
        BuiltinId::Sinh => format!("({x}).cosh()"),
        BuiltinId::Cosh => format!("({x}).sinh()"),
        BuiltinId::Atan => format!("1.0 / (1.0 + ({x}) * ({x}))"),
        BuiltinId::Cbrt => format!("1.0 / (3.0 * ({x}).cbrt() * ({x}).cbrt())"),
        BuiltinId::Fract => "1.0".into(),
        BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max | BuiltinId::Atan2
        | BuiltinId::Mod => return None,
    })
}

fn binary_builtin_expr(id: BuiltinId, l: &str, r: &str) -> Option<String> {
    Some(match id {
        BuiltinId::Hypot => format!("({l}).hypot(({r}))"),
        BuiltinId::Min => format!("({l}).min(({r}))"),
        BuiltinId::Max => format!("({l}).max(({r}))"),
        BuiltinId::Atan2 => format!("({l}).atan2(({r}))"),
        BuiltinId::Mod => format!("({l}) % ({r})"),
        _ => return None,
    })
}

/// Einsum contraction over converted operand carriers. The subscript
/// spec must be a constant (the output carrier's rank is a codegen-time
/// fact); operands must be literal Sequences (List/Set/Record) of
/// Vector/Matrix/Tensor carriers — the same carrier set
/// `einsum_operand_of` accepts in the interp.
pub(crate) fn einsum_contract_expr(
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Option<Expr> {
    let [subscripts, operands] = args else {
        return None;
    };
    let EmirOp::ConstText(spec) = &program.ops.get(subscripts.0 as usize)?.0 else {
        return None;
    };
    if spec.contains('.') {
        // Ellipsis broadcasting has no ported codegen rule.
        return None;
    }
    let elements: Vec<EmirValue> = match &program.ops.get(operands.0 as usize)?.0 {
        EmirOp::ListCreate(elements) => elements.clone(),
        EmirOp::SetCreate { elements, .. } => elements.clone(),
        EmirOp::RecordCreate { type_name, fields } if type_name == "Sequence" => {
            fields.iter().map(|(_, value)| *value).collect()
        }
        _ => return None,
    };
    let mut tensors = Vec::with_capacity(elements.len());
    for element in &elements {
        let converted = element_tensor_expr(*element, program, kinds, 4)?;
        tensors.push(format!(
            "emath_rt::EinsumIn::einsum_operand(&{converted})"
        ));
    }
    let rank = einsum_output_rank(spec);
    let out = match rank {
        0 => "Ok::<_, String>(__e.1.first().copied().unwrap_or(0.0))".to_string(),
        1 => "Ok::<_, String>(__e.1)".to_string(),
        2 => "Ok::<_, String>(emath_rt::Matrix::new(__e.0[0], __e.0[1], __e.1).map_err(String::from)?)"
            .to_string(),
        _ => "Ok::<_, String>(emath_rt::Tensor { shape: __e.0, data: __e.1 })".to_string(),
    };
    // Same typed refusals as `einsum_contract` in the interp: E-EINSUM-001
    // for subscript/precondition failures, E-EINSUM-002 for an index
    // outside an operand axis.
    Some(map_runtime_result(format!(
        "{{ let __e = emath_rt::einsum_checked({spec:?}, &[{}]).map_err(|error| match error {{ emath_rt::EinsumError::Arithmetic(detail) => format!(\"E-EINSUM-001: {{detail}}\"), emath_rt::EinsumError::IndexOutOfBounds {{ index, len }} => format!(\"E-EINSUM-002: einsum index {{index}} is outside 0..{{len}}\") }})?; {out} }}",
        tensors.join(", ")
    )))
}

/// One einsum operand converted to an `emath_rt::Tensor` expression.
/// The carrier set matches the interp's `einsum_operand_of`
/// (Vector/Matrix/Tensor); a chained einsum result derives as
/// `ValueKind::Other` in the kinds table (the image spells the output
/// `Tensor` for every rank), so its Rust carrier resolves through its
/// own static rank, recursively, depth-capped.
pub(crate) fn element_tensor_expr(
    value: EmirValue,
    program: &EmirProgram,
    kinds: &[ValueKind],
    depth: u8,
) -> Option<String> {
    let expr = render_expr(&operand(program, value));
    match kind_at(kinds, value) {
        ValueKind::Vector(_) => Some(format!(
            "emath_rt::Tensor {{ shape: vec![({expr}).len()], data: ({expr}).clone() }}"
        )),
        ValueKind::Matrix(_) => Some(format!(
            "emath_rt::Tensor {{ shape: vec![({expr}).rows(), ({expr}).cols()], data: ({expr}).as_slice().to_vec() }}"
        )),
        ValueKind::Tensor => Some(format!("({expr}).clone()")),
        ValueKind::Other if depth > 0 => {
            let EmirOp::ApplyCapability {
                capability, args, ..
            } = &program.ops.get(value.0 as usize)?.0
            else {
                return None;
            };
            let binding = emath_exec_ir::native_kernel::verified_kernel_binding(capability)
                .ok()?;
            if binding.kernel_id != "einsum-contract" {
                return None;
            }
            let [sub, _operands] = args.as_slice() else {
                return None;
            };
            let EmirOp::ConstText(spec) = &program.ops.get(sub.0 as usize)?.0 else {
                return None;
            };
            match einsum_output_rank(spec) {
                0 => Some(format!(
                    "emath_rt::Tensor {{ shape: vec![], data: vec![({expr})] }}"
                )),
                1 => Some(format!(
                    "emath_rt::Tensor {{ shape: vec![({expr}).len()], data: ({expr}).clone() }}"
                )),
                2 => Some(format!(
                    "emath_rt::Tensor {{ shape: vec![({expr}).rows(), ({expr}).cols()], data: ({expr}).as_slice().to_vec() }}"
                )),
                _ => Some(format!("({expr}).clone()")),
            }
        }
        _ => None,
    }
}

/// Output rank of an einsum subscript string, mirroring
/// `emath_rt::einsum_output_rank`: explicit mode counts the output
/// letters after `->`; implicit mode emits the letters that appear
/// exactly once across the spec.
pub(crate) fn einsum_output_rank(spec: &str) -> usize {    match spec.split_once("->") {
        Some((_, output)) => output.chars().filter(|c| c.is_alphabetic()).count(),
        None => {
            let mut letters: Vec<char> =
                spec.chars().filter(|c| c.is_alphabetic()).collect();
            letters.sort_unstable();
            let mut rank = 0;
            let mut runs = letters.len();
            let mut index = 0;
            while index < runs {
                let mut run = 1;
                while index + run < letters.len() && letters[index + run] == letters[index] {
                    run += 1;
                }
                if run == 1 {
                    rank += 1;
                }
                index += run;
            }
            rank
        }
    }
}

/// Reed-Solomon encode over the two width lanes the interp dispatches by
/// value kind: a BigInt modulus rides `big_rs_encode_checked`
/// (`Vec<UBig>` result), an i64 modulus rides `rs_encode_checked`
/// (`Vec<f64>` result). The coefficient vector stays Float64 in both
/// lanes (the authored polynomial's coefficients).
pub(crate) fn rs_encode_expr(
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Option<Expr> {
    let [coefficients, length, modulus] = args else {
        return None;
    };
    if !matches!(kind_at(kinds, *coefficients), ValueKind::Vector(_)) {
        return None;
    }
    if !matches!(kind_at(kinds, *length), ValueKind::I64) {
        return None;
    }
    let coeffs = render_expr(&operand(program, *coefficients));
    let n = render_expr(&operand(program, *length));
    let modulus_expr = render_expr(&operand(program, *modulus));
    let call = match kind_at(kinds, *modulus) {
        ValueKind::BigInt => format!(
            "emath_rt::big_rs_encode_checked(&({coeffs}), ({n}), &({modulus_expr}))"
        ),
        ValueKind::I64 => {
            format!("emath_rt::rs_encode_checked(&({coeffs}), ({n}), ({modulus_expr}))")
        }
        _ => return None,
    };
    Some(map_runtime_result(call))
}
