//! The `.emath` call surface for graph
//! algorithms.
//!
//! The kernels and EMIR ops already landed; this file opens the computed the kernels + EMIR ops; this slice opens the call
//! surface through the TERM-COMPILER seam (disjoint from the parser
//! lanes and the sema admission table — that name-table row is the
//! named next slice):
//! - Capability cells `std.capability.graph.*` bind the graph kernels
//!   through the checked-in Language Image; the `Matrix<Float64>`
//!   adjacency carrier law is the capsules' declared input shape, and
//!   mis-shaped arguments refuse typed at the kernel ABI
//!   (`E-TYPE-012`) — never a silent mis-lowering.
//! - The ApplyCapability path runs the identical kernels over real
//!   FeatureIDs (the anti-LOC law; no domain-named `EmirOp`).
//! - `E-GRAPH-004`: a NON-FINITE edge weight refuses typed (the
//!   all-finite numeric policy; a NaN weight must never silently
//!   propagate NaN distances through Dijkstra).

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};

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

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
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

fn adjacency() -> Value {
    // The reference carrier: 0→1, 0→2, 1→3, 2→3.
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

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

#[test]
fn graph_call_surface_reachability_computes() {
    // reachability(adj, source) through the capability path
    // (ApplyCapability resolves the std.capability.graph.* kernels):
    // the mask matches the graph kernel law.
    let mask = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[adjacency(), Value::F64(0.0)],
    )
    .expect("reachability evaluates through the capability path");
    assert_eq!(vector_of(&mask), vec![1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn graph_call_surface_shortest_distances_compute() {
    // Weighted carrier (0→1 w1, 1→2 w1, 0→2 w3): distances [0, 1, 2,
    // +Inf] — the via-path law from the graph core, through the registry path.
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
    let distances = eval(
        vec![cell(SHORTEST_DISTANCES, vec![EmirValue(0), EmirValue(1)])],
        &[weighted, Value::F64(0.0)],
    )
    .expect("shortest_distances evaluates through the capability path");
    let d = vector_of(&distances);
    assert!((d[1] - 1.0).abs() < 1e-12, "d[1] = 1, got {d:?}");
    assert!((d[2] - 2.0).abs() < 1e-12, "d[2] = 2 via 0→1→2, got {d:?}");
    assert!(
        d[3].is_infinite() && d[3] > 0.0,
        "unreachable is +Inf, got {d:?}"
    );
}

#[test]
fn graph_call_surface_out_degrees_compute() {
    let degrees = eval(vec![cell(OUT_DEGREES, vec![EmirValue(0)])], &[adjacency()])
        .expect("out_degrees evaluates through the capability path");
    assert_eq!(vector_of(&degrees), vec![2.0, 1.0, 1.0, 0.0]);
}

#[test]
fn graph_call_surface_shape_law_refuses() {
    // The closed vocabulary's shape law: reachability on a VECTOR-shaped
    // argument refuses typed at the kernel ABI (E-TYPE-012 naming the
    // Matrix<Float64> requirement) — never a silent mis-lowering of a
    // vector into the adjacency slot.
    install_language();
    let error = eval(
        vec![cell(REACHABILITY, vec![EmirValue(0), EmirValue(1)])],
        &[Value::Vector(vec![0.0, 1.0, 1.0, 0.0]), Value::F64(0.0)],
    )
    .expect_err("a vector in the adjacency slot refuses");
    let text = format!("{error:?}");
    assert!(
        text.contains("E-TYPE-012") && text.contains("Matrix<Float64>"),
        "shape law must refuse the adjacency slot, got {text}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/graph_call_surface.emath");
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
    // The distribution is DATA: the four std.capability.graph cells
    // bind through the checked-in Language Image with Matrix-typed
    // adjacency signatures and evaluate identically to the call
    // surface (one algorithmic core, two spellings).
    install_language();
    for feature_id in [REACHABILITY, BFS_ORDER, SHORTEST_DISTANCES, OUT_DEGREES] {
        assert!(native_kernel(feature_id).is_some(), "{feature_id} bound");
    }
    let descriptor = native_kernel(REACHABILITY).expect("reachability bound");
    assert!(
        descriptor.signature.contains("Matrix<Float64>"),
        "graph cells declare Matrix adjacency params, got {}",
        descriptor.signature
    );
}

#[test]
fn non_finite_graph_weight_refuses_typed() {
    // E-GRAPH-004 at the capability path (the all-finite numeric
    // policy): a NaN weight must never silently propagate NaN
    // distances — and `adj[u][v] != 0.0` is TRUE for NaN, so the gate
    // also protects BFS from treating NaN as an edge. The kernel's
    // typed refusal surfaces verbatim through ApplyCapability.
    install_language();
    let nan_carrier = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, f64::NAN, 0.0, 0.0],
    };
    let error = eval(
        vec![cell(SHORTEST_DISTANCES, vec![EmirValue(0), EmirValue(1)])],
        &[nan_carrier, Value::F64(0.0)],
    )
    .expect_err("non-finite weight refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-004"),
        "non-finite weight must name E-GRAPH-004, got {fault}"
    );
}

#[test]
fn graph_call_surface_bfs_order_computes() {
    // bfs_order through the capability path: the breadth-first law from
    // the graph core (ascending-index discovery, never DFS).
    let order = eval(
        vec![cell(BFS_ORDER, vec![EmirValue(0), EmirValue(1)])],
        &[adjacency(), Value::F64(0.0)],
    )
    .expect("bfs_order evaluates through the capability path");
    assert_eq!(vector_of(&order), vec![0.0, 1.0, 2.0, 3.0]);
}
