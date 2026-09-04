//! Ten-operation anti-LOC migration cohort.
//!
//! The architecture migrates real existing ops to
//! capability cells on the DUAL PATH — the handwritten generic-op program
//! (the VM nucleus arm, inlined directly) vs the registry-compiled cell
//! through the cell seam — and the differential must agree BIT-FOR-BIT
//! under the declared numeric policy. Ops the closed vocabulary cannot
//! express (matmul: matrix carrier shapes; RK4: integrator loops) refuse
//! typed and DIAGNOSE the missing nucleus — never a silent wrong
//! lowering. Cell identity is frozen: an identity-affecting numeric-
//! policy mutation of a frozen cohort cell refuses typed (E-CELL-003,
//! the negative seed's silent-success scenario). Rollback is independent
//! per op: every registry entry is standalone data.

use std::collections::BTreeMap;

use emath_core::QualifiedName;
use emath_core::Span;
use emath_exec_ir::interp::{Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    ArgGuard, ParamShape, TermCompileError, compile_reference, std_cell_registry,
};
use emath_exec_ir::{BuiltinId, CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, ReduceId};
use emath_ir::capability::{
    CellClass as SchemaClass, CellSchema, MigrationPolicy, admit_cell_mutation,
    softmax_reference_strict_f64,
};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// The cell seam for a registry cell: load inputs, then one
/// ApplyCapability. This is the CELL path.
fn seam_eval(cell: &str, inputs: &[Value]) -> Value {
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: cell.to_string(),
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
        .expect("seam evaluates the registry cell")
}

/// The HANDWRITTEN path: the same computation inlined as a plain op
/// program over constants (or the input register for vector args).
fn direct_eval(ops: Vec<EmirOp>, consts: &[Value], result: u32) -> Value {
    let program = EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(result),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, consts, &[], EvalBudget::default())
        .expect("direct program evaluates")
}

fn const_regs(values: &[f64]) -> Vec<EmirOp> {
    values
        .iter()
        .map(|&v| EmirOp::ConstF64(f64_bits(v)))
        .collect()
}

/// Bit-exact scalar parity (Bool compares directly).
fn assert_parity(label: &str, cell: &Value, direct: &Value) {
    match (cell, direct) {
        (Value::F64(a), Value::F64(b)) => assert_eq!(
            f64_bits(*a),
            f64_bits(*b),
            "{label}: bit-exact parity broken ({a} vs {b})"
        ),
        (Value::Bool(a), Value::Bool(b)) => assert_eq!(a, b, "{label}: boolean parity"),
        other => panic!("{label}: shape confusion {other:?}"),
    }
}

/// The scalar cohort: (cell, [inputs], direct ops, direct result
/// register). The direct ops ARE the handwritten nucleus arms; the cell
/// path is compiled registry data. Parity is the migration contract.
#[test]
fn scalar_cohort_dual_path_bit_exact() {
    for (cell, inputs, mut ops, result) in [
        ("std.math.add", [2.0, 3.0], const_regs(&[2.0, 3.0]), 2),
        ("std.math.mul", [1.5, 4.0], const_regs(&[1.5, 4.0]), 2),
    ] {
        ops.push(match cell {
            "std.math.add" => EmirOp::F64Add(EmirValue(0), EmirValue(1)),
            _ => EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
        });
        let consts: Vec<Value> = inputs.iter().map(|&v| Value::F64(v)).collect();
        let via_cell = seam_eval(cell, &consts);
        let direct = direct_eval(ops, &consts, result);
        assert_parity(&format!("{cell}({:?})", inputs), &via_cell, &direct);
        // NaN propagates identically on both paths (no guard hides it).
        let nan_consts = vec![Value::F64(f64::NAN), Value::F64(1.0)];
        let nan_cell = seam_eval(cell, &nan_consts);
        let nan_direct = direct_eval(
            match cell {
                "std.math.add" => vec![
                    EmirOp::ConstF64(f64_bits(f64::NAN)),
                    EmirOp::ConstF64(f64_bits(1.0)),
                    EmirOp::F64Add(EmirValue(0), EmirValue(1)),
                ],
                _ => vec![
                    EmirOp::ConstF64(f64_bits(f64::NAN)),
                    EmirOp::ConstF64(f64_bits(1.0)),
                    EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
                ],
            },
            &nan_consts,
            2,
        );
        assert_parity(&format!("{cell}(NaN, 1)"), &nan_cell, &nan_direct);
    }

    // Unary builtins: sin, exp, sqrt (sqrt over the declared non-negative
    // domain; the strict policy propagates NaN identically on both paths
    // for out-of-domain inputs).
    for (cell, builtin, fixtures) in [
        (
            "std.math.sin",
            BuiltinId::Sin,
            [0.0, 0.7, -3.14159265358979],
        ),
        ("std.math.exp", BuiltinId::Exp, [0.0, 1.5, -742.0]),
        ("std.math.sqrt", BuiltinId::Sqrt, [0.0, 4.0, 1e300]),
    ] {
        for x in fixtures {
            let ops = vec![
                EmirOp::ConstF64(f64_bits(x)),
                EmirOp::UnaryBuiltin(builtin, EmirValue(0)),
            ];
            let consts = vec![Value::F64(x)];
            let via_cell = seam_eval(cell, &consts);
            let direct = direct_eval(ops, &consts, 1);
            assert_parity(&format!("{cell}({x})"), &via_cell, &direct);
        }
        // Out-of-domain sqrt: NaN on BOTH paths (silent NaN propagation
        // is the declared strict-f64 behavior for unguarded scalars —
        // the policy lives in the cell's guards, and scalar cohort cells
        // declare none).
        if matches!(builtin, BuiltinId::Sqrt) {
            let consts = vec![Value::F64(-1.0)];
            let ops = vec![
                EmirOp::ConstF64(f64_bits(-1.0)),
                EmirOp::UnaryBuiltin(BuiltinId::Sqrt, EmirValue(0)),
            ];
            let via_cell = seam_eval(cell, &consts);
            let direct = direct_eval(ops, &consts, 1);
            assert_parity(&format!("{cell}(-1)"), &via_cell, &direct);
            match (&via_cell, &direct) {
                (Value::F64(a), Value::F64(b)) => {
                    assert!(a.is_nan() && b.is_nan(), "sqrt(-1) is NaN on both paths")
                }
                other => panic!("scalar outputs expected, got {other:?}"),
            }
        }
    }

    // Comparison: the comparison vocabulary lowers to the generic
    // comparison ops; bit-parity includes the signed-zero case.
    for (a, b) in [(2.0, 3.0), (3.0, 3.0), (-0.0, 0.0)] {
        let ops = vec![
            EmirOp::ConstF64(f64_bits(a)),
            EmirOp::ConstF64(f64_bits(b)),
            EmirOp::Lt(EmirValue(0), EmirValue(1)),
        ];
        let consts = vec![Value::F64(a), Value::F64(b)];
        let via_cell = seam_eval("std.math.lt", &consts);
        let direct = direct_eval(ops, &consts, 2);
        assert_parity(&format!("lt({a}, {b})"), &via_cell, &direct);
    }
}

/// The vector cohort: sum reduction. Left-to-right strict order on both
/// paths (the cell's compiled VectorReduce vs the handwritten fold), the
/// declared finite policy (AllFinite guard refuses typed).
#[test]
fn sum_reduction_dual_path_bit_exact() {
    for case in [
        vec![1.0, 2.0, 3.0],
        vec![1e-300, 1e300, 0.0],
        vec![-742.5, 741.5, 1.0],
    ] {
        let ops = vec![
            EmirOp::LoadInput(0),
            EmirOp::VectorReduce {
                reduce: ReduceId::Sum,
                source: EmirValue(0),
            },
        ];
        let mut direct = 0.0_f64;
        for &x in &case {
            direct = direct + x; // Iterator::sum order, no FMA
        }
        let via_cell = seam_eval("std.tensor.sum", &[Value::Vector(case.clone())]);
        let direct = Value::F64(direct);
        assert_parity(&format!("sum({case:?})"), &via_cell, &direct);
        let _ = ops;
    }

    // The declared numeric policy is cell DATA: the sum cell guards
    // AllFinite — a NaN element refuses typed at the seam (E-CELL-006),
    // exactly like softmax. The guard is load-bearing: without it the
    // reduce would silently return a poisoned sum.
    let policy_program = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: "std.tensor.sum".to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    match evaluate_with_budget(
        &policy_program,
        &[Value::Vector(vec![1.0, f64::NAN])],
        &[],
        EvalBudget::default(),
    ) {
        Err(emath_exec_ir::interp::EvalFault::CapabilityRefused { code, .. }) => {
            assert_eq!(code, "E-CELL-006");
        }
        other => panic!("NaN element must refuse E-CELL-006, got {other:?}"),
    }
}

#[test]
fn softmax_stays_in_cohort() {
    // Softmax remains the cohort's tensor anchor: registry cell vs the
    // emath-ir handwritten oracle, bit-for-bit ('s differential,
    // re-run through the cohort harness).
    for logits in [&[1.0, 2.0, 3.0] as &[f64], &[-5.0, 0.0, 5.0, 500.0], &[0.0]] {
        let via_cell = match seam_eval("std.tensor.softmax", &[Value::Vector(logits.to_vec())]) {
            Value::Vector(values) => values,
            other => panic!("expected vector, got {other:?}"),
        };
        let oracle = softmax_reference_strict_f64(logits).expect("oracle computes");
        for (i, (g, w)) in via_cell.iter().zip(oracle.iter()).enumerate() {
            assert_eq!(g.to_bits(), w.to_bits(), "softmax {logits:?} [{i}]");
        }
    }
}

#[test]
fn registry_is_data_and_dispatch_is_branch_free() {
    // Anti-LOC law: the cohort is DATA. The registry must contain every
    // REQUIRED migrated-cohort cell — the 7 migrated ops +
    // softmax + the four `std.linalg` cells, the five `std.graph`
    // cells, the two `std.optimize` cells, the two `std.poly` cells,
    // the three `std.control` cells, and the two `std.category` cells.
    // The registry is OPEN by
    // design (adding a cell is one data entry, never an op variant or
    // dispatch arm), so the contract is required-set CONTAINMENT, not a
    // frozen total: unrelated registry additions from later are
    // permitted and are not this assertion's concern. The seam gained
    // NO per-op branch for any of them (unknown cells still refuse
    // typed, the contract); no compiled op name carries a cell
    // path.
    const REQUIRED_MIGRATED_COHORT: &[&str] = &[
        "std.math.add",
        "std.math.mul",
        "std.math.sin",
        "std.math.exp",
        "std.math.sqrt",
        "std.math.lt",
        "std.tensor.sum",
        "std.tensor.softmax",
        "std.linalg.norm",
        "std.linalg.norm1",
        "std.linalg.norminf",
        "std.linalg.inner_product",
        "std.graph.reachability",
        "std.graph.bfs_order",
        "std.graph.shortest_distances",
        "std.graph.out_degrees",
        "std.graph.laplacian",
        "std.optimize.lp",
        "std.optimize.pareto_front",
        "std.poly.mul",
        "std.poly.eval",
        "std.control.transfer_eval",
        "std.control.dc_gain",
        "std.control.poles_stable",
        "std.category.check",
        "std.category.commutative",
    ];
    let registry = std_cell_registry();
    for name in REQUIRED_MIGRATED_COHORT {
        assert!(
            registry.contains_key(*name),
            "required migrated cohort cell {name} is missing from the registry: {:?}",
            registry.keys().collect::<Vec<_>>()
        );
    }
    let cell = registry.get("std.math.exp").expect("present");
    for (op, _) in &cell.program.ops {
        let name = op.name();
        assert!(
            !name.contains("std.math"),
            "bytecode is generic vocabulary, not per-op naming: {name}"
        );
    }

    // Contracts are data: the sum cell declares the finite-policy guard;
    // scalar cohort cells declare the unguarded-scalar policy (NaN
    // propagates — pinned in the scalar test).
    let sum = registry.get("std.tensor.sum").expect("registered");
    assert!(matches!(sum.guards[0], ArgGuard::AllFinite(0)));
    let add = registry.get("std.math.add").expect("registered");
    assert!(add.guards.is_empty());
    assert_eq!(add.params.len(), 2);
    assert_eq!(sum.params, vec![("x".to_string(), ParamShape::Vector)]);
}

#[test]
fn missing_nucleus_diagnosed_typed() {
    // Matmul and RK4 are NOT in the closed reference vocabulary: the
    // compiler refuses typed, naming the missing nucleus (matrix carrier
    // shapes; integrator loops in first-order terms). The law:
    // failures diagnose the missing nucleus — never a silent wrong
    // lowering.
    let x = || Term::Variable(VariableId("x".into()));
    let y = || Term::Variable(VariableId("y".into()));
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("matmul".into()), 2)
        .expect("conflict-free");
    signature
        .insert(SymbolId("rk4".into()), 2)
        .expect("conflict-free");
    let params = vec![
        ("x".to_string(), ParamShape::Vector),
        ("y".to_string(), ParamShape::Vector),
    ];

    match compile_reference(
        &Term::Apply {
            operator: SymbolId("matmul".into()),
            arguments: vec![x(), y()],
        },
        &signature,
        &params,
        Vec::new(),
        "test.matmul",
    ) {
        Err(TermCompileError::UnknownOperator { symbol }) => assert_eq!(symbol, "matmul"),
        other => panic!("matmul must diagnose the missing nucleus, got {other:?}"),
    }
    match compile_reference(
        &Term::Apply {
            operator: SymbolId("rk4".into()),
            arguments: vec![x(), y()],
        },
        &signature,
        &params,
        Vec::new(),
        "test.rk4",
    ) {
        Err(TermCompileError::UnknownOperator { symbol }) => assert_eq!(symbol, "rk4"),
        other => panic!("rk4 must diagnose the missing nucleus, got {other:?}"),
    }
}

#[test]
fn frozen_policy_mutation_refused() {
    // The negative seed's silent-success scenario: an identity-affecting
    // numeric-policy change to a frozen cohort cell refuses typed
    // (E-CELL-003) — the registry never serves a mutated cell silently,
    // and rollback stays independent per op (entries are standalone).
    let frozen = CellSchema {
        name: QualifiedName::single("std.math.add"),
        class: SchemaClass::Pure,
        version: "1.0.0".to_string(),
        migration: MigrationPolicy::Frozen,
        arity: 2,
        about: None,
    };
    let mutated = CellSchema {
        version: "2.0.0".to_string(),
        ..frozen.clone()
    };
    match admit_cell_mutation(&frozen, &mutated) {
        Err(refusal) => {
            assert_eq!(refusal.code(), "E-CELL-003");
            assert_eq!(refusal.cell_name(), "std.math.add");
        }
        Ok(_) => panic!("frozen cell mutation must refuse"),
    }

    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/capability_cell_migration.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-CELL-003"),
        "seed expects the frozen-policy refusal, found: {expect_line}"
    );
}

#[test]
fn cohort_mutant_is_caught_and_lands_in_bundle() {
    // Mutation law: flip the compiled add cell into a subtractor and the
    // dual-path differential MUST catch it bit-level (2+3 != 2-3). Then
    // the healthy cohort verdict lands as a labeled world record.
    let registry = std_cell_registry();
    let add = registry.get("std.math.add").expect("registered");
    let mutant_ops: Vec<(EmirOp, Span)> = add
        .program
        .ops
        .clone()
        .into_iter()
        .map(|(op, span)| match op {
            EmirOp::F64Add(a, b) => (EmirOp::F64Sub(a, b), span),
            other => (other, span),
        })
        .collect();
    assert_ne!(mutant_ops, add.program.ops, "the seed mutates the cell");
    let mutant_program = EmirProgram {
        ops: mutant_ops,
        result: add.program.result,
        input_count: add.program.input_count,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let inputs = [Value::F64(2.0), Value::F64(3.0)];
    let via_cell = seam_eval("std.math.add", &inputs);
    let via_mutant = evaluate_with_budget(&mutant_program, &inputs, &[], EvalBudget::default())
        .expect("mutant evaluates");
    match (&via_cell, &via_mutant) {
        (Value::F64(a), Value::F64(b)) => {
            assert_ne!(
                a.to_bits(),
                b.to_bits(),
                "the differential catches 2+3 vs 2-3"
            )
        }
        other => panic!("scalar outputs expected, got {other:?}"),
    }

    // Labeled portfolio: the healthy cohort verdict in the
    // envelope — every migrated op answers through the seam, labeled.
    struct CohortWorld;
    impl emath_genesis::FirstOrderWorld for CohortWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let a = seam_eval("std.math.add", &[Value::F64(2.0), Value::F64(3.0)]);
            let s = seam_eval("std.tensor.sum", &[Value::Vector(vec![1.0, 2.0, 3.0])]);
            match (a, s) {
                (Value::F64(sum), Value::F64(total)) if sum == 5.0 && total == 6.0 => {
                    Ok("cohort-parity-ok".to_string())
                }
                other => panic!("cohort parity broken: {other:?}"),
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
                "cohort-parity",
                &["dual-path-bit-parity", "registry-as-data"],
            )
        }
    }

    let term = Term::Constant(SymbolId("cohort[ten-op]".into()));
    let environment = BTreeMap::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &CohortWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "cohort-parity");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
