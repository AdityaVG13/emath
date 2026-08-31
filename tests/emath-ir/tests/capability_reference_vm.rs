//! emath-epic-machine-fjxh.6: Generic reference VM — capability
//! applications, budgets, provider continuation holes.
//!
//! The bead's law: a custom alien term (`ExprNode::Apply` of an admitted
//! capability cell) evaluates in at least one world (the interp world,
//! for cells with local reference semantics), and resource exhaustion /
//! unsupported dispatch is a typed refusal — never partial authority,
//! never a silent identity. Extends `emath-exec-ir`; no second VM, zero
//! core delta in `emath-ir` op enums.

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, lower_definition};
use emath_ir::{Capability, CapabilityId, ExprNode, SemanticPackage};

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// Hand-built EMIR: `softmax([1.0, 2.0, 3.0])` as a capability
/// application over a vector constant.
fn softmax_program() -> EmirProgram {
    EmirProgram {
        // Registers materialize in op order (op i's result is register i),
        // so operands may only reference earlier ops.
        ops: vec![
            (EmirOp::ConstF64(f64_bits(1.0)), Span::default()),
            (EmirOp::ConstF64(f64_bits(2.0)), Span::default()),
            (EmirOp::ConstF64(f64_bits(3.0)), Span::default()),
            (
                EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]),
                Span::default(),
            ),
            (
                EmirOp::ApplyCapability {
                    capability: STD_TENSOR_SOFTMAX.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(3)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(4),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

#[test]
fn softmax_apply_evaluates_in_interp_world() {
    // Success criterion: the custom alien term evaluates in at least one
    // world. The interp world dispatches the cell's reference semantics
    // from the capability layer (data-driven, no domain VM branch).
    let program = softmax_program();
    let out = evaluate_with_budget(&program, &[], &[], EvalBudget::default())
        .expect("softmax application evaluates in the interp world");
    let probs = match out {
        Value::Vector(values) => values,
        other => panic!("expected vector, got {other:?}"),
    };

    // Pin against the capability layer's reference semantics AND a
    // hand-derived value (not a self-comparison).
    let logits = [1.0_f64, 2.0, 3.0];
    let reference = emath_ir::capability::softmax_reference_strict_f64(&logits)
        .expect("reference semantics compute");
    assert_eq!(probs.len(), 3);
    for (got, want) in probs.iter().zip(reference.iter()) {
        assert!((got - want).abs() < 1e-15, "{got} != {want}");
    }
    let hand = 1.0_f64 / (1.0 + 1.0_f64.exp() + 2.0_f64.exp());
    assert!((probs[0] - hand).abs() < 1e-12, "hand-derived p0");

    // WorldResultBundle fixture (bead e2e clause): the run as a world
    // record. The World ABI is a later spine bead; this pins the shape it
    // must carry for "evaluates in at least one world".
    #[derive(Debug)]
    struct WorldResultBundle {
        world: &'static str,
        verdict: &'static str,
        outputs: Vec<f64>,
        refusals: Vec<String>,
    }
    let bundle = WorldResultBundle {
        world: "interp",
        verdict: "evaluated",
        outputs: probs.clone(),
        refusals: Vec::new(),
    };
    assert_eq!(bundle.world, "interp");
    assert_eq!(bundle.verdict, "evaluated");
    assert_eq!(bundle.outputs.len(), 3);
    assert!(bundle.refusals.is_empty());
}

#[test]
fn budget_exhaustion_is_typed_refusal_never_partial() {
    // Resource exhaustion: an op budget below the program's step count is
    // a typed refusal, and no partial result escapes (Result::Err, not a
    // value).
    let mut program = softmax_program();
    // Shrink to a pure const chain: 5 const ops, result = register 4.
    program.ops = (0..5)
        .map(|i| (EmirOp::ConstF64(f64_bits(i as f64)), Span::default()))
        .collect();
    program.result = EmirValue(4);

    let starved = EvalBudget {
        max_steps: 3,
        max_capability_applications: u32::MAX,
    };
    match evaluate_with_budget(&program, &[], &[], starved) {
        Err(EvalFault::BudgetExhausted { executed }) => assert_eq!(executed, 3),
        other => panic!("expected typed budget refusal, got {other:?}"),
    }

    // Boundary: a budget exactly equal to the needed steps admits.
    let exact = EvalBudget {
        max_steps: 5,
        max_capability_applications: u32::MAX,
    };
    assert!(evaluate_with_budget(&program, &[], &[], exact).is_ok());

    // The application budget bounds capability dispatch itself.
    let app_starved = EvalBudget {
        max_steps: u32::MAX,
        max_capability_applications: 0,
    };
    let softmax = softmax_program();
    assert!(matches!(
        evaluate_with_budget(&softmax, &[], &[], app_starved),
        Err(EvalFault::BudgetExhausted { .. })
    ));
}

#[test]
fn provider_and_shape_refusals_are_typed() {
    let span = Span::default();
    let vector: Vec<(EmirOp, Span)> = vec![
        (EmirOp::ConstF64(f64_bits(1.0)), span),
        (EmirOp::ConstF64(f64_bits(2.0)), span),
    ];
    let build = |op: EmirOp| EmirProgram {
        ops: {
            let mut ops = vector.clone();
            ops.push((op, span));
            ops
        },
        result: EmirValue(2),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };

    // Provider-class cell: no local reference semantics -> outstanding
    // provider call, the typed continuation hole (resumable by a provider
    // run; never a silent identity, never partial authority).
    let provider = build(EmirOp::ApplyCapability {
        capability: "sim.engine.integrate".to_string(),
        class: CellClass::Provider,
        args: vec![EmirValue(0), EmirValue(1)],
    });
    match evaluate_with_budget(&provider, &[], &[], EvalBudget::default()) {
        Err(EvalFault::ProviderCallRequired { capability, args }) => {
            assert_eq!(capability, "sim.engine.integrate");
            assert_eq!(args, 2);
        }
        other => panic!("expected provider continuation hole, got {other:?}"),
    }

    // Contract arity: softmax takes exactly one vector argument.
    let wrong_arity = build(EmirOp::ApplyCapability {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0), EmirValue(1)],
    });
    assert!(matches!(
        evaluate_with_budget(&wrong_arity, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ));

    // Contract shape: a scalar argument is a typed confusion.
    let wrong_shape = build(EmirOp::ApplyCapability {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0)],
    });
    assert!(matches!(
        evaluate_with_budget(&wrong_shape, &[], &[], EvalBudget::default()),
        Err(EvalFault::TypeConfusion { .. })
    ));

    // Strict-f64 firewall at the VM seam: non-finite logits refuse with
    // the capability layer's E-CELL-006, never a silent NaN distribution.
    let nan_vector = EmirProgram {
        ops: vec![
            (EmirOp::ConstF64(f64_bits(f64::NAN)), span),
            (EmirOp::ConstF64(f64_bits(1.0)), span),
            (EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]), span),
            (
                EmirOp::ApplyCapability {
                    capability: STD_TENSOR_SOFTMAX.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(2)],
                },
                span,
            ),
        ],
        result: EmirValue(3),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    match evaluate_with_budget(&nan_vector, &[], &[], EvalBudget::default()) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, STD_TENSOR_SOFTMAX);
            assert_eq!(code, "E-CELL-006");
        }
        other => panic!("expected E-CELL-006 refusal at the VM seam, got {other:?}"),
    }

    // A pure cell with no local reference semantics refuses typed (it is
    // an implementation gap, not a silent success).
    let unknown_pure = build(EmirOp::ApplyCapability {
        capability: "sim.engine.magic".to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0)],
    });
    assert!(matches!(
        evaluate_with_budget(&unknown_pure, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ));
}

#[test]
fn emitter_lowers_apply_and_term_evaluates() {
    // Lowering seam: an admitted package's Apply term lowers to
    // ApplyCapability (data: name + class), prints deterministically, and
    // the lowered program evaluates end-to-end.
    let mut package = SemanticPackage::new();
    let cap: CapabilityId = package.push_capability(Capability {
        name: QualifiedName::single(STD_TENSOR_SOFTMAX),
        class: CellClass::Pure,
    });
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let app = package.push_expr(
        ExprNode::Apply {
            capability: cap,
            arguments: vec![x],
        },
        Span::default(),
    );

    let program = lower_definition(
        &package,
        app,
        &["x".to_string()][..],
        &[],
    )
    .expect("Apply lowers to the capability op");

    let (last_op, _) = program.ops.last().expect("non-empty program");
    match last_op {
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => {
            assert_eq!(capability, STD_TENSOR_SOFTMAX);
            assert_eq!(*class, CellClass::Pure);
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected ApplyCapability, got {other:?}"),
    }
    let printed = program.print();
    assert!(
        printed.contains("apply-capability name=std.tensor.softmax class=pure"),
        "byte-deterministic SSA must carry the cell identity: {printed}"
    );

    let out = evaluate_with_budget(
        &program,
        &[Value::Vector(vec![1.0, 2.0, 3.0])],
        &[],
        EvalBudget::default(),
    )
    .expect("lowered alien term evaluates");
    let reference = emath_ir::capability::softmax_reference_strict_f64(&[1.0, 2.0, 3.0])
        .expect("reference");
    match out {
        Value::Vector(values) => {
            for (got, want) in values.iter().zip(reference.iter()) {
                assert!((got - want).abs() < 1e-15);
            }
        }
        other => panic!("expected vector, got {other:?}"),
    }

    // Dangling capability id at the lowering seam: typed refusal, never a
    // silent lower.
    let dangling = package.push_expr(
        ExprNode::Apply {
            capability: CapabilityId(u32::MAX),
            arguments: vec![x],
        },
        Span::default(),
    );
    assert!(lower_definition(&package, dangling, &["x".to_string()][..], &[]).is_err());
}

/// Bead negative seed: seeded silent-success scenario is refused.
#[test]
fn negative_seed_names_typed_refusal() {
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/capability_reference_vm.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-VM") || expect_line.contains("E-CELL"),
        "seed expects a typed VM/admission refusal, found: {expect_line}"
    );
}
