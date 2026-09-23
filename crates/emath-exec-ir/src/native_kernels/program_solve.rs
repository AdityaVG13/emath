//! Domain-neutral program-carrier kernels for authored calculus goals.
//!
//! This module deliberately contains no model or method-domain names; capsules
//! own those meanings. The `program-expression-goal` router dispatches by
//! method label; scalar differentiation executes through the program-carrier
//! calculus kernels. The former `"integral"`/`"solve"` arms dispatched to the
//! `std.capability.calculus.simpson-integral` / `scalar-solve` reference
//! programs, which are not part of the installed Language Image — dead
//! dispatch arms, removed.

use crate::interp::Value;
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

/// Dispatch an authored program goal by method label. The last Sequence
/// packs the method extras; unknown labels refuse typed. Method text is
/// carrier data, not `FeatureID` dispatch.
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
