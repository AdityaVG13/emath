//! (B34+B35): linear solves, decompositions,
//! and norms/inner products.
//!
//! The law, scoped along the file-disjoint seams:
//! - **B35 norms/inner product** enter as REGISTRY DATA (quoted terms
//!   over the closed vector vocabulary the interp already executes):
//!   `std.linalg.norm` (L2 default, the generic VectorNorm op),
//!   `std.linalg.norm1` (L1 = sum of abs), `std.linalg.norminf`
//!   (Linf = max of abs), `std.linalg.inner_product` (the generic dot).
//!   Zero per-op VM code, zero core branches.
//! - **B34 distinction:** `solve` is the nonlinear root-finding GOAL
//!   command; `solve_linear`/`lu`/`qr`/`outer_product` need MATRIX
//!   carriers that are outside the closed reference vocabulary — a cell
//!   calling them refuses typed (`TermCompileError::UnknownOperator`,
//!   the matmul precedent), diagnosing the missing matrix nucleus.
//!   Never a silent alias of `solve`, never a wrong answer.
//! - The `p=` keyword for `norm` and the C8 quantifier `in` fix
//!   belong to the parser (tree.rs/stmt_suite) — deferred with
//!   the boundary named in the pack.

use emath_core::Span;
use emath_core::limits::Limits;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{ParamShape, std_cell_registry};
use emath_exec_ir::{EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use emath_term::{Signature, SymbolId, Term, VariableId};
use std::collections::BTreeMap;

/// The .14 seam for a registry cell: load inputs, then one
/// ApplyCapability. This is the CELL path.
fn seam_eval(cell: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: cell.to_string(),
            class: emath_exec_ir::CellClass::Pure,
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

fn f64_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

#[test]
fn euclidean_norm_computes() {
    // B35: `norm(v, p=2)` — L2 is the default norm; the registry cell
    // lowers to the generic VectorNorm op.
    let registry = std_cell_registry();
    let cell = registry
        .get("std.linalg.norm")
        .expect("std.linalg.norm is a registry cell");
    let value =
        seam_eval("std.linalg.norm", &[Value::Vector(vec![3.0, 4.0])]).expect("norm evaluates");
    assert!(
        (f64_of(&value) - 5.0).abs() < 1e-12,
        "‖[3,4]‖₂ = 5, got {value:?}"
    );
    let _ = cell;
}

#[test]
fn one_and_infinity_norms_compute() {
    // B35: L1 = sum |v_i|; Linf = max |v_i| — both compose the closed
    // vocabulary (abs map + sum/vmax reduces), no new ops.
    let value =
        seam_eval("std.linalg.norm1", &[Value::Vector(vec![3.0, -4.0])]).expect("norm1 evaluates");
    assert!(
        (f64_of(&value) - 7.0).abs() < 1e-12,
        "‖[3,-4]‖₁ = 7, got {value:?}"
    );
    let value = seam_eval("std.linalg.norminf", &[Value::Vector(vec![3.0, -4.0])])
        .expect("norminf evaluates");
    assert!(
        (f64_of(&value) - 4.0).abs() < 1e-12,
        "‖[3,-4]‖∞ = 4, got {value:?}"
    );
}

#[test]
fn inner_product_computes() {
    // B35: inner_product(u, v) — the generic dot; length mismatch
    // refuses typed (the interp's vector-length law).
    let value = seam_eval(
        "std.linalg.inner_product",
        &[
            Value::Vector(vec![1.0, 2.0, 3.0]),
            Value::Vector(vec![4.0, 5.0, 6.0]),
        ],
    )
    .expect("inner product evaluates");
    assert!(
        (f64_of(&value) - 32.0).abs() < 1e-12,
        "⟨[1,2,3],[4,5,6]⟩ = 32, got {value:?}"
    );
}

#[test]
fn linear_solve_is_distinct_and_typed() {
    // B34: `solve_linear` is a matrix operation, not the nonlinear
    // `solve` goal. The registry path computes the known system.
    let solved = seam_eval(
        "std.linalg.solve_linear",
        &[
            matrix(2, 2, &[3.0, 1.0, 1.0, 2.0]),
            Value::Vector(vec![9.0, 8.0]),
        ],
    )
    .expect("nonsingular system solves");
    assert_eq!(solved, Value::Vector(vec![2.0, 3.0]));

    let outer = seam_eval(
        "std.linalg.outer_product",
        &[
            Value::Vector(vec![1.0, 2.0]),
            Value::Vector(vec![3.0, 4.0, 5.0]),
        ],
    )
    .expect("outer product computes");
    assert_eq!(outer, matrix(2, 3, &[3.0, 4.0, 5.0, 6.0, 8.0, 10.0]));

    for cell in ["std.linalg.lu", "std.linalg.qr"] {
        let factors = seam_eval(cell, &[matrix(2, 2, &[4.0, 3.0, 6.0, 3.0])])
            .expect("factorization computes");
        assert!(
            matches!(factors, Value::Matrix { .. }),
            "{cell}: {factors:?}"
        );
    }

    // Wrong carriers still refuse at compile with the shape diagnosis.
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("solve_linear".into()), 2)
        .expect("conflict-free");
    let term = Term::Apply {
        operator: SymbolId("solve_linear".into()),
        arguments: vec![
            Term::Variable(VariableId("A".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let error = emath_exec_ir::term_compile::compile_reference(
        &term,
        &signature,
        &[
            ("A".to_string(), ParamShape::Scalar),
            ("b".to_string(), ParamShape::Vector),
        ],
        Vec::new(),
        "test.solve_linear",
    )
    .expect_err("scalar matrix carrier refuses");
    assert!(
        matches!(
            error,
            emath_exec_ir::term_compile::TermCompileError::ShapeMismatch { .. }
        ),
        "wrong carrier must be a shape refusal, got {error:?}"
    );
}

#[test]
fn strict_linear_algebra_source_compiles() {
    // E2E: strict source admits and evaluates the matrix vocabulary.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "emath function geometry:\n    inputs: []\n    outputs:\n        y: Float64\n    definitions:\n        lu_pack = lu([[4, 3], [6, 3]])\n        qr_pack = qr([[4, 3], [6, 3]])\n        y = solve_linear([[3, 1], [1, 2]], [9, 8])[0] + outer_product([1, 2], [3, 4])[0, 0]\n";
    let result = session.check_owned("geometry", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.is_empty(),
        "a linear-algebra model compiles, got {codes:?} (messages: {:?})",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
    let values = emath_exec_ir::runner::eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("matrix source evaluates");
    assert_eq!(values.get("y"), Some(&Value::F64(5.0)));
}

#[test]
fn linear_algebra_result_bundle_is_complete() {
    // WorldResultBundle fixture (e2e clause; the cell path is touched):
    // the labeled world verdict records the norm family computing
    // through the registry path.
    struct LinearWorld;
    impl emath_genesis::FirstOrderWorld for LinearWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let l2 = seam_eval("std.linalg.norm", &[Value::Vector(vec![3.0, 4.0])])
                .map(|v| f64_of(&v))
                .unwrap_or(f64::NAN);
            let l1 = seam_eval("std.linalg.norm1", &[Value::Vector(vec![3.0, -4.0])])
                .map(|v| f64_of(&v))
                .unwrap_or(f64::NAN);
            let ip = seam_eval(
                "std.linalg.inner_product",
                &[Value::Vector(vec![1.0, 2.0]), Value::Vector(vec![3.0, 4.0])],
            )
            .map(|v| f64_of(&v))
            .unwrap_or(f64::NAN);
            if (l2 - 5.0).abs() < 1e-12 && (l1 - 7.0).abs() < 1e-12 && (ip - 11.0).abs() < 1e-12 {
                Ok("norm-family-computes".to_string())
            } else {
                Ok("norm-family-diverged".to_string())
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
                "linear-solves-b35",
                &["registry-is-data", "typed-missing-nucleus"],
            )
        }
    }

    let term = Term::Constant(SymbolId("linear[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &LinearWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "linear-solves-b35");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
