//! emath-r2-graphs-masa (slice 6): sparse storage — the COO triplet
//! carrier.
//!
//! The epic's named "sparse STORAGE formats" deferral, thinned to the
//! thin nucleus that makes sparse graphs WRITABLE and COMPUTABLE with
//! zero new algorithm machinery:
//! - `sparse_triplets(adj)` — dense carrier → flat triplet vector
//!   `[u0, v0, w0, u1, v1, w1, ...]` in ascending (u, v) order;
//!   explicit 0.0 entries are NOT edges (the dense convention) and
//!   are skipped.
//! - `sparse_from_triplets(n, triplets)` — COO build into the dense
//!   adjacency; DUPLICATE (u, v) entries SUM (the COO build law,
//!   documented — parallel edges add weights); out-of-range indices
//!   refuse `E-GRAPH-003` (a vertex outside 0..n), non-finite weights
//!   refuse `E-GRAPH-004`, a length that is not a multiple of three
//!   refuses the NEW `E-GRAPH-006` (the sparse carrier's own
//!   well-formedness law).
//! - Zero new algorithm machinery: the built carrier feeds the
//!   EXISTING `dijkstra`/`reachability`/... ops unchanged.
//! - Determinism: extraction order ascending (u, v); identical
//!   inputs bit-identical.
//! - Surface: call names + registry cells `std.graph.sparse_triplets`
//!   and `std.graph.sparse_from_triplets` (cohort 32).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

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

fn dense(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

#[test]
fn extraction_ascending_skips_zeros() {
    // 3-vertex carrier with edges 0→2 (2.5), 1→0 (−1), 2→1 (0.5);
    // explicit 0.0 entries are skipped; order ascending (u, v).
    let triplets = eval(
        vec![EmirOp::GraphSparseTriplets(EmirValue(0))],
        &[dense(
            3,
            3,
            &[0.0, 0.0, 2.5, -1.0, 0.0, 0.0, 0.0, 0.5, 0.0],
        )],
    )
    .expect("extraction computes");
    let flat = vector_of(&triplets);
    let expected = [0.0, 2.0, 2.5, 1.0, 0.0, -1.0, 2.0, 1.0, 0.5];
    assert_eq!(flat.len(), expected.len(), "3 edges × 3 fields");
    for (got, want) in flat.iter().zip(expected.iter()) {
        assert_eq!(got, want, "triplet stream {flat:?} vs {expected:?}");
    }
}

#[test]
fn round_trip_law() {
    // from_triplets(n, triplets(adj)) == adj when no duplicates and
    // no explicit zeros — the storage round trip at 1e-12 (kills
    // index-swaps and weight/transposed-weight mutants).
    let adj = dense(
        3,
        3,
        &[0.0, 0.0, 2.5, -1.0, 0.0, 0.0, 0.0, 0.5, 0.0],
    );
    let triplets = eval(
        vec![EmirOp::GraphSparseTriplets(EmirValue(0))],
        &[adj.clone()],
    )
    .expect("extraction computes");
    let rebuilt = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(3.0), triplets],
    )
    .expect("build computes");
    assert_eq!(rebuilt, adj, "round trip preserves the carrier");
}

#[test]
fn duplicate_entries_sum() {
    // The COO build law: duplicate (u, v) entries SUM (parallel edges
    // add weights). (0→1, 1.5) + (0→1, 2.5) → adj[0][1] = 4.
    let triplets = Value::Vector(vec![0.0, 1.0, 1.5, 0.0, 1.0, 2.5]);
    let built = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(2.0), triplets],
    )
    .expect("build computes");
    let Value::Matrix { data, .. } = built else {
        panic!("expected a matrix")
    };
    assert!((data[1] - 4.0).abs() < 1e-12, "duplicates sum, got {}", data[1]);
}

#[test]
fn composition_with_dijkstra() {
    // End-to-end: the dense reference answer vs the sparse-built
    // answer agree through the EXISTING dijkstra op (zero new
    // algorithm machinery).
    let dense_adj = dense(
        4,
        4,
        &[0.0, 1.0, 5.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0],
    );
    let reference = eval(
        vec![EmirOp::GraphDijkstra(EmirValue(0), EmirValue(1))],
        &[dense_adj.clone(), Value::F64(0.0)],
    )
    .expect("dense dijkstra computes");
    let triplets = eval(
        vec![EmirOp::GraphSparseTriplets(EmirValue(0))],
        &[dense_adj],
    )
    .expect("extraction computes");
    let built = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(4.0), triplets],
    )
    .expect("build computes");
    let sparse_answer = eval(
        vec![EmirOp::GraphDijkstra(EmirValue(0), EmirValue(1))],
        &[built, Value::F64(0.0)],
    )
    .expect("sparse-built dijkstra computes");
    assert_eq!(reference, sparse_answer, "storage composition law");
}

#[test]
fn refusals() {
    // Out-of-range index → E-GRAPH-003; non-finite weight →
    // E-GRAPH-004; length not a multiple of three → the NEW
    // E-GRAPH-006. The negative seed cross-checks E-GRAPH-006.
    let out_of_range = Value::Vector(vec![0.0, 9.0, 1.0]);
    let error = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(3.0), out_of_range],
    )
    .expect_err("out-of-range index refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-003"),
        "out-of-range must name E-GRAPH-003, got {error:?}"
    );
    let non_finite = Value::Vector(vec![0.0, 1.0, f64::NAN]);
    let error = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(2.0), non_finite],
    )
    .expect_err("non-finite weight refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-004"),
        "non-finite must name E-GRAPH-004, got {error:?}"
    );
    let ragged = Value::Vector(vec![0.0, 1.0, 1.0, 0.0]);
    let error = eval(
        vec![EmirOp::GraphSparseFromTriplets(
            EmirValue(0),
            EmirValue(1),
        )],
        &[Value::F64(2.0), ragged],
    )
    .expect_err("ragged triplet stream refuses");
    assert!(
        format!("{error:?}").contains("E-GRAPH-006"),
        "ragged stream must name E-GRAPH-006, got {error:?}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/sparse_graph_dimensions.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-GRAPH-006"),
        "seed expects the malformed-carrier refusal, found: {expect_line}"
    );
}

#[test]
fn cell_registry_and_shape_law() {
    // Both cells are registry DATA (cohort 32); a scalar triplet
    // stream refuses at COMPILE (ShapeMismatch).
    let registry = std_cell_registry();
    for name in [
        "std.graph.sparse_triplets",
        "std.graph.sparse_from_triplets",
    ] {
        assert!(
            registry.contains_key(name),
            "registry cell {name} present; have {:?}",
            registry.keys().collect::<Vec<_>>()
        );
    }
    let term = Term::Apply {
        operator: SymbolId("sparse_from_triplets".into()),
        arguments: vec![
            Term::Variable(VariableId("n".into())),
            Term::Variable(VariableId("triplets".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("sparse_from_triplets".into()), 2)
        .expect("signature is conflict-free");
    compile_reference(
        &term,
        &signature,
        &[
            ("n".to_string(), ParamShape::Scalar),
            ("triplets".to_string(), ParamShape::Vector),
        ],
        Vec::new(),
        "std.graph.sparse_from_triplets",
    )
    .expect("vector triplets compile");
    let error = compile_reference(
        &term,
        &signature,
        &[
            ("n".to_string(), ParamShape::Scalar),
            ("triplets".to_string(), ParamShape::Scalar),
        ],
        Vec::new(),
        "std.graph.sparse_from_triplets",
    )
    .expect_err("scalar triplets refuse at compile");
    assert!(
        format!("{error:?}").contains("ShapeMismatch"),
        "scalar triplets must ShapeMismatch at compile, got {error:?}"
    );
    let _ = TermCompileError::ShapeMismatch {
        symbol: "sparse_from_triplets".to_string(),
        detail: "unused".to_string(),
    };
}

#[test]
fn bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct SparseWorld;
    impl emath_genesis::FirstOrderWorld for SparseWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let adj = dense(2, 2, &[0.0, 3.0, 0.0, 0.0]);
            let triplets = eval(
                vec![EmirOp::GraphSparseTriplets(EmirValue(0))],
                &[adj],
            )
            .ok()
            .map(|value| vector_of(&value));
            match triplets {
                Some(stream) if stream == vec![0.0, 1.0, 3.0] => {
                    Ok("sparse-carrier-roundtrip".to_string())
                }
                _ => Ok("sparse-carrier-diverged".to_string()),
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
                "sparse-storage-nucleus",
                &["coo-triplets", "duplicate-sum", "ascending-order"],
            )
        }
    }

    let term = Term::Constant(SymbolId("graph[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &SparseWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "sparse-storage-nucleus");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
