use super::{CellClass, EmirValue, Value, EvalBudget, EvalFault, register, CompiledCell, evaluate_with_budget, run_guards, REFERENCE_DEPTH, ResultGuard};

pub(super) fn apply_capability(
    capability: &str,
    class: CellClass,
    args: &[EmirValue],
    registers: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    if class != CellClass::Pure {
        return Err(EvalFault::ProviderCallRequired {
            capability: capability.to_string(),
            args: args.len(),
        });
    }
    let values = args
        .iter()
        .map(|value| register(registers, *value).cloned())
        .collect::<Result<Vec<_>, _>>()?;
    crate::progress::apply(capability, &values, || {
        dispatch_capability(capability, &values, budget)
    })
}

pub(super) fn dispatch_capability(
    capability: &str,
    values: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    // A verified installed native binding is the fast path; it is preferred
    // when present because the install validated the capsule contract.
    if let Some(kernel) = crate::native_kernel::native_kernel(capability) {
        if !kernel.admits_arity(values.len()) {
            return Err(EvalFault::Arithmetic {
                op: "apply-capability",
                detail: "capability argument count does not match capsule contract",
            });
        }
        return (kernel.handler)(values).map_err(|code| EvalFault::CapabilityRefused {
            capability: capability.to_string(),
            code,
        });
    }
    // Fallback: the installed authored reference cell (Language Image
    // reference data). Nothing else matches — no feature-name lookup, no
    // std-cell registry.
    if let Some(cell) = crate::native_kernel::installed_reference_cell(capability) {
        return apply_reference_cell(capability, &cell, values, budget);
    }
    Err(EvalFault::Arithmetic {
        op: "apply-capability",
        detail: "no installed reference bytecode or native kernel",
    })
}

/// Execute an installed authored reference cell: arity and defaults from the
/// declared params, contract guards at the seam, generic bytecode with
/// budget accounting, and the optional post-body certificate guard.
pub(super) fn apply_reference_cell(
    capability: &str,
    cell: &CompiledCell,
    values: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    if !cell.admits_arity(values.len()) {
        return Err(EvalFault::Arithmetic {
            op: "apply-capability",
            detail: "capability argument count does not match capsule contract",
        });
    }
    let mut normalized;
    let values = if values.len() < cell.params.len() {
        normalized = Vec::with_capacity(cell.params.len());
        normalized.extend_from_slice(values);
        let first_default = cell.params.len() - cell.defaults.len();
        for default in &cell.defaults[values.len() - first_default..] {
            normalized.push(evaluate_with_budget(default, &normalized, &[], budget)?);
        }
        normalized.as_slice()
    } else {
        values
    };
    run_guards(capability, &cell.guards, values)?;
    let entered = REFERENCE_DEPTH.with(|depth| {
        let next = depth.get() + 1;
        if next > budget.max_capability_applications {
            None
        } else {
            depth.set(next);
            Some(())
        }
    });
    let Some(()) = entered else {
        return Err(EvalFault::BudgetExhausted {
            executed: budget.max_capability_applications,
        });
    };
    let evaluated = evaluate_with_budget(&cell.program, values, &[], budget);
    REFERENCE_DEPTH.with(|depth| depth.set(depth.get() - 1));
    let value = evaluated?;
    if let Some(guard) = cell.result_guard {
        enforce_result_guard(capability, guard, &value)?;
    }
    Ok(value)
}

pub(super) fn enforce_result_guard(
    capability: &str,
    guard: ResultGuard,
    value: &Value,
) -> Result<(), EvalFault> {
    let ResultGuard::AllZero { code } = guard;
    let violated = match value {
        Value::Vector(elements) => elements.iter().any(|element| *element != 0.0),
        Value::Matrix { data, .. } => data.iter().any(|element| *element != 0.0),
        Value::F64(element) => *element != 0.0,
        Value::I64(element) => *element != 0,
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: 0,
                op: "apply-capability",
            });
        }
    };
    if violated {
        Err(EvalFault::CapabilityRefused {
            capability: capability.to_string(),
            code: code.to_string(),
        })
    } else {
        Ok(())
    }
}

