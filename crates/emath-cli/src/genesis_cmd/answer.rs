//! Answer evaluation and portfolio assembly.

use super::*;

/// Evaluate `analysis.term` in `world` with the parametric lane's fixtures.
/// Returns `(answer, valuation_label, vm_steps)`; suspensions/unbound vars
/// yield the structural canonical form — never a fabricated constant.
pub(super) fn evaluated_answer(
    analysis: &Analysis,
    world: &WorldIr,
) -> (String, &'static str, u64) {
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
    match world.name.as_str() {
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
            // The one-point carrier IS the unit type: a zero-sized-value
            // map is the honest environment for a one-point algebra.
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
    }
}

/// Honest portfolio for the built-in seed worlds: every candidate is a
/// real evaluation (or the structural term) with disclosed valuation and
/// `Structural` authority (no checker ran, so never `tested`). Also
/// returns VM step counts per world for the receipt's metered cost.
pub(super) fn portfolio(
    analysis: &Analysis,
    worlds: &[WorldIr],
) -> (InterpretationPortfolio, BTreeMap<String, u64>) {
    let mut vm_steps = BTreeMap::new();
    let candidates = worlds
        .iter()
        .map(|world| {
            let label = world.name.as_str();
            let (answer, valuation, steps) = evaluated_answer(analysis, world);
            vm_steps.insert(label.to_string(), steps);
            let (cost, complexity, utility) = match label {
                "free_symbolic" => (1.0, 2.0, 2.0),
                "Boolean_algebra" => (3.0, 1.0, 4.0),
                // Totality witnesses rank below the interpreting worlds:
                // they answer everything but claim no intended meaning.
                "one_point" => (0.5, 0.5, 1.0),
                "csa_seeded" => (2.0, 3.0, 1.5),
                _ => (4.0, 2.0, 5.0),
            };
            InterpretationCandidate {
                world_id: world.identity(),
                name: label.into(),
                answer,
                authority: Authority::Structural,
                score: ScoreVector {
                    cost,
                    complexity,
                    // No checker ran: evidence stays zero and the
                    // receipt's checker_receipts list stays empty.
                    evidence: 0.0,
                    utility,
                },
                provenance: format!("builtin-seed;valuation={valuation}"),
            }
        })
        .collect();
    (InterpretationPortfolio::new(candidates), vm_steps)
}
