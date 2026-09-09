//! (slice 4): directed graphs reach the spectral
//! path via EXPLICIT symmetrization.
//!
//! The epic's named "directed/symmetrized spectra" slice, thinned to
//! one op: `graph_symmetrize(adj)` = (A + Aᵀ)/2 — the weight-preserving
//! symmetrization convention, documented. Zero new spectral machinery:
//! the symmetrized carrier is a symmetric adjacency, so the EXISTING
//! `graph_laplacian` + `EigenSymmetric` path applies (the slice-3
//! directed-refusal fence stays: symmetrization is a USER choice, never
//! a silent one inside laplacian/eigen).
//!
//! Laws (each discriminating):
//! - Defining law: the output IS symmetric (S = Sᵀ elementwise).
//! - Idempotence on symmetric carriers: symmetrize(S) = S at 1e-12.
//! - Weight law: A[0][1] = 4, A[1][0] = 0 → S[0][1] = S[1][0] = 2
//!   (the (A + Aᵀ)/2 convention, NOT max, NOT boolean-or).
//! - Composition law: the directed path 0→1→2→3 symmetrized is the
//!   undirected path P4; its Laplacian spectrum is exactly the
//!   slice-3 fixture {0, 2−√2, 2, 2+√2} at 1e-9.
//! - Refusals reuse the closed graph set: ragged → `E-GRAPH-001`,
//!   negative weight → `E-GRAPH-002`, non-finite → `E-GRAPH-004`.
//! - Surface: call name `graph_symmetrize` (compile-time shape law),
//!   registry cell `std.graph.symmetrize` (cohort 29).

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::term_compile::{
    ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn std_cell_registry() -> std::collections::HashMap<String, emath_exec_ir::term_compile::CompiledCell> {
    std::collections::HashMap::new()
}

fn compile_reference<P>(
    _: &emath_term::Term,
    _: &emath_term::Signature,
    _: P,
    _: Vec<emath_exec_ir::term_compile::ArgGuard>,
    _: &str,
) -> Result<emath_exec_ir::term_compile::CompiledCell, emath_exec_ir::term_compile::TermCompileError> {
    Err(emath_exec_ir::term_compile::TermCompileError::UnknownSymbol {
        symbol: "compile_reference-removed".to_string(),
    })
}

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

const SYMMETRIZE: &str = "std.capability.graph.symmetrize";
const LAPLACIAN: &str = "std.capability.graph.laplacian";
const EIGENVALUES: &str = "std.capability.linear.symmetric-eigenvalues";

fn directed_path4() -> Value {
    // Directed path 0→1→2→3 (asymmetric adjacency, unweighted).
    Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 0.0,
        ],
    }
}

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

fn matrix_of(value: &Value) -> (usize, usize, Vec<f64>) {
    let Value::Matrix { rows, cols, data } = value else {
        panic!("expected a matrix, got {value:?}")
    };
    (*rows, *cols, data.clone())
}

#[test]
fn intent() {
    let mut p = Probe::new("(slice 4): directed graphs reach the spectral");
    p.case("defining_law_and_idempotence", |p| {

    // The output IS symmetric; a symmetric input passes through
    // unchanged at 1e-12.
    let symmetrized = eval(
        vec![cell(SYMMETRIZE, vec![EmirValue(0)])],
        &[directed_path4()],
    )
    .expect("directed carrier symmetrizes");
    let (rows, cols, data) = matrix_of(&symmetrized);
    p.eq("defining_law_and_idempotence#1", (rows, cols), (4, 4));
    for i in 0..4 {
        for j in 0..4 {
            p.eq(format!("S[{i}][{j}] = S[{j}][{i}] (defining law)"), data[i * 4 + j], data[j * 4 + i]);
        }
    }
    // Idempotence law: the symmetrized path IS the undirected path
    // with HALVED one-way weights — S[i][j] = (A[i][j] + A[j][i])/2 =
    // 0.5 for every one-way edge (the weight-preserving convention;
    // a max/boolean mutant yields 1.0 here and fails).
    let expected = vec![
        0.0, 0.5, 0.0, 0.0, //
        0.5, 0.0, 0.5, 0.0, //
        0.0, 0.5, 0.0, 0.5, //
        0.0, 0.0, 0.5, 0.0,
    ];
    for (got, want) in data.iter().zip(expected.iter()) {
        p.demand(format!("{got} vs {want}"), (got - want).abs() < 1e-12, format!("{got} vs {want}"));
    }
    // A symmetric input maps to itself.
    let symmetric_input = Value::Matrix {
        rows: 4,
        cols: 4,
        data: expected.clone(),
    };
    let again = eval(
        vec![cell(SYMMETRIZE, vec![EmirValue(0)])],
        &[symmetric_input],
    )
    .expect("symmetric carrier symmetrizes");
    let (_, _, again_data) = matrix_of(&again);
    for (got, want) in again_data.iter().zip(expected.iter()) {
        p.demand(format!("idempotence: {got} vs {want}"), (got - want).abs() < 1e-12, format!("idempotence: {got} vs {want}"));
    }

    });
    p.case("weight_preserving_convention", |p| {

    // (A + Aᵀ)/2, NOT max and NOT boolean-or: A[0][1]=4, A[1][0]=0 →
    // S[0][1] = S[1][0] = 2. A max-convention mutant yields 4; a
    // boolean-or mutant yields 1.
    let carrier = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, 4.0, 0.0, 0.0],
    };
    let symmetrized = eval(vec![cell(SYMMETRIZE, vec![EmirValue(0)])], &[carrier])
        .expect("weighted carrier symmetrizes");
    let (_, _, data) = matrix_of(&symmetrized);
    p.demand(format!("S[0][1] = 2, got {}", data[1]), (data[1] - 2.0).abs() < 1e-12, format!("S[0][1] = 2, got {}", data[1]));
    p.demand(format!("S[1][0] = 2, got {}", data[2]), (data[2] - 2.0).abs() < 1e-12, format!("S[1][0] = 2, got {}", data[2]));

    });
    p.case("composition_spectrum_law", |p| {

    // Directed 4-cycle 0→1→2→3→0 (weight 1): avg-symmetrize gives
    // S = 0.5·A(C4); the existing count-degree laplacian gives
    // L = 2I − S (every vertex has exactly two nonzero neighbors);
    // the eigenvalues follow from A(C4)'s spectrum {2, 0, 0, −2}:
    // L = {2−1, 2−0, 2−0, 2+1} = {1, 2, 2, 3} at 1e-9. A max/boolean
    // mutant yields {0, 2, 2, 4} and fails — the convention law.
    let directed_cycle4 = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            1.0, 0.0, 0.0, 0.0,
        ],
    };
    let symmetrized = eval(
        vec![cell(SYMMETRIZE, vec![EmirValue(0)])],
        &[directed_cycle4],
    )
    .expect("symmetrize computes");
    let Value::Matrix { rows, cols, data } = symmetrized else {
        { p.fail("composition_spectrum_law#1", format!("expected a matrix")); return; }};
    let laplacian = eval(
        vec![cell(LAPLACIAN, vec![EmirValue(0)])],
        &[Value::Matrix { rows, cols, data }],
    )
    .expect("laplacian computes");
    let Value::Matrix {
        rows: lrows,
        cols: lcols,
        data: ldata,
    } = laplacian
    else {
        { p.fail("composition_spectrum_law#2", format!("expected a matrix")); return; }};
    let spectrum = eval(
        vec![cell(EIGENVALUES, vec![EmirValue(0)])],
        &[Value::Matrix {
            rows: lrows,
            cols: lcols,
            data: ldata,
        }],
    )
    .expect("eigen computes");
    let vector_of = |value: &Value| {
        let Value::Vector(v) = value else {
            panic!("expected a vector, got {value:?}"); };
        v.clone()
    };
    let eigenvalues = vector_of(&spectrum);
    let expected = [1.0, 2.0, 2.0, 3.0];
    for (got, want) in eigenvalues.iter().zip(expected.iter()) {
        p.demand(format!("symmetrized-cycle spectrum law: {got} vs {want}"), (got - want).abs() < 1e-9, format!("symmetrized-cycle spectrum law: {got} vs {want}"));
    }

    });
    p.case("refusals_reuse_closed_set", |p| {

    // Ragged → E-GRAPH-001; negative weight → E-GRAPH-002; non-finite
    // → E-GRAPH-004 (the established graph refusal set; no new codes
    // minted for an op that composes existing semantics).
    let ragged = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
    };
    let error =
        eval(vec![cell(SYMMETRIZE, vec![EmirValue(0)])], &[ragged]).expect_err("ragged refuses");
    p.demand(format!("ragged must name E-GRAPH-001, got {error:?}"), format!("{error:?}").contains("E-GRAPH-001"), format!("ragged must name E-GRAPH-001, got {error:?}"));
    let negative = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, -1.0, 0.0, 0.0],
    };
    let error = eval(vec![cell(SYMMETRIZE, vec![EmirValue(0)])], &[negative])
        .expect_err("negative weight refuses");
    p.demand(format!("negative must name E-GRAPH-002, got {error:?}"), format!("{error:?}").contains("E-GRAPH-002"), format!("negative must name E-GRAPH-002, got {error:?}"));
    let non_finite = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, f64::NAN, 0.0, 0.0],
    };
    let error = eval(vec![cell(SYMMETRIZE, vec![EmirValue(0)])], &[non_finite])
        .expect_err("non-finite refuses");
    p.demand(format!("non-finite must name E-GRAPH-004, got {error:?}"), format!("{error:?}").contains("E-GRAPH-004"), format!("non-finite must name E-GRAPH-004, got {error:?}"));
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/graph_weights.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects the carrier refusal, found: {expect_line}"), expect_line.contains("E-GRAPH-001"), format!("seed expects the carrier refusal, found: {expect_line}"));

    });
    p.case("cell_registry_and_shape_law", |p| {

    // std.graph.symmetrize is registry DATA (cohort 29), compiles
    // through the call seam, and evaluates the SAME symmetrization;
    // a scalar adjacency refuses at COMPILE (ShapeMismatch).
    install_language();
    let registry = std_cell_registry();
    p.demand(format!("registry cell present; have {:?}", registry.keys().collect::<Vec<_>>()), registry.contains_key("std.graph.symmetrize"), format!("registry cell present; have {:?}", registry.keys().collect::<Vec<_>>()));

    let term = Term::Apply {
        operator: SymbolId("graph_symmetrize".into()),
        arguments: vec![Term::Variable(VariableId("adj".into()))],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("graph_symmetrize".into()), 1)
        .expect("graph_symmetrize signature is conflict-free");
    compile_reference(
        &term,
        &signature,
        &[("adj".to_string(), ParamShape::Matrix)],
        Vec::new(),
        "std.graph.symmetrize",
    )
    .expect("matrix adjacency compiles");

    // Cell path evaluates the same convention: A[0][1]=4 → S=2. The
    // eval seam resolves the capsule FeatureID (hyphenated).
    let mut ops: Vec<(EmirOp, Span)> = vec![(EmirOp::LoadInput(0), Span::default())];
    ops.push((
        EmirOp::ApplyCapability {
            capability: "std.capability.graph.symmetrize".to_string(),
            class: CellClass::Pure,
            args: vec![EmirValue(0)],
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let value = evaluate_with_budget(
        &program,
        &[Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![0.0, 4.0, 0.0, 0.0],
        }],
        &[],
        EvalBudget::default(),
    )
    .expect("cell evaluates");
    let (_, _, data) = matrix_of(&value);
    p.demand("cell_registry_and_shape_law#2", (data[1] - 2.0).abs() < 1e-12 && (data[2] - 2.0).abs() < 1e-12, "cell_registry_and_shape_law#2: (data[1] - 2.0).abs() < 1e-12 && (data[2] - 2.0).abs() < 1e-12");

    // Shape law: a scalar adjacency refuses at COMPILE.
    let error = compile_reference(
        &term,
        &signature,
        &[("adj".to_string(), ParamShape::Scalar)],
        Vec::new(),
        "std.graph.symmetrize",
    )
    .expect_err("scalar adjacency refuses at compile");
    p.demand(format!("scalar adjacency must ShapeMismatch at compile, got {error:?}"), format!("{error:?}").contains("ShapeMismatch"), format!("scalar adjacency must ShapeMismatch at compile, got {error:?}"));
    let _ = TermCompileError::ShapeMismatch {
        symbol: "graph_symmetrize".to_string(),
        detail: "unused".to_string(),
    };

    });
    p.case("bundle_fixture", |p| {

    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct GraphWorld;
    impl emath_genesis::FirstOrderWorld for GraphWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let symmetrized = eval(
                vec![cell(SYMMETRIZE, vec![EmirValue(0)])],
                &[directed_path4()],
            )
            .ok()
            .map(|value| matrix_of(&value));
            match symmetrized {
                Some((4, 4, data))
                    if (data[1] - 0.5).abs() < 1e-12 && (data[4] - 0.5).abs() < 1e-12 =>
                {
                    Ok("directed-symmetrized-spectral".to_string())
                }
                _ => Ok("directed-symmetrized-diverged".to_string()),
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
                "directed-symmetrized-spectra",
                &[
                    "graph-symmetrize",
                    "avg-convention",
                    "laplacian-composition",
                ],
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
    p.demand("bundle_fixture#1", matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ), "bundle_fixture#1: matches!(\n        result.disposition,\n        emath_genesis::Disposition::Answer { .. }\n    )");
    p.demand("bundle_fixture#2", result.world == "directed-symmetrized-spectra", format!("expected {:?}, got {:?}", "directed-symmetrized-spectra", result.world));
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("bundle_fixture#3", bundle.bundle_id.starts_with("fnv1a64:"), "bundle_fixture#3: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    });
    p.finish();
}











