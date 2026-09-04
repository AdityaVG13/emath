use std::path::Path;

use emath_exec_ir::interp::Value;
use emath_exec_ir::language_image::compile_language_directory;
use emath_exec_ir::native_kernel::{install_language_distribution, native_kernel};

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

#[test]
fn graph_optimization_and_game_live_only_through_active_capsules() {
    let distribution =
        compile_language_directory(&language_root()).expect("compile authored language");
    for feature in [
        REACHABILITY,
        SHORTEST,
        BELLMAN_FORD,
        LP,
        PARETO,
        PURE_NASH,
        BEST_RESPONSES,
    ] {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature)
            .unwrap_or_else(|| panic!("missing capsule {feature}"));
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id]
                .state
                .as_str(),
            "capsule-active"
        );
    }
    install_language_distribution(&distribution).expect("bind active capsule kernels");

    let graph = matrix(
        4,
        4,
        &[
            0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        ],
    );
    let reachability = native_kernel(REACHABILITY).expect("active graph kernel");
    assert_eq!(
        (reachability.handler)(&[graph.clone(), Value::F64(0.0)]),
        Ok(Value::Vector(vec![1.0, 1.0, 1.0, 1.0]))
    );
    let shortest = native_kernel(SHORTEST).expect("active shortest-path kernel");
    assert_eq!(
        (shortest.handler)(&[graph, Value::F64(0.0)]),
        Ok(Value::Vector(vec![0.0, 1.0, 1.0, 2.0]))
    );

    let negative_cycle = matrix(2, 2, &[0.0, -1.0, -1.0, 0.0]);
    let refusal =
        (native_kernel(BELLMAN_FORD).unwrap().handler)(&[negative_cycle, Value::F64(0.0)])
            .unwrap_err();
    assert_eq!(
        refusal, "E-GRAPH-005",
        "no distance certificate exists for a reachable negative cycle"
    );

    let lp = native_kernel(LP).expect("active Bland-simplex kernel");
    let solution = (lp.handler)(&[
        matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]),
        Value::Vector(vec![1.0, 1.0]),
        Value::Vector(vec![-1.0, -1.0]),
    ])
    .expect("bounded standard-form LP");
    assert_eq!(solution, Value::Vector(vec![1.0, 1.0]));

    let pareto = native_kernel(PARETO).expect("active Pareto kernel");
    assert_eq!(
        (pareto.handler)(&[matrix(3, 2, &[1.0, 1.0, 1.0, 1.0, 2.0, 2.0])]),
        Ok(Value::Vector(vec![1.0, 1.0, 0.0])),
        "identical points do not dominate each other"
    );

    let row = matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]);
    let column = matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]);
    let nash = native_kernel(PURE_NASH).expect("active finite-claim kernel");
    assert_eq!(
        (nash.handler)(&[row.clone(), column, Value::I64(0), Value::I64(0)]),
        Ok(Value::Bool(true))
    );
    let ties = native_kernel(BEST_RESPONSES).expect("active complete tie-set kernel");
    assert_eq!(
        (ties.handler)(&[matrix(3, 1, &[4.0, 4.0, 2.0]), Value::I64(0)]),
        Ok(Value::Vector(vec![0.0, 1.0])),
        "ties are a complete ascending certificate, never one hidden argmax"
    );

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
    assert!(
        native_kernel(REACHABILITY).is_none(),
        "non-active authority has no live kernel binding"
    );
    install_language_distribution(&distribution).expect("restore active distribution");
}

#[test]
fn migrated_sema_file_contains_no_graph_optimization_name_authority() {
    let source = include_str!("../../../crates/emath-sema/src/admit/lowering/call/linear.rs");
    for legacy in [
        "bellman_ford",
        "sparse_triplets",
        "sparse_from_triplets",
        "reachability",
        "bfs_order",
        "shortest_distances",
        "out_degrees",
        "graph_laplacian",
        "graph_symmetrize",
        "lp_minimize",
        "pareto_front",
    ] {
        assert!(
            !source.contains(legacy),
            "migrated semantic branch remains for {legacy}"
        );
    }
}
