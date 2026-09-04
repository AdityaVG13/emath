//! Spectral graph basics.
//!
//! Scope: the unnormalized Laplacian:
//! `graph_laplacian(adj)` = D − A (D the
//! out-degree diagonal, A the adjacency carrier). The spectrum then
//! composes through the EXISTING `EigenSymmetric` op — zero new
//! spectral machinery.
//!
//! Class fences (documented, each tested):
//! - The Laplacian of an UNDIRECTED graph (symmetric adjacency) is
//!   symmetric; its spectrum is real with the algebraic-connectivity
//!   law (smallest eigenvalue 0 ⇔ ... for the path graph P4:
//!   {0, 2−√2, 2, 2+√2}).
//! - The Laplacian of a DIRECTED carrier is NOT symmetric, and the
//!   symmetric-only eigen gate refuses it (`E-LINALG-002`) — the
//!   honest class fence, never a silently symmetrized spectrum.
//! - The negative weight gate (`E-GRAPH-002`) still applies: a
//!   negative adjacency entry is not a graph carrier for D − A.

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::term_compile::{
    ParamShape, TermCompileError, compile_reference, std_cell_registry,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

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

const LAPLACIAN: &str = "std.capability.graph.laplacian";
const EIGENVALUES: &str = "std.capability.linear.symmetric-eigenvalues";

fn undirected_path4() -> Value {
    // Undirected path 0–1–2–3 (symmetric adjacency, unweighted).
    Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            1.0, 0.0, 1.0, 0.0, //
            0.0, 1.0, 0.0, 1.0, //
            0.0, 0.0, 1.0, 0.0,
        ],
    }
}

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    // The seam law: LoadInput per input, result = last register.
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

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

fn matrix_param(name: &str) -> (String, ParamShape) {
    (name.to_string(), ParamShape::Matrix)
}

/// Registry-path evaluation of a one-matrix-arg graph cell: compile
/// against the registry spelling, evaluate the capsule FeatureID.
fn cell_seval(
    registry_name: &str,
    feature_id: &str,
    operator: &str,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    install_language();
    let term = Term::Apply {
        operator: SymbolId(operator.into()),
        arguments: vec![Term::Variable(VariableId("adj".into()))],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(operator.into()), 1)
        .expect("single-operator signature is conflict-free");
    let compiled = compile_reference(
        &term,
        &signature,
        &[matrix_param("adj")],
        vec![emath_exec_ir::term_compile::ArgGuard::AllFinite(0)],
        registry_name,
    )
    .expect("graph cell compiles");
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: feature_id.to_string(),
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

#[test]
fn graph_laplacian_computes() {
    // D − A over the undirected path: out-degrees [1, 2, 2, 1], so
    // L = diag(1,2,2,1) − A.
    let laplacian = eval(
        vec![cell(LAPLACIAN, vec![EmirValue(0)])],
        &[undirected_path4()],
    )
    .expect("laplacian computes");
    let Value::Matrix { rows, cols, data } = laplacian else {
        panic!("expected a matrix, got {laplacian:?}")
    };
    assert_eq!((rows, cols), (4, 4));
    let expected = [
        1.0, -1.0, 0.0, 0.0, //
        -1.0, 2.0, -1.0, 0.0, //
        0.0, -1.0, 2.0, -1.0, //
        0.0, 0.0, -1.0, 1.0,
    ];
    for (got, want) in data.iter().zip(expected.iter()) {
        assert!((got - want).abs() < 1e-12, "L = D − A: {data:?}");
    }
}

#[test]
fn laplacian_spectrum_composes_through_eigen() {
    // The spectral law (P4 path graph): eigenvalues of L are
    // {0, 2−√2, 2, 2+√2} ascending — the composition
    // eigvals(graph_laplacian(A)) computes through the EXISTING
    // symmetric eigen op, zero new spectral machinery.
    let spectrum = eval(
        vec![
            cell(LAPLACIAN, vec![EmirValue(0)]),
            cell(EIGENVALUES, vec![EmirValue(1)]),
        ],
        &[undirected_path4()],
    )
    .expect("laplacian spectrum composes");
    let values = vector_of(&spectrum);
    assert_eq!(values.len(), 4);
    let expected = [0.0, 2.0 - 2.0_f64.sqrt(), 2.0, 2.0 + 2.0_f64.sqrt()];
    for (got, want) in values.iter().zip(expected.iter()) {
        assert!(
            (got - want).abs() < 1e-9,
            "P4 Laplacian spectrum {{0, 2−√2, 2, 2+√2}}, got {values:?}"
        );
    }
    assert!(values[0].abs() < 1e-9, "the smallest eigenvalue is 0");
}

#[test]
fn directed_laplacian_spectrum_refuses_typed() {
    // The class fence: the directed reference carrier's Laplacian is
    // NOT symmetric, and the symmetric-only eigen gate refuses it
    // (E-LINALG-002) — never a silently symmetrized spectrum. The
    // Laplacian itself still computes (D − A is well-defined).
    let directed = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    };
    let error = eval(
        vec![
            cell(LAPLACIAN, vec![EmirValue(0)]),
            cell(EIGENVALUES, vec![EmirValue(1)]),
        ],
        &[directed],
    )
    .expect_err("directed Laplacian spectrum refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LINALG-002"),
        "directed Laplacian spectrum must name the symmetric gate, got {fault}"
    );
}

#[test]
fn non_square_laplacian_refuses_typed() {
    // The carrier law from the graph core: a non-square adjacency matrix is
    // not a graph carrier (E-GRAPH-001) — the negative seed's shape.
    let rectangular = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };
    let error = eval(vec![cell(LAPLACIAN, vec![EmirValue(0)])], &[rectangular])
        .expect_err("non-square adjacency refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-GRAPH-001"),
        "non-square Laplacian must name E-GRAPH-001, got {fault}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/spectral_graph_asymmetry.emath");
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
fn laplacian_registry_cell_computes() {
    // std.graph.laplacian: the same kernel as registry DATA (the
    // anti-LOC law), evaluated through the ApplyCapability path.
    let registry = std_cell_registry();
    assert!(
        registry.contains_key("std.graph.laplacian"),
        "std.graph.laplacian registered"
    );
    let laplacian = cell_seval(
        "std.graph.laplacian",
        "std.capability.graph.laplacian",
        "graph_laplacian",
        &[undirected_path4()],
    )
    .expect("registry cell evaluates");
    let Value::Matrix { data, .. } = laplacian else {
        panic!("expected a matrix, got {laplacian:?}")
    };
    assert!((data[0] - 1.0).abs() < 1e-12 && (data[1] + 1.0).abs() < 1e-12);
}

#[test]
fn laplacian_call_surface_shape_law_refuses() {
    // A vector in the adjacency slot refuses at COMPILE (the closed
    // vocabulary's shape law, the call-surface law extended to the new name).
    let term = Term::Apply {
        operator: SymbolId("graph_laplacian".into()),
        arguments: vec![Term::Variable(VariableId("adj".into()))],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("graph_laplacian".into()), 1)
        .expect("signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &[("adj".to_string(), ParamShape::Vector)],
        Vec::new(),
        "surface.shape-law-3",
    )
    .expect_err("a vector in the adjacency slot refuses at compile");
    let text = format!("{error:?}");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "shape law must refuse the adjacency slot, got {text}"
    );
}
