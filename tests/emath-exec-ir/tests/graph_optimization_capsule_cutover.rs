use std::path::Path;

use emath_exec_ir::interp::{evaluate_with_budget, EvalFault, Value};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_test_harness::Probe;

const REACHABILITY: &str = "std.capability.graph.reachability";
const SHORTEST: &str = "std.capability.graph.shortest-distances";
const BELLMAN_FORD: &str = "std.capability.graph.bellman-ford";
const LP: &str = "std.capability.optimize.lp-minimize";
const PARETO: &str = "std.capability.optimize.pareto-front";
const PURE_NASH: &str = "std.capability.game.pure-nash-claim";
const BEST_RESPONSES: &str = "std.capability.game.best-response-set";

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn language_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    // Uses the distribution installed by the probe harness; never reloads here
    // so authority-gating cases (rollback) observe the installed image.
    let count = inputs.len();
    let mut ops: Vec<_> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Default::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Default::default(),
    ));
    evaluate_with_budget(
        &EmirProgram {
            ops,
            result: EmirValue(count as u32),
            input_count: count as u16,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        inputs,
        &[],
        EvalBudget::default(),
    )
}

#[test]
fn probe() {
    let mut probe = Probe::new("graph_optimization_capsule_cutover.rs: every check in one probe");
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("bind active capsule reference programs");
    probe.case("graph_optimization_and_game_live_only_through_active_capsules", |probe| {
        let graph = matrix(
            4,
            4,
            &[
                0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
            ],
        );
        probe.eq("seam_eval(REACHABILITY, &[graph.clone(), Value::F64(0.0)])", &(seam_eval(REACHABILITY, &[graph.clone(), Value::F64(0.0)])), &(Ok(Value::Vector(vec![1.0, 1.0, 1.0, 1.0]))));
        probe.eq("seam_eval(SHORTEST, &[graph, Value::F64(0.0)])", &(seam_eval(SHORTEST, &[graph, Value::F64(0.0)])), &(Ok(Value::Vector(vec![0.0, 1.0, 1.0, 2.0]))));

        let negative_cycle = matrix(2, 2, &[0.0, -1.0, -1.0, 0.0]);
        let refusal = seam_eval(BELLMAN_FORD, &[negative_cycle, Value::F64(0.0)]).unwrap_err();
        let EvalFault::CarrierRefused { detail, .. } = refusal else { panic!("expected authored negative-cycle refusal"); };
        probe.eq("detail", &(detail), &("E-GRAPH-005".to_string()));

        let solution = seam_eval(
            LP,
            &[
                matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]),
                Value::Vector(vec![1.0, 1.0]),
                Value::Vector(vec![-1.0, -1.0]),
            ],
        )
        .expect("bounded standard-form LP");
        probe.eq("solution", &(solution), &(Value::Vector(vec![1.0, 1.0])));

        probe.eq("seam_eval(PARETO, &[matrix(3, 2, &[1.0, 1.0, 1.0, 1.0, 2.0, 2.0])])", &(seam_eval(PARETO, &[matrix(3, 2, &[1.0, 1.0, 1.0, 1.0, 2.0, 2.0])])), &(Ok(Value::Vector(vec![1.0, 1.0, 0.0]))));

        let row = matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]);
        let column = matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]);
        probe.eq("seam_eval(PURE_NASH, &[row.clone(), column, Value::I64(0), Value::I64(0)])", &(seam_eval(PURE_NASH, &[row.clone(), column, Value::I64(0), Value::I64(0)])), &(Ok(Value::Bool(true))));
        probe.eq("seam_eval(BEST_RESPONSES, &[matrix(3, 1, &[4.0, 4.0, 2.0]), Value::I64(0)])", &(seam_eval(BEST_RESPONSES, &[matrix(3, 1, &[4.0, 4.0, 2.0]), Value::I64(0)])), &(Ok(Value::Vector(vec![0.0, 1.0]))));

        let id = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == REACHABILITY)
            .unwrap()
            .feature_id
            .clone();
        let rolled_back = distribution
            .rollback_feature(&id)
            .expect("scoped rollback reseals the distribution with the prior image chained");
        install_language_distribution(&rolled_back).expect("inactive feature is omitted, not executed");
        probe.demand("\"non-active authority has no live capability binding\"", seam_eval(REACHABILITY, &[matrix(1, 1, &[0.0]), Value::F64(0.0)]).is_err(), "non-active authority has no live capability binding");
        install_language_distribution(&distribution).expect("restore active distribution");
    });
    probe.finish();
}
