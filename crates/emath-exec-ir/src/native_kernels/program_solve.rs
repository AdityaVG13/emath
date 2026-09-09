//! Domain-neutral program-carrier kernels for authored calculus goals.
//!
//! This module deliberately contains no model or method-domain names; capsules
//! own those meanings. The `program-expression-goal` router dispatches by
//! method label; composite Simpson integration and scalar Newton solving
//! execute from the authored reference programs installed for their
//! capabilities (see `language/spec/capabilities/numerics/program-solve.emath`).

use emath_core::Span;

use crate::{CellClass, EmirOp, EmirProgram, EmirValue};
use crate::interp::{EvalFault, Value, evaluate};
use crate::native_kernel::NativeKernel;

/// Descriptors to chain into `native_kernel::NATIVE_KERNELS`.
pub const KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "program-expression-goal",
        signature: "(Program,Vector<Float64>,Text,Sequence)->Float64",
        arity: 4,
        handler: program_expression_goal,
    },
];

/// Evaluate already-shaped goal arguments through the authored reference
/// program installed for `capability`. Refusals surface under their authored
/// codes; any other evaluation fault is reported verbatim, the same way a
/// direct capability invocation surfaces it.
fn apply_authored(capability: &str, args: &[Value]) -> Result<Value, String> {
    let mut ops = Vec::with_capacity(args.len() + 1);
    for index in 0..args.len() {
        let register = u16::try_from(index).map_err(|_| {
            "E-TYPE-012: program-expression-goal passes too many arguments".to_string()
        })?;
        ops.push((EmirOp::LoadInput(register), Span::default()));
    }
    let result = u16::try_from(args.len()).map_err(|_| {
        "E-TYPE-012: program-expression-goal passes too many arguments".to_string()
    })?;
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..args.len() as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
    match evaluate(
        &EmirProgram {
            ops,
            result: EmirValue(u32::from(result)),
            input_count: result,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        args,
        &[],
    ) {
        Ok(value) => Ok(value),
        Err(EvalFault::CapabilityRefused { code, .. }) => Err(code),
        Err(EvalFault::CarrierRefused { detail, .. }) => Err(detail),
        Err(fault) => Err(format!("{fault:?}")),
    }
}

/// Dispatch an authored program goal by method label. The last Sequence
/// packs the existing Simpson / Newton / dual extras; unknown labels
/// refuse typed. Method text is carrier data, not FeatureID dispatch.
fn program_expression_goal(args: &[Value]) -> Result<Value, String> {
    let [program, environment, method, extras] = args else {
        return Err(
            "E-TYPE-012: program-expression-goal expects Program, Vector<Float64>, Text, Sequence"
                .to_string(),
        );
    };
    if !matches!(program, Value::Program(_)) {
        return Err("E-TYPE-012: program-expression-goal expects a Program carrier".to_string());
    }
    if !matches!(environment, Value::Vector(_)) {
        return Err(
            "E-TYPE-012: program-expression-goal expects Vector<Float64> environment".to_string(),
        );
    }
    let Value::Text(method) = method else {
        return Err("E-TYPE-012: program-expression-goal method must be Text".to_string());
    };
    let extras = sequence_values(extras)?;
    match method.as_str() {
        "differentiate" | "forward-difference" => {
            let [slot] = extras.as_slice() else {
                return Err(
                    "E-TYPE-012: differentiate extras must be a single slot index".to_string(),
                );
            };
            super::calculus::program_forward_difference(&[
                program.clone(),
                environment.clone(),
                slot.clone(),
            ])
        }
        "integral" | "simpson" => {
            let [start, end, steps, loop_var] = extras.as_slice() else {
                return Err(
                    "E-TYPE-012: integral extras must be start, end, steps, loop-variable index"
                        .to_string(),
                );
            };
            apply_authored("std.capability.calculus.simpson-integral", &[
                program.clone(),
                environment.clone(),
                start.clone(),
                end.clone(),
                steps.clone(),
                loop_var.clone(),
            ])
        }
        "solve" | "newton" => {
            let [slot, tolerance, budget] = extras.as_slice() else {
                return Err(
                    "E-TYPE-012: solve extras must be slot index, tolerance, and iteration budget"
                        .to_string(),
                );
            };
            apply_authored("std.capability.calculus.scalar-solve", &[
                program.clone(),
                environment.clone(),
                slot.clone(),
                tolerance.clone(),
                budget.clone(),
            ])
        }
        _ => Err(format!(
            "E-TYPE-012: program-expression-goal does not admit method `{method}`"
        )),
    }
}

fn sequence_values(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Set(values) | Value::List(values) => Ok(values.clone()),
        Value::Record { type_name, fields } if type_name == "Sequence" => {
            Ok(fields.values().cloned().collect())
        }
        _ => Err("E-TYPE-012: program-expression-goal extras must be a Sequence".to_string()),
    }
}
