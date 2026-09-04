//! Option/Result carrier payload type resolution.

use super::*;

/// Payload Rust types of every Option/Result carrier register, resolved
/// by dataflow over the SSA program: producers (the payload of
/// `option_some`/`result_ok`/`result_err`) and consumers (the eager
/// default of the `unwrap_or` honesty gate; the error payload composed
/// by `result_error_of`) must agree. A conflict is a typed lowering
/// refusal (interp TypeConfusion parity), never a panic. A payload kind
/// the program never materializes (e.g. the Err slot of a `result_ok`
/// that is only `is_ok`-ed) defaults to the sibling slot so every
/// carrier register still gets one concrete Rust type.
pub(super) struct CarrierPayloadTypes {
    /// Option carrier register → payload Rust type.
    opt: HashMap<u32, String>,
    /// Result carrier register → Ok payload Rust type.
    ok: HashMap<u32, String>,
    /// Result carrier register → Err payload Rust type.
    err: HashMap<u32, String>,
}

impl CarrierPayloadTypes {
    pub(super) fn option(&self, register: u32) -> String {
        self.opt
            .get(&register)
            .cloned()
            .unwrap_or_else(|| "f64".to_string())
    }

    pub(super) fn result_ok(&self, register: u32) -> String {
        self.ok
            .get(&register)
            .cloned()
            .unwrap_or_else(|| "f64".to_string())
    }

    pub(super) fn result_err(&self, register: u32) -> String {
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
/// acyclic, so recursion terminates (nested parity).
pub(super) fn nested_operand_ty(
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

pub(super) fn carrier_payload_types(
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
    let payload_ty = |register: EmirValue, op: &EmirOp| -> Result<String, BackendError> {
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
                    changed |= bind(&mut tys.opt, carrier.0, payload_ty(*default, op)?, op)?;
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
pub(super) fn op_self_index(program: &EmirProgram, op: &EmirOp) -> Option<u32> {
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
pub(super) fn expect_carrier(
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
