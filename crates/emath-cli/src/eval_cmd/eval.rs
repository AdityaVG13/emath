//! World evaluation and receipt rendering.

use super::*;

pub(super) fn evaluate_named(analysis: &Analysis, name: &str) -> Result<EvalReceipt, String> {
    let label = resolve_world_name(name)?;
    if WORLD_IR_WORLD_NAMES.contains(&label) {
        return evaluate_world_ir_builtin(analysis, label);
    }
    let worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    evaluate_in_world(analysis, &worlds, label)
}

/// Evaluates the term in one World-IR builtin class world via the
/// WorldIr-driven adapter (no per-class evaluator match arm), with the
/// same `{a: 4, b: 7}` integer valuation the modular fixture uses.
pub(super) fn evaluate_world_ir_builtin(
    analysis: &Analysis,
    name: &str,
) -> Result<EvalReceipt, String> {
    let mut builtins = emath_world_ir::builtin::builtin_worlds();
    let Some(index) = builtins
        .iter()
        .position(|builtin| builtin.world.name == name)
    else {
        return Err(unknown_world_error(name));
    };
    let world_ir = builtins.swap_remove(index).world;
    let world = crate::world_ir_eval::WorldIrWorld::new(&world_ir);
    let canonical = analysis.term.canonical();
    let environment: Environment<WorldIrValue> = [
        (VariableId("a".to_string()), WorldIrValue::Int(4)),
        (VariableId("b".to_string()), WorldIrValue::Int(7)),
    ]
    .into();
    let budget = VmBudget::seed_default();
    let (answer, valuation, vm_steps) = match vm_run(&analysis.term, &world, &environment, &budget)
    {
        Ok(VmOutcome::Complete { value, steps, .. }) => {
            (value.canonical(), "world-ir-builtin", steps)
        }
        Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
    };
    let world_id = world_ir.identity().0;
    Ok(EvalReceipt {
        answer,
        world_name: world_ir.name,
        world_id,
        vm_steps,
        term_id: analysis.term_id,
        source_hash: analysis.source_hash,
        valuation,
        lock_id: None,
    })
}

pub(super) fn evaluate_in_world(
    analysis: &Analysis,
    worlds: &[WorldIr],
    name: &str,
) -> Result<EvalReceipt, String> {
    let world = worlds
        .iter()
        .find(|world| world.name == name)
        .ok_or_else(|| unknown_world_error(name))?;
    Ok(evaluate_world(analysis, world))
}

/// Same per-world environment construction as genesis `evaluated_answer`.
pub(super) fn evaluate_world(analysis: &Analysis, world: &WorldIr) -> EvalReceipt {
    let canonical = analysis.term.canonical();
    let free_env: Environment<Term> = [
        (
            VariableId("a".to_string()),
            Term::Variable(VariableId("a".to_string())),
        ),
        (
            VariableId("b".to_string()),
            Term::Variable(VariableId("b".to_string())),
        ),
    ]
    .into();
    let boolean_env: Environment<bool> = [
        (VariableId("a".to_string()), true),
        (VariableId("b".to_string()), false),
    ]
    .into();
    let modular_env: Environment<i64> = [
        (VariableId("a".to_string()), 4),
        (VariableId("b".to_string()), 7),
    ]
    .into();
    let budget = VmBudget::seed_default();
    let (answer, valuation, vm_steps) = match world.name.as_str() {
        "free_symbolic" => match vm_run(&analysis.term, &FreeTermWorld, &free_env, &budget) {
            Ok(VmOutcome::Complete { value, steps, .. }) => {
                (value.canonical(), "fixture_free", steps)
            }
            Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
        },
        "Boolean_algebra" => {
            match vm_run(&analysis.term, &BooleanAlienWorld, &boolean_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_boolean", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "modular_numeric" => {
            match vm_run(&analysis.term, &ModularAlienWorld, &modular_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_modular", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "one_point" => {
            #[allow(clippy::zero_sized_map_values)]
            let env: Environment<()> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), ()))
                .collect();
            match vm_run(&analysis.term, &OnePointWorld, &env, &budget) {
                Ok(VmOutcome::Complete { steps, .. }) => ("•".to_string(), "one_point", steps),
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "csa_seeded" => {
            let csa = SeededCsaWorld::baseline();
            let env: Environment<u64> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), csa.variable_value(&variable.0)))
                .collect();
            match vm_run(&analysis.term, &csa, &env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (format!("{value:016x}"), "csa_baseline_seed", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        _ => (canonical, "structural", 0),
    };
    EvalReceipt {
        answer,
        world_name: world.name.clone(),
        world_id: world.identity().0,
        vm_steps,
        term_id: analysis.term_id,
        source_hash: analysis.source_hash,
        valuation,
        lock_id: None,
    }
}

pub(super) fn emit_receipt(receipt: &EvalReceipt, json: bool) {
    if json {
        print!("{}", render_json(receipt));
    } else {
        print!("{}", render_text(receipt));
    }
}

pub(super) fn render_text(receipt: &EvalReceipt) -> String {
    match receipt.lock_id {
        Some(lock_id) => format!(
            "value {}\nworld {} {:016x}\nvm_steps {}\nprovenance user-locked\nlock_id {:016x}\n",
            receipt.answer, receipt.world_name, receipt.world_id, receipt.vm_steps, lock_id
        ),
        None => format!(
            "value {}\nworld {} {:016x}\nvm_steps {}\n",
            receipt.answer, receipt.world_name, receipt.world_id, receipt.vm_steps
        ),
    }
}

pub(super) fn render_json(receipt: &EvalReceipt) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.eval-answer");
    object.int("schema_version", 1);
    object.string("answer", &receipt.answer);
    object.string("world_name", &receipt.world_name);
    object.string("world_id", &format!("{:016x}", receipt.world_id));
    object.int("vm_steps", receipt.vm_steps);
    object.int("term_id", receipt.term_id);
    object.int("source_hash", receipt.source_hash);
    if let Some(lock_id) = receipt.lock_id {
        object.string("meaning_provenance", "user-locked");
        object.string("lock_id", &format!("{lock_id:016x}"));
    }
    object.finish()
}

pub(super) fn print_portfolio(analysis: &Analysis, worlds: &[WorldIr]) {
    for world in worlds {
        let receipt = evaluate_world(analysis, world);
        println!(
            "world {} {:016x} answer {}",
            receipt.world_name, receipt.world_id, receipt.answer
        );
    }
}

pub(super) fn print_explain(receipt: &EvalReceipt) {
    println!(
        "world {} {:016x}\nvm_steps {}\nvaluation {}",
        receipt.world_name, receipt.world_id, receipt.vm_steps, receipt.valuation
    );
}
