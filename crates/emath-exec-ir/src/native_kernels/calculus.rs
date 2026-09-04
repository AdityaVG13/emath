//! Domain-neutral program-carrier autodiff kernels for calculus capability
//! capsules.
//!
//! This module deliberately does not register itself. `native_kernel.rs` can
//! integrate [`KERNELS`] into its immutable table without matching on a
//! mathematical feature name. The descriptor key and signature are the entire
//! ABI; aliases and FeatureIDs remain language data.
//!
//! Carrier semantics (ported from the orphaned interpreter AD machinery
//! `interp/dual.rs` and `interp/reverse.rs`, trimmed to the universal op
//! set and re-based on the ordinary `Value::Program` carrier):
//! - Forward mode carries `(primal, tangent)` duals through the program,
//!   seeding input slot `var_index` with tangent 1.0.
//! - Reverse mode records per-op primals on a Wengert tape, seeds the
//!   result adjoint with 1.0, and propagates adjoints in one backward
//!   traversal. The primal tape evaluates each op prefix through the
//!   public `crate::interp::evaluate` — one evaluator, no duplicated
//!   semantics — at O(n²) op cost, intended for capability-authored
//!   scalar programs (typical depth well under a few hundred ops).
//!
//! Error model: carrier/index violations refuse with `E-TYPE-012`
//! payloads; anything the underlying evaluation or an adjoint rule
//! refuses propagates that refusal detail verbatim (never a silent
//! zero, never an invented root). No-claim boundaries: state-carrying
//! programs, non-scalar program results, and ops without a ported dual
//! or adjoint rule (vector map/reduce carriers, control ops) refuse
//! typed instead of differentiating dishonestly. Rust-backend codegen
//! for these kernels is an explicit no-claim.

use std::collections::HashMap;

use crate::interp::{self, EvalFault, Value};
use crate::native_kernel::NativeKernel;
use crate::{BuiltinId, EmirOp, EmirProgram, EmirValue};

/// Capsule-backed kernels in stable descriptor order. Binding is by
/// `(kernel_id, signature)`; the FeatureID spelling lives in capsule data.
pub static KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "program-forward-difference",
        signature: "(Program,Vector<Float64>,I64)->Float64",
        arity: 3,
        handler: program_forward_difference,
    },
    NativeKernel {
        kernel_id: "program-reverse-gradient",
        signature: "(Program,Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 3,
        handler: program_reverse_gradient,
    },
];

/// Forward-mode tangent of `program` at `point` w.r.t. input slot
/// `var_index`: dual evaluation with tangent seed 1.0 on that slot.
fn program_forward_difference(args: &[Value]) -> Result<Value, String> {
    let [program, point, var_index] = args else {
        return Err(
            "E-TYPE-012: program-forward-difference expects (Program, Vector<Float64>, I64)"
                .to_string(),
        );
    };
    let program = program_carrier(program)?;
    let point = point_vector(point)?;
    let var_index = forward_slot(var_index, &program, point.len())?;
    let tangent = evaluate_dual(&program, &point, var_index)?;
    Ok(Value::F64(tangent))
}

/// Reverse-mode gradient of `program` at `point` w.r.t. the input slots in
/// `var_indices` (whole finite f64 encodings of slot numbers, because the
/// `Value` carrier has no integer vector). Returns one gradient per slot,
/// in slot order.
fn program_reverse_gradient(args: &[Value]) -> Result<Value, String> {
    let [program, point, var_indices] = args else {
        return Err(
            "E-TYPE-012: program-reverse-gradient expects (Program, Vector<Float64>, Vector<Float64>)"
                .to_string(),
        );
    };
    let program = program_carrier(program)?;
    let point = point_vector(point)?;
    let slots = gradient_slots(var_indices, &program)?;
    let grads = evaluate_reverse(&program, &point, &slots)?;
    Ok(Value::Vector(grads))
}

// ── carrier and slot validation ──────────────────────────────────────────

fn program_carrier(value: &Value) -> Result<EmirProgram, String> {
    match value {
        Value::Program(program) => Ok(program.clone()),
        _ => Err("E-TYPE-012: program-carrier kernel argument must be a Program value".to_string()),
    }
}

fn point_vector(value: &Value) -> Result<Vec<f64>, String> {
    match value {
        Value::Vector(point) => Ok(point.clone()),
        _ => Err("E-TYPE-012: program evaluation point must be a Vector<Float64>".to_string()),
    }
}

/// State-carrying programs are outside the carrier contract: the kernel ABI
/// passes no state, so a state read could only fabricate a zero.
fn reject_state(program: &EmirProgram) -> Result<(), String> {
    if program.state_count != 0 {
        return Err(
            "E-TYPE-012: program-carrier calculus kernels refuse state-carrying programs"
                .to_string(),
        );
    }
    Ok(())
}

fn forward_slot(value: &Value, program: &EmirProgram, point_len: usize) -> Result<u16, String> {
    let Value::I64(index) = value else {
        return Err("E-TYPE-012: forward-difference differentiation slot must be I64".to_string());
    };
    slot_in_range(*index, program, point_len)
}

fn gradient_slots(value: &Value, program: &EmirProgram) -> Result<Vec<u16>, String> {
    let Value::Vector(indices) = value else {
        return Err(
            "E-TYPE-012: reverse-gradient slot vector must be a Vector<Float64> of whole finite slots"
                .to_string(),
        );
    };
    if indices.is_empty() {
        return Err("E-TYPE-012: reverse-gradient slot vector must not be empty".to_string());
    }
    let mut slots = Vec::with_capacity(indices.len());
    for &encoded in indices {
        if !encoded.is_finite() || encoded.fract() != 0.0 {
            return Err(
                "E-TYPE-012: reverse-gradient slot vector elements must be whole finite input slots"
                    .to_string(),
            );
        }
        slots.push(slot_in_range(encoded as i64, program, usize::MAX)?);
    }
    Ok(slots)
}

fn slot_in_range(index: i64, program: &EmirProgram, point_len: usize) -> Result<u16, String> {
    if index < 0 || index > u16::MAX as i64 {
        return Err(format!(
            "E-TYPE-012: differentiation slot {index} is not a u16 input slot"
        ));
    }
    let slot = index as u16;
    if slot >= program.input_count {
        return Err(format!(
            "E-TYPE-012: differentiation slot {slot} is outside the program's {} inputs",
            program.input_count
        ));
    }
    if point_len != usize::MAX && slot as usize >= point_len {
        return Err(format!(
            "E-TYPE-012: differentiation slot {slot} has no evaluation point"
        ));
    }
    Ok(slot)
}

// ── forward mode: dual numbers ───────────────────────────────────────────

/// Dual number for forward-mode autodiff: (primal, tangent).
#[derive(Clone, Copy)]
struct Dual {
    primal: f64,
    tangent: f64,
}

/// Dual evaluation ported from `interp/dual.rs`: seed `var_index` with
/// tangent 1.0, carry `(primal, tangent)` pairs, return the result tangent.
fn evaluate_dual(program: &EmirProgram, point: &[f64], var_index: u16) -> Result<f64, String> {
    reject_state(program)?;
    let mut registers: Vec<Dual> = Vec::with_capacity(program.ops.len());
    let mut vec_regs: HashMap<usize, Vec<Dual>> = HashMap::new();
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
                let Some(&primal) = point.get(*idx as usize) else {
                    return Err(format!(
                        "E-TYPE-012: program reads input {} but the evaluation point has {} slots",
                        idx,
                        point.len()
                    ));
                };
                Dual {
                    primal,
                    tangent: if *idx == var_index { 1.0 } else { 0.0 },
                }
            }
            EmirOp::LoadState(_) => {
                return Err(
                    "E-TYPE-012: program-carrier calculus kernels refuse state-carrying programs"
                        .to_string(),
                );
            }
            EmirOp::F64Add(a, b) => {
                let a = dual_of(&registers, a)?;
                let b = dual_of(&registers, b)?;
                Dual {
                    primal: a.primal + b.primal,
                    tangent: a.tangent + b.tangent,
                }
            }
            EmirOp::F64Sub(a, b) => {
                let a = dual_of(&registers, a)?;
                let b = dual_of(&registers, b)?;
                Dual {
                    primal: a.primal - b.primal,
                    tangent: a.tangent - b.tangent,
                }
            }
            EmirOp::F64Mul(a, b) => {
                let a = dual_of(&registers, a)?;
                let b = dual_of(&registers, b)?;
                Dual {
                    primal: a.primal * b.primal,
                    tangent: a.tangent * b.primal + a.primal * b.tangent,
                }
            }
            EmirOp::F64Div(a, b) => {
                let a = dual_of(&registers, a)?;
                let b = dual_of(&registers, b)?;
                Dual {
                    primal: a.primal / b.primal,
                    tangent: (a.tangent * b.primal - a.primal * b.tangent) / (b.primal * b.primal),
                }
            }
            EmirOp::Neg(a) => {
                let a = dual_of(&registers, a)?;
                Dual {
                    primal: -a.primal,
                    tangent: -a.tangent,
                }
            }
            EmirOp::UnaryBuiltin(id, a) => {
                let val = dual_of(&registers, a)?;
                let Some((primal, tangent)) = dual_unary(*id, val.primal, val.tangent) else {
                    return Err(format!("E-TYPE-012: builtin {id:?} has no unary dual rule"));
                };
                Dual { primal, tangent }
            }
            EmirOp::BinaryBuiltin(id, a, b) => {
                let av = dual_of(&registers, a)?;
                let bv = dual_of(&registers, b)?;
                let Some((primal, tangent)) =
                    dual_binary(*id, av.primal, av.tangent, bv.primal, bv.tangent)
                else {
                    return Err(format!(
                        "E-TYPE-012: builtin {id:?} has no binary dual rule"
                    ));
                };
                Dual { primal, tangent }
            }
            EmirOp::F64Pow(a, b) => {
                let a = dual_of(&registers, a)?;
                let b = dual_of(&registers, b)?;
                let p = a.primal.powf(b.primal);
                if b.tangent == 0.0 {
                    // Constant exponent: d/dx [a^b] = b * a^(b-1) * a'.
                    // x^0 is identically 1 (IEEE 0^0 = 1); 0 * a^{-1} is
                    // 0*Inf=NaN at a=0, so keep the closed-form zero.
                    let tangent = if b.primal == 0.0 {
                        0.0
                    } else {
                        b.primal * a.primal.powf(b.primal - 1.0) * a.tangent
                    };
                    Dual { primal: p, tangent }
                } else {
                    // General: a^b * (b * a'/a + b' * ln(a))
                    Dual {
                        primal: p,
                        tangent: p * (b.primal * a.tangent / a.primal + b.tangent * a.primal.ln()),
                    }
                }
            }
            EmirOp::Select {
                condition: c,
                then_value: t,
                else_value: e,
            } => {
                let c = dual_of(&registers, c)?;
                let t = dual_of(&registers, t)?;
                let e = dual_of(&registers, e)?;
                if c.primal != 0.0 {
                    Dual {
                        primal: t.primal,
                        tangent: t.tangent,
                    }
                } else {
                    Dual {
                        primal: e.primal,
                        tangent: e.tangent,
                    }
                }
            }
            EmirOp::IsFinite(a) => {
                let a = dual_of(&registers, a)?;
                Dual {
                    primal: if a.primal.is_finite() { 1.0 } else { 0.0 },
                    tangent: 0.0,
                }
            }
            // Comparisons and boolean ops: piecewise-constant, tangent 0.0.
            EmirOp::Eq(a, b) => bool_dual(&registers, a, b, |l, r| l == r)?,
            EmirOp::Ne(a, b) => bool_dual(&registers, a, b, |l, r| l != r)?,
            EmirOp::Lt(a, b) => bool_dual(&registers, a, b, |l, r| l < r)?,
            EmirOp::Le(a, b) => bool_dual(&registers, a, b, |l, r| l <= r)?,
            EmirOp::Gt(a, b) => bool_dual(&registers, a, b, |l, r| l > r)?,
            EmirOp::Ge(a, b) => bool_dual(&registers, a, b, |l, r| l >= r)?,
            EmirOp::And(a, b) => bool_dual(&registers, a, b, |l, r| l != 0.0 && r != 0.0)?,
            EmirOp::Or(a, b) => bool_dual(&registers, a, b, |l, r| l != 0.0 || r != 0.0)?,
            EmirOp::Not(a) => {
                let a = dual_of(&registers, a)?;
                Dual {
                    primal: if a.primal == 0.0 { 1.0 } else { 0.0 },
                    tangent: 0.0,
                }
            }
            // Same truthiness as the scalar `bool_of` admission (`x != 0.0`),
            // so folding Imply/Iff does not change differentiate results.
            EmirOp::Imply(a, b) => bool_dual(&registers, a, b, |l, r| l == 0.0 || r != 0.0)?,
            EmirOp::Iff(a, b) => bool_dual(&registers, a, b, |l, r| (l != 0.0) == (r != 0.0))?,
            EmirOp::VectorCreate(elems) => {
                let idx = registers.len();
                let mut vec = Vec::with_capacity(elems.len());
                for elem in elems {
                    vec.push(dual_of(&registers, elem)?);
                }
                vec_regs.insert(idx, vec);
                Dual {
                    primal: 0.0,
                    tangent: 0.0,
                }
            }
            EmirOp::ApplyCapability {
                capability, args, ..
            } => {
                let Some(kernel) = crate::native_kernel::native_kernel(capability) else {
                    return Err(
                        "E-TYPE-012: nested capability has no forward dual rule".to_string()
                    );
                };
                match (kernel.kernel_id, args.as_slice()) {
                    ("pairwise-sum-products", [left, right]) => {
                        let left = dual_vec_of(&vec_regs, left)?;
                        let right = dual_vec_of(&vec_regs, right)?;
                        if left.len() != right.len() {
                            return Err(
                                "E-SHAPE-001: pairwise-sum-products requires equal vector lengths"
                                    .to_string(),
                            );
                        }
                        Dual {
                            primal: left
                                .iter()
                                .zip(right)
                                .map(|(left, right)| left.primal * right.primal)
                                .sum(),
                            tangent: left
                                .iter()
                                .zip(right)
                                .map(|(left, right)| {
                                    left.tangent * right.primal + left.primal * right.tangent
                                })
                                .sum(),
                        }
                    }
                    _ => {
                        return Err(
                            "E-TYPE-012: nested capability has no forward dual rule".to_string()
                        );
                    }
                }
            }
            _ => {
                return Err(
                    "E-TYPE-012: program op has no forward dual rule (vector/control carriers are outside the differentiable core)"
                        .to_string(),
                );
            }
        };
        registers.push(dual);
    }
    let result_slot = program.result.0 as usize;
    if vec_regs.contains_key(&result_slot) {
        return Err(
            "E-TYPE-012: program value result must be a scalar Float64 register".to_string(),
        );
    }
    registers
        .get(result_slot)
        .map(|dual| dual.tangent)
        .ok_or_else(|| {
            format!(
                "E-TYPE-012: program value result register %{} is unwritten",
                program.result.0
            )
        })
}

fn bool_dual(
    registers: &[Dual],
    a: &EmirValue,
    b: &EmirValue,
    keep: impl Fn(f64, f64) -> bool,
) -> Result<Dual, String> {
    let a = dual_of(registers, a)?;
    let b = dual_of(registers, b)?;
    Ok(Dual {
        primal: if keep(a.primal, b.primal) { 1.0 } else { 0.0 },
        tangent: 0.0,
    })
}

fn dual_of(registers: &[Dual], value: &EmirValue) -> Result<Dual, String> {
    registers
        .get(value.0 as usize)
        .copied()
        .ok_or_else(|| format!("E-TYPE-012: program reads unwritten register %{}", value.0))
}

fn dual_vec_of<'a>(
    registers: &'a HashMap<usize, Vec<Dual>>,
    value: &EmirValue,
) -> Result<&'a [Dual], String> {
    registers
        .get(&(value.0 as usize))
        .map(Vec::as_slice)
        .ok_or_else(|| {
            format!(
                "E-TYPE-012: program needs a vector dual at register %{}",
                value.0
            )
        })
}

/// Closed dual rule table for the scalar unary builtins. `None` marks the
/// binary-only codes, which cannot appear in a unary slot of honest
/// bytecode but still refuse typed instead of guessing.
fn dual_unary(id: BuiltinId, x: f64, a: f64) -> Option<(f64, f64)> {
    let f = match id {
        BuiltinId::Exp => x.exp(),
        BuiltinId::Ln => x.ln(),
        BuiltinId::Sqrt => x.sqrt(),
        BuiltinId::Sin => x.sin(),
        BuiltinId::Cos => x.cos(),
        BuiltinId::Tan => x.tan(),
        BuiltinId::Tanh => x.tanh(),
        BuiltinId::Abs => x.abs(),
        BuiltinId::Floor => x.floor(),
        BuiltinId::Ceil => x.ceil(),
        BuiltinId::Round => x.round(),
        BuiltinId::Sign => {
            if x == 0.0 {
                0.0
            } else {
                x.signum()
            }
        }
        BuiltinId::Log2 => x.log2(),
        BuiltinId::Log10 => x.log10(),
        BuiltinId::Sinh => x.sinh(),
        BuiltinId::Cosh => x.cosh(),
        BuiltinId::Atan => x.atan(),
        BuiltinId::Cbrt => x.cbrt(),
        BuiltinId::Recip => x.recip(),
        BuiltinId::Fract => x.fract(),
        BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max | BuiltinId::Atan2 | BuiltinId::Mod => {
            return None;
        }
    };
    let df = match id {
        BuiltinId::Exp => x.exp(),
        BuiltinId::Ln => 1.0 / x,
        BuiltinId::Recip => -1.0 / (x * x),
        BuiltinId::Sqrt => 0.5 / x.sqrt(),
        BuiltinId::Sin => x.cos(),
        BuiltinId::Cos => -x.sin(),
        BuiltinId::Tan => {
            let t = x.tan();
            1.0 + t * t
        }
        BuiltinId::Tanh => {
            let t = x.tanh();
            1.0 - t * t
        }
        BuiltinId::Abs => {
            if x == 0.0 {
                0.0
            } else {
                x.signum()
            }
        }
        BuiltinId::Floor | BuiltinId::Ceil | BuiltinId::Round | BuiltinId::Sign => 0.0,
        BuiltinId::Log2 => 1.0 / (x * std::f64::consts::LN_2),
        BuiltinId::Log10 => 1.0 / (x * std::f64::consts::LN_10),
        BuiltinId::Sinh => x.cosh(),
        BuiltinId::Cosh => x.sinh(),
        BuiltinId::Atan => 1.0 / (1.0 + x * x),
        BuiltinId::Cbrt => 1.0 / (3.0 * x.cbrt() * x.cbrt()),
        BuiltinId::Fract => 1.0,
        BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max | BuiltinId::Atan2 | BuiltinId::Mod => {
            return None;
        }
    };
    Some((f, a * df))
}

/// Closed dual rule table for the binary builtins. `None` marks the
/// unary-only codes.
fn dual_binary(id: BuiltinId, l: f64, tl: f64, r: f64, tr: f64) -> Option<(f64, f64)> {
    match id {
        BuiltinId::Hypot => {
            let f = l.hypot(r);
            Some((f, (tl * l + tr * r) / f))
        }
        // Deterministic tie rule: the left operand carries the tangent.
        BuiltinId::Min => Some((l.min(r), if l <= r { tl } else { tr })),
        BuiltinId::Max => Some((l.max(r), if l >= r { tl } else { tr })),
        BuiltinId::Atan2 => {
            let d = l * l + r * r;
            Some((l.atan2(r), (tl * r - tr * l) / d))
        }
        BuiltinId::Mod => Some((l % r, tl - tr * (l / r).trunc())),
        _ => None,
    }
}

// ── reverse mode: Wengert tape ───────────────────────────────────────────

/// Reverse-mode gradient ported from `interp/reverse.rs`: per-op primal
/// tape via the public evaluator, adjoint seeding at the result, one
/// backward traversal, gradient collection at the requested slots.
fn evaluate_reverse(
    program: &EmirProgram,
    point: &[f64],
    slots: &[u16],
) -> Result<Vec<f64>, String> {
    reject_state(program)?;
    let inputs: Vec<Value> = point.iter().map(|value| Value::F64(*value)).collect();

    // Forward pass: record primals (typed, including vectors). Each prefix
    // program re-runs its earlier ops through the one public evaluator, so
    // primal semantics are exactly interpreter semantics.
    let mut primals: Vec<Value> = Vec::with_capacity(program.ops.len());
    for (index, _) in program.ops.iter().enumerate() {
        let prefix = EmirProgram {
            ops: program.ops[..=index].to_vec(),
            result: EmirValue(index as u32),
            input_count: program.input_count,
            state_count: program.state_count,
            domain_obligations: program.domain_obligations.clone(),
        };
        match interp::evaluate(&prefix, &inputs, &[]) {
            Ok(primal) => primals.push(primal),
            Err(fault) => return Err(format!("program primal pass refused: {fault:?}")),
        }
    }

    let result_slot = program.result.0 as usize;
    if primals
        .get(result_slot)
        .and_then(Value::as_real_f64)
        .is_none()
    {
        return Err(
            "E-TYPE-012: program value result must be a scalar Float64 register".to_string(),
        );
    }

    // Backward pass: propagate adjoints.
    let n_regs = primals.len();
    let mut adjoints = vec![0.0_f64; n_regs];
    let mut vec_adjoints: HashMap<usize, Vec<f64>> = HashMap::new();
    adjoints[result_slot] = 1.0;
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
        )?;
    }

    Ok(slots
        .iter()
        .map(|&slot| input_adjoints[slot as usize])
        .collect())
}

/// Backward pass: propagate the adjoint from an op output to its inputs.
/// Ported from `interp/reverse.rs::backward_step`, trimmed to the
/// universal op set; ops without a ported adjoint rule refuse typed.
#[allow(clippy::too_many_arguments)]
fn backward_step(
    op: &EmirOp,
    idx: usize,
    adj: f64,
    primals: &[Value],
    adjoints: &mut [f64],
    vec_adjoints: &mut HashMap<usize, Vec<f64>>,
    input_adjoints: &mut [f64],
) -> Result<(), String> {
    let p = |v: &EmirValue| -> Result<f64, String> {
        primals
            .get(v.0 as usize)
            .and_then(Value::as_real_f64)
            .ok_or_else(|| {
                format!(
                    "E-TYPE-012: reverse pass needs a scalar primal at register %{}",
                    v.0
                )
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
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::ConstComplex(..)
        | EmirOp::ConstBool(_)
        | EmirOp::LoadState(_) => {}
        EmirOp::LoadInput(i) => {
            let ii = *i as usize;
            if ii < input_adjoints.len() {
                input_adjoints[ii] += adj;
            }
        }
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
            let Some(input_adj) = backward_unary(*id, primal_in, primal_out, adj) else {
                return Err(format!(
                    "E-TYPE-012: builtin {id:?} has no unary adjoint rule"
                ));
            };
            push_adj(adjoints, a, input_adj);
        }
        EmirOp::F64Pow(a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals.get(idx).and_then(Value::as_real_f64).unwrap_or(0.0);
            // d/da [a^b] = b * a^(b-1). x^0 is identically 1 (IEEE 0^0 = 1);
            // d/dx[x^1]|_0 = 1, not 0, so the a == 0 case is not skipped.
            if pb == 0.0 {
                // base adjoint is 0, including 0^0
            } else if pa != 0.0 {
                push_adj(adjoints, a, adj * primal_out * pb / pa);
            } else {
                push_adj(adjoints, a, adj * pb * pa.powf(pb - 1.0));
            }
            // d/db [a^b] = a^b * ln(a). IEEE ln of non-positive is NaN,
            // matching the dual's general path (do not silently zero).
            push_adj(adjoints, b, adj * primal_out * pa.ln());
        }
        EmirOp::BinaryBuiltin(id, a, b) => {
            let pa = p(a)?;
            let pb = p(b)?;
            let primal_out = primals.get(idx).and_then(Value::as_real_f64).unwrap_or(0.0);
            let Some((adj_a, adj_b)) = backward_binary(*id, pa, pb, primal_out, adj) else {
                return Err(format!(
                    "E-TYPE-012: builtin {id:?} has no binary adjoint rule"
                ));
            };
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
        EmirOp::ApplyCapability {
            capability, args, ..
        } => {
            let Some(kernel) = crate::native_kernel::native_kernel(capability) else {
                return Err(
                    "E-TYPE-012: nested capability has no reverse-mode adjoint rule".to_string(),
                );
            };
            match (kernel.kernel_id, args.as_slice()) {
                ("pairwise-sum-products", [left, right]) => {
                    let left_primal = primals
                        .get(left.0 as usize)
                        .and_then(|value| match value {
                            Value::Vector(values) => Some(values),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            "E-TYPE-012: pairwise-sum-products needs a vector left primal"
                                .to_string()
                        })?;
                    let right_primal = primals
                        .get(right.0 as usize)
                        .and_then(|value| match value {
                            Value::Vector(values) => Some(values),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            "E-TYPE-012: pairwise-sum-products needs a vector right primal"
                                .to_string()
                        })?;
                    if left_primal.len() != right_primal.len() {
                        return Err(
                            "E-SHAPE-001: pairwise-sum-products requires equal vector lengths"
                                .to_string(),
                        );
                    }
                    add_vec_adj(
                        vec_adjoints,
                        left.0 as usize,
                        right_primal.iter().map(|value| adj * value),
                    );
                    add_vec_adj(
                        vec_adjoints,
                        right.0 as usize,
                        left_primal.iter().map(|value| adj * value),
                    );
                }
                _ => {
                    return Err(
                        "E-TYPE-012: nested capability has no reverse-mode adjoint rule"
                            .to_string(),
                    );
                }
            }
        }
        // Comparisons and boolean ops are not differentiable — no
        // adjoint contribution.
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
        | EmirOp::Iff(..) => {}
        _ => {
            return Err(
                "E-TYPE-012: program op has no reverse-mode adjoint rule (vector/control carriers are outside the differentiable core)"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn add_vec_adj(
    vec_adjoints: &mut HashMap<usize, Vec<f64>>,
    register: usize,
    deltas: impl Iterator<Item = f64>,
) {
    let deltas: Vec<f64> = deltas.collect();
    let accumulated = vec_adjoints
        .entry(register)
        .or_insert_with(|| vec![0.0; deltas.len()]);
    for (value, delta) in accumulated.iter_mut().zip(deltas) {
        *value += delta;
    }
}

/// Closed adjoint rule table for the scalar unary builtins, dual-consistent
/// with [`dual_unary`]. `None` marks the binary-only codes.
fn backward_unary(id: BuiltinId, x: f64, f_out: f64, adj: f64) -> Option<f64> {
    let derivative = match id {
        BuiltinId::Exp => adj * f_out,
        BuiltinId::Ln => adj / x,
        BuiltinId::Sqrt => adj * 0.5 / f_out,
        BuiltinId::Sin => adj * x.cos(),
        BuiltinId::Cos => -adj * x.sin(),
        BuiltinId::Tan => adj * (1.0 + f_out * f_out),
        BuiltinId::Tanh => adj * (1.0 - f_out * f_out),
        BuiltinId::Abs => {
            if x == 0.0 {
                0.0
            } else {
                adj * x.signum()
            }
        }
        BuiltinId::Floor | BuiltinId::Ceil | BuiltinId::Round | BuiltinId::Sign => 0.0,
        BuiltinId::Log2 => adj / (x * std::f64::consts::LN_2),
        BuiltinId::Log10 => adj / (x * std::f64::consts::LN_10),
        BuiltinId::Sinh => adj * x.cosh(),
        BuiltinId::Cosh => adj * x.sinh(),
        BuiltinId::Atan => adj / (1.0 + x * x),
        BuiltinId::Cbrt => adj / (3.0 * f_out * f_out),
        BuiltinId::Recip => -adj * f_out * f_out,
        BuiltinId::Fract => adj,
        BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max | BuiltinId::Atan2 | BuiltinId::Mod => {
            return None;
        }
    };
    Some(derivative)
}

/// Closed adjoint rule table for the binary builtins, dual-consistent with
/// [`dual_binary`]. `None` marks the unary-only codes.
fn backward_binary(id: BuiltinId, l: f64, r: f64, f_out: f64, adj: f64) -> Option<(f64, f64)> {
    match id {
        BuiltinId::Hypot => Some((adj * l / f_out, adj * r / f_out)),
        // Deterministic tie rule, matching [`dual_binary`]: left carries.
        BuiltinId::Min => Some((
            if l <= r { adj } else { 0.0 },
            if r < l { adj } else { 0.0 },
        )),
        BuiltinId::Max => Some((
            if l >= r { adj } else { 0.0 },
            if r > l { adj } else { 0.0 },
        )),
        BuiltinId::Atan2 => {
            let d = l * l + r * r;
            Some((adj * r / d, -adj * l / d))
        }
        BuiltinId::Mod => Some((adj, -adj * (l / r).trunc())),
        _ => None,
    }
}
