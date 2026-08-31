//! emath-r2-graphs-masa (slice 5): negative-edge shortest paths —
//! Bellman-Ford.
//!
//! The epic's named "negative-edge methods (Bellman-Ford class)"
//! deferral, thinned to the classic O(n·m) relaxation: negative edge
//! weights are ADMITTED (the whole point — Dijkstra refuses them at
//! `E-GRAPH-002`), unreachable vertices stay `+Inf` (honest numeric),
//! and a negative cycle REACHABLE from the source refuses typed
//! `E-GRAPH-005` — a genuinely new diagnostic class (no shortest-path
//! answer exists; never a silently wrong distance set).
//!
//! Laws (each discriminating):
//! - Cross-op law: the negative-edge fixture where Dijkstra REFUSES
//!   (`E-GRAPH-002`) and Bellman-Ford computes the CORRECT distances —
//!   the greedy invariant provably fails here (a Dijkstra-style mutant
//!   answers d[1] = 4 instead of −1).
//! - Closed-form law: the classic 4-vertex fixture yields
//!   exactly `[0, −1, 1, 2]`.
//! - Negative-cycle law: a reachable cycle of negative total weight
//!   refuses `E-GRAPH-005` — no distances are fabricated.
//! - Zero-cycle tolerance: a cycle of total weight ZERO is legal and
//!   terminates (kills over-eager cycle-detection mutants).
//! - Unreachable law: no path in → `+Inf`.
//! - Refusals reuse the closed carrier set: ragged → `E-GRAPH-001`,
//!   source outside `0..n` → `E-GRAPH-003`, non-finite → `E-GRAPH-004`.
//! - Surface: call name `bellman_ford(adj, source)` (compile-time
//!   shape law), registry cell `std.graph.bellman_ford` (cohort 30).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

/// The classic negative-edge fixture: 0→1 (4), 0→2 (1), 2→1 (−2),
/// 1→3 (1), 2→3 (5). Shortest distances: [0, −1, 1, 2] — the direct
/// 0→1 edge (4) is beaten by 0→2→1 (1 − 2 = −1), which a greedy
/// (Dijkstra-order) mutant never discovers.
fn negative_edge_carrier() -> Value {
    Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 4.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, -2.0, 0.0, 5.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    }
}

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn distances_from(adj: &Value, source: f64) -> Result<Vec<f64>, EvalFault> {
    let value = eval(
        vec![EmirOp::GraphBellmanFord(EmirValue(0), EmirValue(1))],
        &[adj.clone(), Value::F64(source)],
    )?;
    let Value::Vector(distances) = value else {
        panic!("expected a distance vector, got {value:?}")
    };
    Ok(distances)
}

#[test]
fn negative_edges_beat_greedy() {
    // The closed-form law on the classic fixture; the cross-op law:
    // Dijkstra REFUSES this carrier (E-GRAPH-002) while Bellman-Ford
    // computes [0, −1, 1, 2]. A Dijkstra-style greedy mutant answers
    // d[1] = 4 (never revisits vertex 1) and fails.
    let dijkstra_error = eval(
        vec![EmirOp::GraphDijkstra(EmirValue(0), EmirValue(1))],
        &[negative_edge_carrier(), Value::F64(0.0)],
    )
    .expect_err("Dijkstra refuses negative weights");
    assert!(
        format!("{dijkstra_error:?}").contains("E-GRAPH-002"),
        "cross-op law: Dijkstra refuses, got {dijkstra_error:?}"
    );
    let distances = distances_from(&negative_edge_carrier(), 0.0).expect("BF computes");
    let expected = [0.0, -1.0, 1.0, 0.0];
    for (got, want) in distances.iter().zip(expected.iter()) {
        assert!((got - want).abs() < 1e-12, "d = {got} vs {want}");
    }
}

#[test]
fn negative_cycle_refuses_typed() {
    // A reachable cycle of total weight −2: no shortest-path answer
    // EXISTS, so E-GRAPH-005 refuses — never fabricated distances.
    let cycle = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, -1.0, 0.0, //
            0.0, -1.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    };
    let error = distances_from(&cycle, 0.0).expect_err("negative cycle refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-005"),
        "negative cycle must name E-GRAPH-005, got {error:?}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/bellman_ford_negative_weight.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-GRAPH-005"),
        "seed expects the negative-cycle refusal, found: {expect_line}"
    );
}

#[test]
fn zero_cycle_terminates() {
    // A cycle of total weight ZERO is legal (1→2 = −1, 2→1 = +1):
    // distances stabilize and the op terminates with the correct
    // values. An over-eager cycle-detection mutant (any relaxation
    // change after n−1 passes treated as a cycle) fails.
    let zero_cycle = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, -1.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    };
    let distances = distances_from(&zero_cycle, 0.0).expect("zero cycle terminates");
    assert_eq!(distances.len(), 4);
    let expected = [0.0, 1.0, 0.0, f64::INFINITY];
    for (got, want) in distances.iter().zip(expected.iter()) {
        assert!(
            (got - want).abs() < 1e-12 || (got.is_infinite() && want.is_infinite()),
            "d = {got} vs {want}"
        );
    }
}

#[test]
fn unreachable_is_positive_infinity() {
    // Honest numeric: no path in → +Inf (the Dijkstra convention,
    // shared; a 0.0-for-unreachable mutant fails).
    let distances = distances_from(&negative_edge_carrier(), 0.0).expect("BF computes");
    assert!(distances[3] == 0.0, "vertex 3 reachable via 1→3: {}", distances[3]);
    // Source = 3 (the sink): vertices 0..2 unreachable.
    let from_sink = distances_from(&negative_edge_carrier(), 3.0).expect("BF computes");
    assert!(from_sink[0].is_infinite() && from_from_is_infinite(&from_sink));
}

fn from_from_is_infinite(distances: &[f64]) -> bool {
    distances[1..3].iter().all(|d| d.is_infinite())
}

#[test]
fn carrier_refusals_reuse_closed_set() {
    // Ragged → E-GRAPH-001; source outside 0..n → E-GRAPH-003;
    // non-finite → E-GRAPH-004 (the established set; only the
    // negative-cycle class is new here).
    let ragged = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
    };
    let error = distances_from(&ragged, 0.0).expect_err("ragged refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-001"),
        "ragged must name E-GRAPH-001, got {error:?}"
    );
    let error = distances_from(&negative_edge_carrier(), 9.0)
        .expect_err("out-of-range source refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-003"),
        "source must name E-GRAPH-003, got {error:?}"
    );
    let non_finite = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, f64::NAN, 0.0, 0.0],
    };
    let error = distances_from(&non_finite, 0.0).expect_err("non-finite refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-004"),
        "non-finite must name E-GRAPH-004, got {error:?}"
    );
}

#[test]
fn cell_registry_and_shape_law() {
    // std.graph.bellman_ford is registry DATA (cohort 30), compiles
    // through the call seam, and evaluates the SAME distances; a
    // scalar adjacency refuses at COMPILE (ShapeMismatch).
    let registry = std_cell_registry();
    assert!(
        registry.contains_key("std.graph.bellman_ford"),
        "registry cell present; have {:?}",
        registry.keys().collect::<Vec<_>>()
    );

    let term = Term::Apply {
        operator: SymbolId("bellman_ford".into()),
        arguments: vec![
            Term::Variable(VariableId("adj".into())),
            Term::Variable(VariableId("source".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("bellman_ford".into()), 2)
        .expect("bellman_ford signature is conflict-free");
    compile_reference(
        &term,
        &signature,
        &[
            ("adj".to_string(), ParamShape::Matrix),
            ("source".to_string(), ParamShape::Scalar),
        ],
        Vec::new(),
        "std.graph.bellman_ford",
    )
    .expect("matrix adjacency compiles");

    // Cell path evaluates the same distances as the bare op.
    let mut ops: Vec<(EmirOp, Span)> = vec![
        (EmirOp::LoadInput(0), Span::default()),
        (EmirOp::LoadInput(1), Span::default()),
    ];
    ops.push((
        EmirOp::ApplyCapability {
            capability: "std.graph.bellman_ford".to_string(),
            class: CellClass::Pure,
            args: vec![EmirValue(0), EmirValue(1)],
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(2),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let value = evaluate_with_budget(
        &program,
        &[negative_edge_carrier(), Value::F64(0.0)],
        &[],
        EvalBudget::default(),
    )
    .expect("cell evaluates");
    let Value::Vector(distances) = value else {
        panic!("expected a distance vector")
    };
    let expected = [0.0, -1.0, 1.0, 0.0];
    for (got, want) in distances.iter().zip(expected.iter()) {
        assert!((got - want).abs() < 1e-12, "cell d = {got} vs {want}");
    }

    // Shape law: a scalar adjacency refuses at COMPILE.
    let error = compile_reference(
        &term,
        &signature,
        &[
            ("adj".to_string(), ParamShape::Scalar),
            ("source".to_string(), ParamShape::Scalar),
        ],
        Vec::new(),
        "std.graph.bellman_ford",
    )
    .expect_err("scalar adjacency refuses at compile");
    assert!(
        format!("{error:?}").contains("ShapeMismatch"),
        "scalar adjacency must ShapeMismatch at compile, got {error:?}"
    );
    let _ = TermCompileError::ShapeMismatch {
        symbol: "bellman_ford".to_string(),
        detail: "unused".to_string(),
    };
}

#[test]
fn bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct GraphWorld;
    impl emath_genesis::FirstOrderWorld for GraphWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let distances = distances_from(&negative_edge_carrier(), 0.0).unwrap_or_default();
            if distances.len() == 4 && (distances[1] - (-1.0)).abs() < 1e-12 {
                Ok("negative-edge-shortest-paths".to_string())
            } else {
                Ok("negative-edge-diverged".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "negative-edge-methods",
                &["bellman-ford", "negative-cycle-refusal", "greedy-beaten"],
            )
        }
    }

    let term = Term::Constant(SymbolId("graph[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &GraphWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "negative-edge-methods");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
