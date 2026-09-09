//!: Static specializer with VM parity.
//!
//! The law: a fixed genome + program specializes into static EMIR
//! (constants substituted, generic dispatch dropped, existing bit-exact
//! folding reused — no per-op backend duplication), and the specialized
//! path must agree with the generic VM seam BIT-FOR-BIT under the
//! declared numeric policy — including the typed refusals. The specializer
//! is a partial evaluator over `CompiledCell` data; it refuses typed
//! (unknown param, non-finite constant, vector-shaped binding, guard on a
//! bound param) and never mints per-op Rust.

use std::collections::BTreeMap;

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};

#[derive(Debug)]
enum SpecializeError {
    UnknownParam { name: String },
    NonFiniteConstant { name: String },
    UnsupportedShape { name: String, shape: String },
    GuardOnConstantParam { index: usize },
}

#[derive(Debug)]
struct SpecializedCell {
    capability: String,
    residual_params: Vec<(String, ParamShape)>,
    guards: Vec<ArgGuard>,
    program: EmirProgram,
}

impl SpecializedCell {
    fn evaluate(&self, _inputs: &[Value]) -> Result<Value, EvalFault> {
        Err(EvalFault::MissingInput(0))
    }
}

fn specialize_cell(
    _cell: &emath_exec_ir::term_compile::CompiledCell,
    _bindings: &std::collections::BTreeMap<String, Value>,
) -> Result<SpecializedCell, SpecializeError> {
    Err(SpecializeError::UnknownParam {
        name: "specialize-removed".to_string(),
    })
}

use emath_exec_ir::term_compile::{
    ArgGuard, CompiledCell, ParamShape,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_genesis::{
    Disposition, EvalError, FirstOrderWorld, ResultBundle, WorldBudget, evaluate_labeled,
};
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

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// The generic VM seam for the registry softmax cell (the same shape the
/// seam executes): ApplyCapability dispatch, guards at the seam.
fn run_softmax_seam(vector: &[f64]) -> Result<Value, EvalFault> {
    let program = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: STD_TENSOR_SOFTMAX.to_string(),
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
    evaluate_with_budget(
        &program,
        &[Value::Vector(vector.to_vec())],
        &[],
        EvalBudget::default(),
    )
}

/// Fixture cell with a vector argument AND a scalar argument:
/// `gain * recip(1 + exp(-x))` — a gain-scaled logistic. Declared with
/// the standard guards over the vector argument. Adding this fixture
/// touches no parser/sema/backend code: it is one `compile_reference`
/// call on the closed vocabulary.
fn compile_gain_cell() -> CompiledCell {
    let x = || Term::Variable(VariableId("x".into()));
    let gain = || Term::Variable(VariableId("gain".into()));
    let one = || Term::Constant(SymbolId("1.0".into()));
    let inner = || Term::Apply {
        operator: SymbolId("add".into()),
        arguments: vec![
            Term::Apply {
                operator: SymbolId("exp".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("neg".into()),
                    arguments: vec![x()],
                }],
            },
            one(),
        ],
    };
    let term = Term::Apply {
        operator: SymbolId("mul".into()),
        arguments: vec![
            Term::Apply {
                operator: SymbolId("recip".into()),
                arguments: vec![inner()],
            },
            gain(),
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("recip", 1usize),
        ("add", 2),
        ("exp", 1),
        ("neg", 1),
        ("mul", 2),
        ("1.0", 0),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("gain-cell signature is conflict-free");
    }
    compile_reference(
        &term,
        &signature,
        &[
            ("x".to_string(), ParamShape::Vector),
            ("gain".to_string(), ParamShape::Scalar),
        ],
        vec![ArgGuard::NonEmpty(0), ArgGuard::AllFinite(0)],
        "test.gain-sigmoid",
    )
    .expect("gain cell compiles")
}

/// Generic path for the gain cell: declared guards run first (the seam
/// contract), then the compiled body under the default budget.
fn run_gain_generic(cell: &CompiledCell, xs: &[f64], gain: f64) -> Result<Value, EvalFault> {
    let args = [Value::Vector(xs.to_vec()), Value::F64(gain)];
    for guard in &cell.guards {
        let index = match guard {
            ArgGuard::NonEmpty(index) | ArgGuard::AllFinite(index) => *index,
        };
        let Value::Vector(elements) = &args[index] else {
            panic!("fixture guard targets a vector argument");
        };
        let violated = match guard {
            ArgGuard::NonEmpty(_) => elements.is_empty(),
            ArgGuard::AllFinite(_) => elements.iter().any(|x| !x.is_finite()),
        };
        if violated {
            return Err(EvalFault::CapabilityRefused {
                capability: cell.capability.clone(),
                code: "E-CELL-006".to_string(),
            });
        }
    }
    evaluate_with_budget(&cell.program, &args, &[], EvalBudget::default())
}

fn expect_vector(value: &Value) -> &[f64] {
    match value {
        Value::Vector(values) => values,
        other => panic!("expected vector, got {other:?}"),
    }
}

fn assert_bit_exact(p: &mut Probe, label: &str, got: &[f64], want: &[f64]) {
    p.eq("got.len()", &(got.len()), &(want.len()));
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        p.eq("g.to_bits()", &(g.to_bits()), &(w.to_bits()));
    }
}

#[test]
fn intent() {
    let mut p = Probe::new(": Static specializer with VM parity.");
    p.case("identity_specialization_matches_vm_seam", |p| {

    // Parity capstone, registry cell through the REAL seam: specializing
    // with no bindings is the identity partial evaluation, and its
    // execution must agree with the generic VM bit-for-bit — values AND
    // typed refusals (guards survive renumbering).
    let registry = std_cell_registry();
    let cell = registry
        .get(STD_TENSOR_SOFTMAX)
        .expect("std cell present");
    let specialized = specialize_cell(cell, &BTreeMap::new()).expect("identity specialization");
    p.eq("identity_specialization_matches_vm_seam#1", specialized.capability.as_str(), STD_TENSOR_SOFTMAX);
    p.eq("identity_specialization_matches_vm_seam#2", specialized.residual_params.len(), 1);
    p.eq("guards survive", specialized.guards.len(), 2);

    let fixtures: [&[f64]; 4] = [
        &[1.0, 2.0, 3.0],
        &[-5.0, 0.0, 5.0, 500.0],
        &[1e-300, 1e-300, 1e300],
        &[0.0],
    ];
    for logits in fixtures {
        let generic = run_softmax_seam(logits).expect("seam evaluates");
        let specialized_run = specialized
            .evaluate(&[Value::Vector(logits.to_vec())])
            .expect("specialized evaluates");
        assert_bit_exact(p, 
            "identity parity",
            expect_vector(&specialized_run),
            expect_vector(&generic),
        );
    }

    // Refusal parity: the specialized path refuses exactly like the seam.
    for bad in [&[] as &[f64], &[1.0, f64::NAN]] {
        match (
            run_softmax_seam(bad),
            specialized.evaluate(&[Value::Vector(bad.to_vec())]),
        ) {
            (
                Err(EvalFault::CapabilityRefused { code: a, .. }),
                Err(EvalFault::CapabilityRefused { code: b, .. }),
            ) => {
                p.demand("identity_specialization_matches_vm_seam#4", a == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", a));
                p.demand("identity_specialization_matches_vm_seam#5", b == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", b));
            }
            other => { p.fail("identity_specialization_matches_vm_seam#6", format!("refusal parity broken for {bad:?}: {other:?}")); return; },
        }
    }

    });
    p.case("scalar_binding_specializes_with_parity", |p| {

    // Partial evaluation: the scalar `gain` is bound to a constant; the
    // vector argument stays residual. The specialized program has one
    // input (not two), keeps the guards (renumbered onto the residual
    // argument), and agrees with the generic VM bit-for-bit.
    let cell = compile_gain_cell();
    let mut bindings = BTreeMap::new();
    bindings.insert("gain".to_string(), Value::F64(2.0));
    let specialized = specialize_cell(&cell, &bindings).expect("specializes");

    p.demand("scalar_binding_specializes_with_parity#1", specialized.capability == "test.gain-sigmoid", format!("expected {:?}, got {:?}", "test.gain-sigmoid", specialized.capability));
    p.eq("the bound scalar is dropped from the residual contract", specialized.residual_params.clone(), vec![("x".to_string(), ParamShape::Vector)]);
    p.eq("scalar_binding_specializes_with_parity#3", specialized.program.input_count, 1);
    p.eq("scalar_binding_specializes_with_parity#4", specialized.guards.len(), 2);
    p.demand("scalar_binding_specializes_with_parity#5", matches!(specialized.guards[0], ArgGuard::NonEmpty(0)), "scalar_binding_specializes_with_parity#5: matches!(specialized.guards[0], ArgGuard::NonEmpty(0))");
    p.demand("scalar_binding_specializes_with_parity#6", matches!(specialized.guards[1], ArgGuard::AllFinite(0)), "scalar_binding_specializes_with_parity#6: matches!(specialized.guards[1], ArgGuard::AllFinite(0))");

    let fixtures: [&[f64]; 4] = [
        &[0.0, 1.0, 2.0],
        &[-3.0, 0.0, 3.0],
        &[1e-8, 1.0, 1e8],
        &[0.0],
    ];
    for xs in fixtures {
        let generic = run_gain_generic(&cell, xs, 2.0).expect("generic evaluates");
        let specialized_run = specialized
            .evaluate(&[Value::Vector(xs.to_vec())])
            .expect("specialized evaluates");
        assert_bit_exact(p, 
            "binding parity",
            expect_vector(&specialized_run),
            expect_vector(&generic),
        );
        // The specialized value IS the declared gain-scaled logistic.
        for (x, y) in xs.iter().zip(expect_vector(&specialized_run)) {
            let want = 2.0 / (1.0 + (-x).exp());
            p.demand(format!("specialized value at {x}: {y} != {want}"), (y - want).abs() < 1e-12, format!("specialized value at {x}: {y} != {want}"));
        }
    }

    });
    p.case("full_binding_folds_to_static_constant", |p| {

    // Fixed genome: every parameter bound. The residual is STATIC EMIR —
    // the existing bit-exact folding pass collapses the whole body to a
    // single constant; the program needs zero inputs.
    let term = Term::Apply {
        operator: SymbolId("add".into()),
        arguments: vec![
            Term::Apply {
                operator: SymbolId("mul".into()),
                arguments: vec![
                    Term::Variable(VariableId("k".into())),
                    Term::Constant(SymbolId("3.0".into())),
                ],
            },
            Term::Constant(SymbolId("4.0".into())),
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [("add", 2usize), ("mul", 2), ("3.0", 0), ("4.0", 0)] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("conflict-free");
    }
    let cell = compile_reference(
        &term,
        &signature,
        &[("k".to_string(), ParamShape::Scalar)],
        Vec::new(),
        "test.affine",
    )
    .expect("affine cell compiles");

    let mut bindings = BTreeMap::new();
    bindings.insert("k".to_string(), Value::F64(2.0));
    let specialized = specialize_cell(&cell, &bindings).expect("specializes");

    p.eq("full_binding_folds_to_static_constant#1", specialized.residual_params.len(), 0);
    p.eq("full_binding_folds_to_static_constant#2", specialized.program.input_count, 0);
    p.eq("fully folded", specialized.program.ops.len(), 1);
    p.demand(format!("2*3+4 folds to the static constant 10.0: {:?}", specialized.program.ops[0].0), matches!(specialized.program.ops[0].0, EmirOp::ConstF64(bits) if bits == f64_bits(10.0)), format!("2*3+4 folds to the static constant 10.0: {:?}", specialized.program.ops[0].0));
    let answer = specialized.evaluate(&[]).expect("static answer");
    match answer {
        Value::F64(v) => { p.eq("full_binding_folds_to_static_constant#5", v.to_bits(), f64_bits(10.0)); },
        other => { p.fail("full_binding_folds_to_static_constant#6", format!("expected scalar, got {other:?}")); return; },
    }

    });
    p.case("seeded_backend_mutant_is_caught", |p| {

    // Mutation law: seed a backend mutant into the specialized residual
    // (flip the gain constant 2.0 -> 3.0) and prove the parity
    // differential DETECTS it. A specializer whose output the parity
    // test cannot distinguish from the VM tests nothing.
    let cell = compile_gain_cell();
    let mut bindings = BTreeMap::new();
    bindings.insert("gain".to_string(), Value::F64(2.0));
    let specialized = specialize_cell(&cell, &bindings).expect("specializes");

    let mutant_ops: Vec<(EmirOp, Span)> = specialized
        .program
        .ops
        .clone()
        .into_iter()
        .map(|(op, span)| {
            if matches!(op, EmirOp::ConstF64(bits) if bits == f64_bits(2.0)) {
                (EmirOp::ConstF64(f64_bits(3.0)), span)
            } else {
                (op, span)
            }
        })
        .collect();
    p.ne("the seed must actually mutate the residual", mutant_ops.clone(), specialized.program.ops.clone());
    let mutant = EmirProgram {
        ops: mutant_ops,
        result: specialized.program.result,
        input_count: specialized.program.input_count,
        state_count: 0,
        domain_obligations: Vec::new(),
    };

    let xs = [0.0_f64, 1.0, 2.0];
    let generic = run_gain_generic(&cell, &xs, 2.0).expect("generic evaluates");
    let mutant_run = evaluate_with_budget(
        &mutant,
        &[Value::Vector(xs.to_vec())],
        &[],
        EvalBudget::default(),
    )
    .expect("mutant evaluates");
    let got = expect_vector(&mutant_run);
    let want = expect_vector(&generic);
    let differs = got
        .iter()
        .zip(want.iter())
        .any(|(g, w)| g.to_bits() != w.to_bits());
    p.demand("the parity differential catches the backend mutant", differs, "the parity differential catches the backend mutant");

    });
    p.case("refusals_are_typed", |p| {

    let cell = compile_gain_cell();

    // Unknown param: outside the declared contract — the negative seed's
    // silent-success scenario. Typed, never a silent specialization.
    let mut unknown = BTreeMap::new();
    unknown.insert("y".to_string(), Value::F64(1.0));
    match specialize_cell(&cell, &unknown) {
        Err(SpecializeError::UnknownParam { name }) => { p.demand("refusals_are_typed#1", name == "y", format!("expected {:?}, got {:?}", "y", name)); },
        other => { p.fail("refusals_are_typed#2", format!("expected UnknownParam, got {other:?}")); return; },
    }

    // Non-finite constant: the strict-f64 policy, at the specialization
    // seam too.
    let mut nan = BTreeMap::new();
    nan.insert("gain".to_string(), Value::F64(f64::NAN));
    p.demand("refusals_are_typed#3", matches!(
        specialize_cell(&cell, &nan),
        Err(SpecializeError::NonFiniteConstant { .. })
    ), "refusals_are_typed#3: matches!(\n        specialize_cell(&cell, &nan),\n        Err(SpecializeError::NonFiniteConstant { .. ");

    // Vector-shaped binding: vectors are residual inputs, not partial-
    // evaluation constants in the closed vocabulary. Typed refusal.
    let mut vector_binding = BTreeMap::new();
    vector_binding.insert("x".to_string(), Value::F64(1.0));
    match specialize_cell(&cell, &vector_binding) {
        Err(SpecializeError::UnsupportedShape { name, shape }) => {
            p.demand("refusals_are_typed#4", name == "x", format!("expected {:?}, got {:?}", "x", name));
            p.demand("refusals_are_typed#5", shape == "vector", format!("expected {:?}, got {:?}", "vector", shape));
        }
        other => { p.fail("refusals_are_typed#6", format!("expected UnsupportedShape, got {other:?}")); return; },
    }

    // A guard pointing at a param the specializer is about to bind: the
    // guard could no longer run at the seam, so specialization refuses
    // instead of silently dropping a declared obligation.
    let guarded = compile_reference(
        &Term::Variable(VariableId("gain".into())),
        &Signature::default(),
        &[
            ("x".to_string(), ParamShape::Vector),
            ("gain".to_string(), ParamShape::Scalar),
        ],
        vec![ArgGuard::NonEmpty(1)],
        "test.guard-on-scalar",
    )
    .expect("guarded cell compiles");
    let mut bind_gain = BTreeMap::new();
    bind_gain.insert("gain".to_string(), Value::F64(2.0));
    p.demand("refusals_are_typed#7", matches!(
        specialize_cell(&guarded, &bind_gain),
        Err(SpecializeError::GuardOnConstantParam { index: 1 })
    ), "refusals_are_typed#7: matches!(\n        specialize_cell(&guarded, &bind_gain),\n        Err(SpecializeError::GuardOnConstan");

    });
    p.case("specialized_answer_lands_in_bundle", |p| {

    // WorldResultBundle fixture: the specialized run's
    // answer is a labeled world record in the envelope.
    struct ParityWorld;
    impl FirstOrderWorld for ParityWorld {
        type Value = f64;
        type Error = EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let cell = compile_gain_cell();
            let mut bindings = BTreeMap::new();
            bindings.insert("gain".to_string(), Value::F64(2.0));
            let specialized = specialize_cell(&cell, &bindings).expect("specializes");
            let run = specialized
                .evaluate(&[Value::Vector(vec![0.0, 1.0, 2.0])])
                .expect("specialized evaluates");
            match run {
                Value::Vector(values) => Ok(values[0]),
                other => panic!("expected vector, got {other:?}"),
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed("specializer-parity", &["vm-specializer-bit-parity"])
        }
    }

    let term = Term::Constant(SymbolId("gain-sigmoid[0.0,1.0,2.0]".into()));
    let environment = emath_genesis::Environment::<f64>::new();
    let result = evaluate_labeled(
        &term,
        &ParityWorld,
        &environment,
        WorldBudget { max_steps: 8 },
        |answer: &f64| format!("{answer:.6}"),
    );
    p.demand("specialized_answer_lands_in_bundle#2", matches!(result.disposition, Disposition::Answer { .. }), "specialized_answer_lands_in_bundle#2: matches!(result.disposition, Disposition::Answer { .. })");
    p.demand("specialized_answer_lands_in_bundle#3", result.world == "specializer-parity", format!("expected {:?}, got {:?}", "specializer-parity", result.world));
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("specialized_answer_lands_in_bundle#4", bundle.bundle_id.starts_with("fnv1a64:"), "specialized_answer_lands_in_bundle#4: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    // Negative seed: the seeded silent-success declares a typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/static_specialization.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects a typed specializer refusal, found: {expect_line}"), expect_line.contains("E-SPEC"), format!("seed expects a typed specializer refusal, found: {expect_line}"));

    });
    p.finish();
}











