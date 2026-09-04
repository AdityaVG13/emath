//! Graph data structure + graph EMIR ops.
//!
//! The law, scoped to the numeric-kernel + EMIR seam (disjoint
//! from the parser and the term-compiler Matrix-shape seam):
//! - **Carrier**: a directed graph enters as a DENSE adjacency-matrix
//!   operand — flat row-major `n×n`, entry `(i, j)` = edge `i → j`
//!   (0.0 = no edge; nonzero = edge, its value is the weight). No
//!   hash-order nondeterminism anywhere: vertices are indices, neighbor
//!   scans are ascending-index.
//! - **Reachability / BFS** (`GraphReachable`, `GraphBfsOrder`):
//!   deterministic traversal — BFS visit order with ascending-index
//!   neighbor discovery (never DFS order, never insertion order).
//! - **Shortest path** (`GraphDijkstra`): O(n²) selection Dijkstra over
//!   the dense carrier, nonnegative weights; an unreachable vertex is
//!   +Inf (honest numeric), and a NEGATIVE weight refuses typed
//!   `E-GRAPH-002` — Dijkstra's precondition, never a silently wrong
//!   distance. Non-square adjacency refuses `E-GRAPH-001` (the
//!   negative seed's silent-success shape); a source vertex outside
//!   `0..n` refuses `E-GRAPH-003`.
//! - **Degrees** (`GraphDegreeOut`): out-degree = count of nonzero
//!   entries per row (a 0.0 entry is no edge, even in a weighted
//!   carrier; in-degree = the same op on the transposed carrier).
//! - The `.emath` call surface (`shortest_distances(A, s)` and friends)
//!   is deferred: it needs the term compiler's
//!   Matrix-shape plumbing (ParamShape/Shape are scalar/vector today).

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{SymbolId, Term};

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("graph kernels install");
}

/// The active universal seam for domain math: an `ApplyCapability`
/// over a capsule-active FeatureID (no domain-named `EmirOp`).
fn cell(capability: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: capability.to_string(),
        class: CellClass::Pure,
        args,
    }
}

const REACHABILITY: &str = "std.capability.graph.reachability";
const BFS_ORDER: &str = "std.capability.graph.bfs-order";
const SHORTEST_DISTANCES: &str = "std.capability.graph.shortest-distances";
const OUT_DEGREES: &str = "std.capability.graph.out-degrees";

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    // The seam law: inputs enter registers through LoadInput ops,
    // then the kernel ops consume them by register index; every op
    // appends one register, so the program result is the LAST op's
    // output register.
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

fn f64_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
}

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

/// The reference carrier: edges 0→1, 0→2, 1→3, 2→3 (unweighted).
/// BFS from 0 discovers 1 and 2 at depth 1 (ascending index), 3 at
/// depth 2 — a DFS would visit 3 before 2, so this graph discriminates.
fn reference_adjacency() -> Value {
    matrix(
        4,
        4,
        &[
            0.0, 1.0, 1.0, 0.0, // 0 → 1, 0 → 2
            0.0, 0.0, 0.0, 1.0, // 1 → 3
            0.0, 0.0, 0.0, 1.0, // 2 → 3
            0.0, 0.0, 0.0, 0.0,
        ],
    )
}

#[test]
fn graph_reachability_computes() {
    // From 0: {0, 1, 2, 3}. From 3: {3} (the sink reaches only itself).
    let reachable = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[reference_adjacency(), Value::F64(0.0)],
    )
    .expect("reachability computes");
    assert_eq!(vector_of(&reachable), vec![1.0, 1.0, 1.0, 1.0]);
    let reachable = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[reference_adjacency(), Value::F64(3.0)],
    )
    .expect("sink reachability computes");
    assert_eq!(vector_of(&reachable), vec![0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn bfs_order_is_breadth_first_not_depth_first() {
    // The order law: [0, 1, 2, 3] — breadth-first with ascending-index
    // discovery. A depth-first traversal on this carrier yields
    // [0, 1, 3, 2]; an insertion-order traversal yields [0, 2, 1, 3].
    // The test pins the law, so either mutant fails.
    let order = eval(
        vec![cell(BFS_ORDER, vec![EmirValue(0), EmirValue(1)])],
        &[reference_adjacency(), Value::F64(0.0)],
    )
    .expect("bfs order computes");
    assert_eq!(vector_of(&order), vec![0.0, 1.0, 2.0, 3.0]);
}

#[test]
fn dijkstra_computes_and_marks_unreachable_vertices() {
    // Weighted carrier: 0→1 (1), 1→2 (1), 0→2 (3), and 3 unreachable.
    // Distances [0, 1, 2, +Inf]: the direct 0→2 edge of weight 3 must
    // LOSE to the path 0→1→2 of total 2 (a greedy-first-edge mutant
    // fails), and unreachable vertex 3 is +Inf, never a wrong finite
    // distance.
    let weighted = matrix(
        4,
        4,
        &[
            0.0, 1.0, 3.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    );
    let distances = eval(
        vec![cell(SHORTEST_DISTANCES, vec![EmirValue(0), EmirValue(1)])],
        &[weighted, Value::F64(0.0)],
    )
    .expect("dijkstra computes");
    let d = vector_of(&distances);
    assert_eq!(d.len(), 4);
    assert_eq!(f64_of(&Value::F64(d[0])), 0.0);
    assert!((d[1] - 1.0).abs() < 1e-12, "d[1] = 1, got {d:?}");
    assert!((d[2] - 2.0).abs() < 1e-12, "d[2] = 2 via 0→1→2, got {d:?}");
    assert!(
        d[3].is_infinite() && d[3] > 0.0,
        "unreachable is +Inf, got {d:?}"
    );
}

#[test]
fn dijkstra_refuses_negative_weight() {
    // Dijkstra's precondition is nonnegative weights; a negative entry
    // refuses typed E-GRAPH-002 — never a silently wrong distance set
    // (negative edges need Bellman-Ford-class methods, a named deferral).
    let negative = matrix(
        2,
        2,
        &[
            0.0, 1.0, //
            -1.0, 0.0,
        ],
    );
    let error = eval(
        vec![cell(SHORTEST_DISTANCES, vec![EmirValue(0), EmirValue(1)])],
        &[negative, Value::F64(0.0)],
    )
    .expect_err("negative weight refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-002"),
        "negative-weight dijkstra must name E-GRAPH-002, got {fault}"
    );
}

#[test]
fn non_square_graph_matrix_refuses_typed() {
    // NEGATIVE (the seed's silent-success): a non-square adjacency
    // carrier refuses typed E-GRAPH-001 — never a silently truncated
    // traversal over garbage shape.
    let rectangular = matrix(2, 3, &[0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let error = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[rectangular, Value::F64(0.0)],
    )
    .expect_err("non-square adjacency refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-001"),
        "non-square adjacency must name E-GRAPH-001, got {fault}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/graph_weights.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-GRAPH-001"),
        "seed expects the non-square refusal, found: {expect_line}"
    );
}

#[test]
fn graph_source_out_of_range_refuses_typed() {
    // A source vertex outside 0..n refuses typed E-GRAPH-003 — never a
    // silently empty traversal.
    let error = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[reference_adjacency(), Value::F64(7.0)],
    )
    .expect_err("out-of-range source refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-003"),
        "out-of-range source must name E-GRAPH-003, got {fault}"
    );
}

#[test]
fn graph_out_degree_computes() {
    // Out-degree = count of NONZERO entries per row (0.0 is no edge even
    // in a weighted carrier; a self-loop counts). Reference carrier:
    // [2, 1, 1, 0]. In-degree is the same op on the transposed carrier
    // (documented; the transpose op is the existing generic vocabulary).
    let degrees = eval(
        vec![cell(OUT_DEGREES, vec![EmirValue(0)])],
        &[reference_adjacency()],
    )
    .expect("degrees compute");
    assert_eq!(vector_of(&degrees), vec![2.0, 1.0, 1.0, 0.0]);
    // Weighted carrier: weights do not multiply the degree.
    let weighted = matrix(
        2,
        2,
        &[
            0.0, 5.0, //
            0.0, 0.0,
        ],
    );
    let degrees = eval(vec![cell(OUT_DEGREES, vec![EmirValue(0)])], &[weighted])
        .expect("weighted degrees compute");
    assert_eq!(vector_of(&degrees), vec![1.0, 0.0]);
}

#[test]
fn graph_algorithm_result_bundle_is_complete() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct GraphWorld;
    impl emath_genesis::FirstOrderWorld for GraphWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let order = eval(
                vec![cell(BFS_ORDER, vec![EmirValue(0), EmirValue(1)])],
                &[reference_adjacency(), Value::F64(0.0)],
            )
            .map(|v| vector_of(&v))
            .unwrap_or_default();
            if order == vec![0.0, 1.0, 2.0, 3.0] {
                Ok("bfs-deterministic".to_string())
            } else {
                Ok("bfs-diverged".to_string())
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
                "graph-traversal",
                &["deterministic-bfs", "typed-graph-refusals"],
            )
        }
    }

    let term = Term::Constant(SymbolId("graphs[fixture]".into()));
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
    assert_eq!(result.world, "graph-traversal");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}

/// Vertex-relabel metamorphic laws at
/// the kernel wrapper seam (crates/emath-rt::graph). Relabeling
/// vertices by a permutation p preserves the graph's meaning:
/// reachability masks, shortest distances, and out-degrees permute
/// with the relabel; Laplacians are permutation-similar (diagonal
/// permutes, trace invariant); sparse triplet streams are
/// permutation-equivariant. Twins at the .emath surface live in
/// `tests/emath-sema/tests/graph_emath_surface.rs`.
mod vertex_relabel_laws {
    // The kernel wrappers moved behind the private `emath_rt::graph`
    // module; the crate root re-exports them under their kernel ABI
    // names (same functions, same signatures, same error type).
    use emath_rt::{
        breadth_order, degree_minus_carrier, dense_to_coordinate_stream, nonnegative_shortest_path,
        reachable_mask, row_nonzero_counts,
    };

    /// Base carrier: 0→1 (1.0), 0→2 (2.0), 1→3 (3.0), 2→3 (0.5).
    fn base_adjacency() -> Vec<f64> {
        vec![
            0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0,
        ]
    }

    /// The relabeled carrier under p = [2, 0, 3, 1]: base edges mapped
    /// u→v into p[u]→p[v] (0→1(1.0) becomes 2→0(1.0), 0→2(2.0) becomes
    /// 2→3(2.0), 1→3(3.0) becomes 0→1(3.0), 2→3(0.5) becomes 3→1(0.5)).
    fn relabeled_adjacency() -> Vec<f64> {
        vec![
            0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.5, 0.0, 0.0,
        ]
    }

    const P: [usize; 4] = [2, 0, 3, 1];
    const SQUARE: usize = 4;

    /// relabeled[p[u]] for each u — the relabel law's left side.
    fn relabel_view(relabeled: &[f64]) -> Vec<f64> {
        P.iter().map(|&u| relabeled[u]).collect()
    }

    #[test]
    fn reachability_is_permutation_equivariant() {
        let base = reachable_mask(&base_adjacency(), SQUARE, SQUARE, 0).unwrap();
        let relabeled = reachable_mask(&relabeled_adjacency(), SQUARE, SQUARE, 2).unwrap();
        assert_eq!(
            relabel_view(&relabeled),
            base,
            "reachability relabel law: relabeled[p[u]] == base[u]"
        );
    }

    #[test]
    fn shortest_distances_permute_with_the_relabel() {
        let base = nonnegative_shortest_path(&base_adjacency(), SQUARE, SQUARE, 0).unwrap();
        let relabeled =
            nonnegative_shortest_path(&relabeled_adjacency(), SQUARE, SQUARE, 2).unwrap();
        for old in 0..4usize {
            if base[old].is_finite() {
                assert_eq!(
                    relabeled[P[old]], base[old],
                    "distance relabel law at vertex {old}"
                );
            } else {
                assert!(
                    !relabeled[P[old]].is_finite(),
                    "unreachable stays unreachable at {old}"
                );
            }
        }
    }

    #[test]
    fn out_degrees_permute_with_the_relabel() {
        let base = row_nonzero_counts(&base_adjacency(), SQUARE, SQUARE).unwrap();
        let relabeled = row_nonzero_counts(&relabeled_adjacency(), SQUARE, SQUARE).unwrap();
        assert_eq!(
            relabel_view(&relabeled),
            base,
            "out-degree relabel law: relabeled[p[u]] == base[u]"
        );
    }

    #[test]
    fn laplacian_diagonal_permutes_and_trace_is_invariant() {
        // L = D − A. The spectrum is invariant (P L Pᵀ is a
        // permutation similarity); the full spectrum through the
        // interpreter seam lives at the .emath surface, here we pin
        // the flat invariants that are kernel-visible.
        let base = degree_minus_carrier(&base_adjacency(), SQUARE, SQUARE).unwrap();
        let relabeled = degree_minus_carrier(&relabeled_adjacency(), SQUARE, SQUARE).unwrap();
        let trace = |flat: &[f64]| -> f64 { (0..SQUARE).map(|i| flat[i * SQUARE + i]).sum() };
        assert_eq!(trace(&base), trace(&relabeled));
        let base_diag: Vec<f64> = (0..SQUARE).map(|i| base[i * SQUARE + i]).collect();
        let relabeled_diag: Vec<f64> = (0..SQUARE).map(|i| relabeled[i * SQUARE + i]).collect();
        assert_eq!(
            relabel_view(&relabeled_diag),
            base_diag,
            "degree diagonal permutes"
        );
    }

    #[test]
    fn sparse_round_trip_is_relabel_equivariant() {
        let base_triplets = dense_to_coordinate_stream(&base_adjacency(), SQUARE, SQUARE).unwrap();
        let relabeled_triplets =
            dense_to_coordinate_stream(&relabeled_adjacency(), SQUARE, SQUARE).unwrap();
        for chunk in base_triplets.chunks_exact(3) {
            let [u, v, w] = chunk else {
                unreachable!();
            };
            let found = relabeled_triplets.chunks_exact(3).any(|t| {
                t[0] == P[*u as usize] as f64 && t[1] == P[*v as usize] as f64 && t[2] == *w
            });
            assert!(
                found,
                "relabeled stream must contain the permuted pair ({}, {}, {})",
                P[*u as usize], P[*v as usize], w
            );
        }
        assert_eq!(base_triplets.len(), relabeled_triplets.len());
    }

    /// The discriminator: 0→1, 0→2, 1→3, 2→4. BFS discovers
    /// [0, 1, 2, 3, 4]; a depth-first (LIFO) stack pops 2 before 1 and
    /// discovers 4 before 3 -> [0, 1, 2, 4, 3].
    fn discriminator_adjacency() -> Vec<f64> {
        vec![
            0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ]
    }

    const FIVE: usize = 5;

    #[test]
    fn bfs_order_is_breadth_first_ascending() {
        let order = breadth_order(&discriminator_adjacency(), FIVE, FIVE, 0).unwrap();
        assert_eq!(
            order,
            vec![0.0, 1.0, 2.0, 3.0, 4.0],
            "BFS, never DFS (a LIFO stack discovers 4 before 3)"
        );
    }

    #[test]
    fn dijkstra_tie_break_is_lowest_index() {
        // Equal forks 0→1 (1.0) and 0→2 (1.0): distances are
        // deterministic [0,1,1,2], and re-running is bit-identical
        // (the deterministic tie-break law).
        let flat = vec![
            0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        ];
        let first = nonnegative_shortest_path(&flat, SQUARE, SQUARE, 0).unwrap();
        let second = nonnegative_shortest_path(&flat, SQUARE, SQUARE, 0).unwrap();
        assert_eq!(first, vec![0.0, 1.0, 1.0, 2.0]);
        assert_eq!(first, second);
    }

    #[test]
    fn evaluation_is_bit_identical_for_identical_inputs() {
        let first = breadth_order(&base_adjacency(), SQUARE, SQUARE, 0).unwrap();
        let second = breadth_order(&base_adjacency(), SQUARE, SQUARE, 0).unwrap();
        assert_eq!(first, second, "determinism class: pure sequence");
    }
}

/// Sparse COO build error classification: the refusal
/// names the ACTUAL defect class — a non-multiple-of-three length is
/// `E-GRAPH-006`, an out-of-range/non-integral index ANYWHERE in the
/// stream (u or v, first or later triplet) is `E-GRAPH-003`, a
/// non-finite weight is `E-GRAPH-004` — in scan order, never a guess
/// reverse-engineered from an empty kernel result (the kernel returns
/// empty for every failure class, so only pre-classification can name
/// the true defect).
mod sparse_error_classification {
    use emath_rt::{DenseCarrierError, coordinate_stream_to_dense};

    fn code_of(error: DenseCarrierError) -> String {
        // The wrapper's error carries its class in the Display/code
        // form the interpreter surfaces; pin the code token itself.
        let text = error.to_string();
        text["E-GRAPH-".len()..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .map(|digits| format!("E-GRAPH-{digits}"))
            .unwrap_or(text)
    }

    #[test]
    fn bad_index_in_a_later_triplet_refuses_e_graph_003() {
        // First triplet fine; the second triplet's v is out of range.
        // The refusal must name the index class — not fall through to
        // the weight class (the pre-fix scan only inspected triplet 0's
        // v and misclassified this as E-GRAPH-004).
        let error = coordinate_stream_to_dense(3.0, &[0.0, 1.0, 1.0, 1.0, 9.0, 2.0]).unwrap_err();
        assert_eq!(
            code_of(error),
            "E-GRAPH-003",
            "bad v in triplet 2 must be E-GRAPH-003"
        );
    }

    #[test]
    fn bad_u_index_in_first_triplet_refuses_e_graph_003() {
        // u is out of range in the FIRST triplet: the pre-fix scan only
        // validated v, so this also misclassified as E-GRAPH-004.
        let error = coordinate_stream_to_dense(3.0, &[7.0, 1.0, 1.0]).unwrap_err();
        assert_eq!(code_of(error), "E-GRAPH-003", "bad u must be E-GRAPH-003");
    }

    #[test]
    fn non_finite_weight_refuses_e_graph_004() {
        let error = coordinate_stream_to_dense(3.0, &[0.0, 1.0, f64::NAN]).unwrap_err();
        assert_eq!(code_of(error), "E-GRAPH-004");
    }

    #[test]
    fn malformed_length_refuses_e_graph_006() {
        let error = coordinate_stream_to_dense(3.0, &[0.0, 1.0, 1.0, 2.0]).unwrap_err();
        assert_eq!(code_of(error), "E-GRAPH-006");
    }

    #[test]
    fn non_integral_index_refuses_e_graph_003() {
        // The kernel's index law: finite, integral, 0 <= index < n.
        // A fractional index is an index-class defect, not a weight one.
        let error = coordinate_stream_to_dense(3.0, &[0.5, 1.0, 1.0]).unwrap_err();
        assert_eq!(code_of(error), "E-GRAPH-003");
    }

    #[test]
    fn well_formed_stream_builds_and_parallel_edges_sum() {
        // The COO law: parallel edges add their weights (2.0 + 3.0 = 5.0
        // at (0, 1)); a clean stream builds the dense n×n carrier.
        let built = coordinate_stream_to_dense(2.0, &[0.0, 1.0, 2.0, 0.0, 1.0, 3.0]).unwrap();
        assert_eq!(built, vec![0.0, 5.0, 0.0, 0.0]);
        assert_eq!(built.len(), 4); // n × n
    }
}
