//! Deterministic reference VM for the universal executable instruction set.

use std::cell::Cell;
use std::collections::BTreeMap;

use crate::term_compile::{CompiledCell, ResultGuard, run_guards};
use crate::{
    BuiltinId, CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, FoldCombine, ReduceId,
    VectorScalarOp,
};

mod helpers;
mod value;

use helpers::{register, bool_of, i64_of, f64_of, eq_ne, vector_of, tensor_of, matrix_of, eval_tensor_slice};
pub use value::{EvalFault, ProgramValue, Value, format_f64};

pub fn evaluate(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    evaluate_with_budget(program, inputs, state, EvalBudget::default())
}

pub fn evaluate_with_budget(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    evaluate_frames(
        program,
        ValueFrame::direct(inputs),
        ValueFrame::direct(state),
        budget,
    )
}

#[derive(Clone, Copy)]
struct ValueFrame<'a> {
    values: &'a [Value],
    slots: Option<&'a [EmirValue]>,
}

impl<'a> ValueFrame<'a> {
    fn direct(values: &'a [Value]) -> Self {
        Self {
            values,
            slots: None,
        }
    }
    fn mapped(values: &'a [Value], slots: &'a [EmirValue]) -> Result<Self, EvalFault> {
        for slot in slots {
            register(values, *slot)?;
        }
        Ok(Self {
            values,
            slots: Some(slots),
        })
    }
    fn get(self, index: usize) -> Option<&'a Value> {
        let index = match self.slots {
            Some(slots) => slots.get(index)?.0 as usize,
            None => index,
        };
        self.values.get(index)
    }
    fn to_vec(self) -> Vec<Value> {
        match self.slots {
            // mapped() validates every immutable slot before this frame is used.
            Some(slots) => slots
                .iter()
                .map(|slot| self.values[slot.0 as usize].clone())
                .collect(),
            None => self.values.to_vec(),
        }
    }
}

fn evaluate_frames(
    program: &EmirProgram,
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    evaluate_frames_in(program, program, inputs, state, budget)
}

fn evaluate_frames_in(
    self_program: &EmirProgram,
    program: &EmirProgram,
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    let _fuel = EvaluationFuel::enter(budget);
    let mut registers = Vec::with_capacity(program.ops.len());
    let mut applications = 0_u32;
    for (step, (op, span)) in program.ops.iter().enumerate() {
        let _site = crate::progress::site(*span, step);
        let executed = u32::try_from(step).unwrap_or(u32::MAX);
        if executed >= budget.max_steps {
            return Err(EvalFault::BudgetExhausted { executed });
        }
        if matches!(op, EmirOp::ApplyCapability { .. }) {
            applications = applications.saturating_add(1);
            if applications > budget.max_capability_applications {
                return Err(EvalFault::BudgetExhausted { executed });
            }
        }
        EvaluationFuel::charge(matches!(op, EmirOp::ApplyCapability { .. }))?;
        registers.push(eval_op(
            self_program,
            op,
            &registers,
            inputs,
            state,
            budget,
        )?);
    }
    register(&registers, program.result).cloned()
}

// Nested authored calls and loops share the outer evaluation budget.
#[derive(Clone, Copy)]
struct Fuel {
    steps: u32,
    applications: u32,
    executed: u32,
}
thread_local! { static FUEL: Cell<Option<Fuel>> = const { Cell::new(None) }; }
struct EvaluationFuel(bool);
impl EvaluationFuel {
    fn enter(budget: EvalBudget) -> Self {
        Self(FUEL.with(|slot| {
            if slot.get().is_some() {
                false
            } else {
                slot.set(Some(Fuel {
                    steps: budget.max_steps,
                    applications: budget.max_capability_applications,
                    executed: 0,
                }));
                true
            }
        }))
    }
    fn charge(application: bool) -> Result<(), EvalFault> {
        FUEL.with(|slot| {
            let Some(mut fuel) = slot.get() else {
                return Ok(());
            };
            if fuel.steps == 0 || (application && fuel.applications == 0) {
                return Err(EvalFault::BudgetExhausted {
                    executed: fuel.executed,
                });
            }
            fuel.steps -= 1;
            fuel.applications -= u32::from(application);
            fuel.executed += 1;
            slot.set(Some(fuel));
            Ok(())
        })
    }
}
impl Drop for EvaluationFuel {
    fn drop(&mut self) {
        if self.0 {
            FUEL.with(|slot| slot.set(None));
        }
    }
}

pub fn evaluate_f64(
    program: &EmirProgram,
    inputs: &[f64],
    state: &[f64],
) -> Result<Value, EvalFault> {
    let inputs = inputs.iter().copied().map(Value::F64).collect::<Vec<_>>();
    let state = state.iter().copied().map(Value::F64).collect::<Vec<_>>();
    evaluate(program, &inputs, &state)
}


thread_local! {
    /// Depth of ongoing reference-cell evaluations. Bounds recursive
    /// capability bodies by the same budget that bounds flat applications:
    /// each nested reference dispatch counts one level.
    static REFERENCE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

// Split out of the original single file. `prelude` reexports every
// child at module width so the children can share helpers.
use prelude::*;
mod eval_control;
mod eval_const;
mod eval_scalar_arith;
mod eval_structure;
mod eval_math;
mod eval_option_result;
mod eval_aggregate;
mod eval_calls;
mod prelude;
mod eval_op;
mod capability;
mod scalar;
mod fold;
mod coerce;
mod exact;
