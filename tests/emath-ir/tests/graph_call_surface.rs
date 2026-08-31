//! emath-r2-graphs-masa (slice 2): the `.emath` call surface for graph
//! algorithms.
//!
//! Slice 1 computed the kernels + EMIR ops; this slice opens the call
//! surface through the TERM-COMPILER seam (disjoint from the parser
//! lanes and the sema admission table — that name-table row is the
//! named next slice):
//! - `ParamShape::Matrix` / `Shape::Matrix`: declared Matrix parameters
//!   compile with a Matrix shape (the carrier law from slice 1).
//! - Closed call names bound to the slice-1 EMIR ops:
//!   `reachability(adj, source)`, `bfs_order(adj, source)`,
//!   `shortest_distances(adj, source)`, `out_degrees(adj)`. Wrong
//!   shapes refuse typed at COMPILE (`ShapeMismatch` naming the
//!   requirement) — the closed vocabulary's shape law, never a silent
//!   mis-lowering.
//! - Registry cells `std.graph.*`: the same algorithms as registry
//!   DATA (the fjxh.14 anti-LOC law) — the ApplyCapability path runs
//!   the identical compiled programs.
//! - `E-GRAPH-004`: a NON-FINITE edge weight refuses typed (the
//!   all-finite numeric policy; a NaN weight must never silently
//!   propagate NaN distances through Dijkstra).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn adjacency() -> Value {
    // The slice-1 reference carrier: 0→1, 0→2, 1→3, 2→3.
    Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    }
}

/// Compile a one-call cell through the public `compile_reference` seam
/// and evaluate it via ApplyCapability over `inputs`.
fn cell_seval(
    name: &str,
    operator: &str,
    arity: usize,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    let term = Term::Apply {
        operator: SymbolId(operator.into()),
        arguments: params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect(),
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(operator.into()), arity)
        .expect("single-operator signature is conflict-free");
    let cell = compile_reference(&term, &signature, &params, Vec::new(), name)
        .expect("graph cell compiles through the call surface");
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: name.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn matrix_param(name: &str) -> (String, ParamShape) {
    (name.to_string(), ParamShape::Matrix)
}

fn scalar_param(name: &str) -> (String, ParamShape) {
    (name.to_string(), ParamShape::Scalar)
}

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

#[test]
fn graph_call_surface_reachability_computes() {
    // reachability(adj, source) via the REGISTRY path (ApplyCapability
    // resolves std.graph.* cells): the mask matches the slice-1 kernel
    // law.
    let mask = cell_seval(
        "std.graph.reachability",
        "reachability",
        2,
        vec![matrix_param("adj"), scalar_param("source")],
        &[adjacency(), Value::F64(0.0)],
    )
    .expect("reachability evaluates through the registry path");
    assert_eq!(vector_of(&mask), vec![1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn graph_call_surface_shortest_distances_compute() {
    // Weighted carrier (0→1 w1, 1→2 w1, 0→2 w3): distances [0, 1, 2,
    // +Inf] — the via-path law from slice 1, through the registry path.
    let weighted = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 3.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    };
    let distances = cell_seval(
        "std.graph.shortest_distances",
        "shortest_distances",
        2,
        vec![matrix_param("adj"), scalar_param("source")],
        &[weighted, Value::F64(0.0)],
    )
    .expect("shortest_distances evaluates through the registry path");
    let d = vector_of(&distances);
    assert!((d[1] - 1.0).abs() < 1e-12, "d[1] = 1, got {d:?}");
    assert!((d[2] - 2.0).abs() < 1e-12, "d[2] = 2 via 0→1→2, got {d:?}");
    assert!(d[3].is_infinite() && d[3] > 0.0, "unreachable is +Inf, got {d:?}");
}

#[test]
fn graph_call_surface_out_degrees_compute() {
    let degrees = cell_seval(
        "std.graph.out_degrees",
        "out_degrees",
        1,
        vec![matrix_param("adj")],
        &[adjacency()],
    )
    .expect("out_degrees evaluates through the registry path");
    assert_eq!(vector_of(&degrees), vec![2.0, 1.0, 1.0, 0.0]);
}

#[test]
fn graph_call_surface_shape_law_refuses() {
    // The closed vocabulary's shape law: reachability on a VECTOR-shaped
    // argument refuses at COMPILE (ShapeMismatch naming the call) —
    // never a silent mis-lowering of a vector into the adjacency slot.
    let term = Term::Apply {
        operator: SymbolId("reachability".into()),
        arguments: vec![
            Term::Variable(VariableId("adj".into())),
            Term::Variable(VariableId("source".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("reachability".into()), 2)
        .expect("signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &[
            ("adj".to_string(), ParamShape::Vector),
            ("source".to_string(), ParamShape::Scalar),
        ],
        Vec::new(),
        "surface.shape-law",
    )
    .expect_err("a vector in the adjacency slot refuses at compile");
    let text = format!("{error:?}");
    assert!(
        text.contains("ShapeMismatch") && text.contains("reachability"),
        "shape law must refuse the adjacency slot, got {text}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/graph_call_surface.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-TYPE"),
        "seed expects the shape-class refusal, found: {expect_line}"
    );
}

#[test]
fn registry_carries_standard_graph_cells() {
    // The registry is DATA: the four std.graph cells register with
    // Matrix-typed params and evaluate identically to the bare-name
    // call surface (one algorithmic core, two spellings).
    let registry = std_cell_registry();
    for name in [
        "std.graph.reachability",
        "std.graph.bfs_order",
        "std.graph.shortest_distances",
        "std.graph.out_degrees",
    ] {
        assert!(registry.contains_key(name), "{name} registered");
    }
    let cell = registry
        .get("std.graph.out_degrees")
        .expect("out_degrees cell");
    assert!(
        cell.params
            .iter()
            .all(|(_, shape)| *shape == ParamShape::Matrix),
        "graph cells declare Matrix params, got {:?}",
        cell.params
    );
}

#[test]
fn non_finite_graph_weight_refuses_typed() {
    // E-GRAPH-004 at the BARE-OP path (the all-finite numeric policy):
    // a NaN weight must never silently propagate NaN distances — and
    // `adj[u][v] != 0.0` is TRUE for NaN, so the gate also protects BFS
    // from treating NaN as an edge.
    let nan_carrier = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, f64::NAN, 0.0, 0.0],
    };
    let ops: Vec<(EmirOp, Span)> = vec![
        (EmirOp::LoadInput(0), Span::default()),
        (EmirOp::LoadInput(1), Span::default()),
        (
            EmirOp::GraphDijkstra(EmirValue(0), EmirValue(1)),
            Span::default(),
        ),
    ];
    let result = EmirValue(ops.len() as u32 - 1);
    let error = evaluate_with_budget(
        &EmirProgram {
            ops,
            result,
            input_count: 2,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        &[nan_carrier, Value::F64(0.0)],
        &[],
        EvalBudget::default(),
    )
    .expect_err("non-finite weight refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-004"),
        "non-finite weight must name E-GRAPH-004, got {fault}"
    );
    // The REGISTRY path refuses one layer EARLIER — the cell's declared
    // AllFinite guard fires at the seam (E-CELL-006, the .14 policy
    // law) before the kernel's own E-GRAPH-004 can. Both layers refuse;
    // the registry's declared contract precedes the kernel gate.
    let registry_error = cell_seval(
        "std.graph.shortest_distances",
        "shortest_distances",
        2,
        vec![matrix_param("adj"), scalar_param("source")],
        &[
            Value::Matrix {
                rows: 2,
                cols: 2,
                data: vec![0.0, f64::NAN, 0.0, 0.0],
            },
            Value::F64(0.0),
        ],
    )
    .expect_err("registry-path non-finite weight refuses");
    let registry_fault = format!("{registry_error:?}");
    assert!(
        registry_fault.contains("E-CELL-006"),
        "registry path refuses via the declared all-finite guard, got {registry_fault}"
    );
}

#[test]
fn graph_call_surface_bfs_order_computes() {
    // bfs_order through the registry path: the breadth-first law from
    // slice 1 (ascending-index discovery, never DFS).
    let order = cell_seval(
        "std.graph.bfs_order",
        "bfs_order",
        2,
        vec![matrix_param("adj"), scalar_param("source")],
        &[adjacency(), Value::F64(0.0)],
    )
    .expect("bfs_order evaluates through the registry path");
    assert_eq!(vector_of(&order), vec![0.0, 1.0, 2.0, 3.0]);
}
