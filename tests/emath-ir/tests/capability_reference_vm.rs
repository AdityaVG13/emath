//! Generic reference VM — capability
//! applications, budgets, provider continuation holes.
//!
//! The law: a custom alien term (`ExprNode::Apply` of an admitted
//! capability cell) evaluates in at least one world (the interp world,
//! for cells with local reference semantics), and resource exhaustion /
//! unsupported dispatch is a typed refusal — never partial authority,
//! never a silent identity. Extends `emath-exec-ir`; no second VM, zero
//! core delta in `emath-ir` op enums.
//!
//! Reference cells enter ONLY through the verified Language Image
//! installs: `install_language_distribution` (native bindings + reference
//! programs) and `install_reference_programs` (reference-only; clears
//! native state). Every test loads the real checked-in distribution —
//! no handcrafted cells, no injection API. Native/reference parity is a
//! later slice (.5); here the exact-add capsule pins the seam.

use std::path::{Path, PathBuf};

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::{LanguageDistribution, load_language_distribution};
use emath_exec_ir::native_kernel::{
    binding_semantic_hash, install_language_distribution, install_reference_programs, native_kernel,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, lower_definition};
use emath_ir::{Capability, CapabilityId, ExprNode, SemanticPackage};

const ADD: &str = "std.capability.math.add";

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// The real checked-in Language Image (the only authority source the
/// tests install from).
fn real_distribution() -> LanguageDistribution {
    load_language_distribution(&language_root()).expect("checked-in language distribution loads")
}

/// `capability(left, right)` over two const operands (registers 0, 1).
fn binary_apply_program(capability: &str, left: EmirOp, right: EmirOp) -> EmirProgram {
    EmirProgram {
        ops: vec![
            (left, Span::default()),
            (right, Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: capability.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0), EmirValue(1)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(2),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn i64_add_result(out: Result<Value, EvalFault>, want: i64) {
    match out {
        Ok(Value::I64(sum)) => assert_eq!(sum, want),
        other => panic!("expected exact Int sum {want}, got {other:?}"),
    }
}

#[test]
fn exact_add_fallback_executes_from_real_image_reference() {
    // Reference-only install: the native binding state is cleared, so the
    // seam MUST execute the authored exact-add reference body from the
    // checked-in image (capsule semantics: Int,Int -> Int, exact).
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");
    assert!(
        native_kernel(ADD).is_none(),
        "reference-only install leaves no native binding state"
    );

    let out = evaluate_with_budget(
        &binary_apply_program(ADD, EmirOp::ConstI64(2), EmirOp::ConstI64(1)),
        &[],
        &[],
        EvalBudget::default(),
    );
    i64_add_result(out, 3);
}

#[test]
fn exact_add_native_binding_wins_with_capsule_hash() {
    // Full install: the checked-add native binding is present, its
    // semantic hash matches the capsule of record, and exact add answers
    // through it.
    let distribution = real_distribution();
    install_language_distribution(&distribution).expect("full install");
    let capsule_hash = distribution
        .capsules
        .iter()
        .find(|capsule| capsule.feature_id.to_string() == ADD)
        .expect("checked-in image carries the exact-add capsule")
        .semantic_hash
        .as_str()
        .to_string();
    assert_eq!(
        binding_semantic_hash(ADD).as_deref(),
        Some(capsule_hash.as_str()),
        "installed binding hash matches the capsule"
    );
    assert!(native_kernel(ADD).is_some());

    let out = evaluate_with_budget(
        &binary_apply_program(ADD, EmirOp::ConstI64(2), EmirOp::ConstI64(1)),
        &[],
        &[],
        EvalBudget::default(),
    );
    i64_add_result(out, 3);
}

#[test]
fn exact_add_overflow_refuses_typed() {
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");
    let out = evaluate_with_budget(
        &binary_apply_program(ADD, EmirOp::ConstI64(i64::MAX), EmirOp::ConstI64(1)),
        &[],
        &[],
        EvalBudget::default(),
    );
    match out {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "f64-add");
            assert!(detail.contains("overflow"), "overflow named: {detail}");
        }
        other => panic!("expected typed overflow refusal, got {other:?}"),
    }
}

#[test]
fn exact_add_type_mismatch_refuses_typed() {
    // The capsule declares Int,Int: a mixed-carrier application refuses
    // typed — never a silent coercion.
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");
    let out = evaluate_with_budget(
        &binary_apply_program(ADD, EmirOp::ConstI64(2), EmirOp::ConstF64(f64_bits(0.5))),
        &[],
        &[],
        EvalBudget::default(),
    );
    assert!(
        matches!(out, Err(EvalFault::TypeConfusion { .. })),
        "mixed Int/Float64 operands refuse typed, got {out:?}"
    );
}

#[test]
fn reference_arity_refuses_typed() {
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");
    // One operand against the two-param cell: typed refusal, never a
    // partial application.
    let program = EmirProgram {
        ops: vec![
            (EmirOp::ConstI64(2), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: ADD.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let out = evaluate_with_budget(&program, &[], &[], EvalBudget::default());
    assert!(
        matches!(out, Err(EvalFault::Arithmetic { .. })),
        "wrong arity refuses typed, got {out:?}"
    );
}

#[test]
fn reference_no_body_no_kernel_refuses_typed() {
    // No native binding and no installed reference body for this
    // capability: the typed refusal is the ONLY outcome.
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");
    let out = evaluate_with_budget(
        &binary_apply_program(
            "std.capability.math.never-authored",
            EmirOp::ConstI64(2),
            EmirOp::ConstI64(1),
        ),
        &[],
        &[],
        EvalBudget::default(),
    );
    match out {
        Err(EvalFault::Arithmetic { detail, .. }) => {
            assert!(detail.contains("no installed reference bytecode or native kernel"));
        }
        other => panic!("expected typed no-body refusal, got {other:?}"),
    }
}

#[test]
fn budget_exhaustion_is_typed_refusal_never_partial() {
    // Resource exhaustion: an op budget below the program's step count is
    // a typed refusal, and no partial result escapes (Result::Err, not a
    // value). No capability dispatch is needed to pin the budget.
    install_language_distribution(&real_distribution()).expect("full install");
    let span = Span::default();
    let chain = EmirProgram {
        ops: (0..5)
            .map(|i| (EmirOp::ConstF64(f64_bits(i as f64)), span))
            .collect(),
        result: EmirValue(4),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };

    let starved = EvalBudget {
        max_steps: 3,
        max_capability_applications: u32::MAX,
    };
    match evaluate_with_budget(&chain, &[], &[], starved) {
        Err(EvalFault::BudgetExhausted { executed }) => assert_eq!(executed, 3),
        other => panic!("expected typed budget refusal, got {other:?}"),
    }

    // Boundary: a budget exactly equal to the needed steps admits.
    let exact = EvalBudget {
        max_steps: 5,
        max_capability_applications: u32::MAX,
    };
    assert!(evaluate_with_budget(&chain, &[], &[], exact).is_ok());

    // The application budget bounds capability dispatch itself.
    let app_starved = EvalBudget {
        max_steps: u32::MAX,
        max_capability_applications: 0,
    };
    let dispatch = binary_apply_program(ADD, EmirOp::ConstI64(2), EmirOp::ConstI64(1));
    assert!(matches!(
        evaluate_with_budget(&dispatch, &[], &[], app_starved),
        Err(EvalFault::BudgetExhausted { .. })
    ));
}

#[test]
fn provider_and_unknown_pure_refusals_are_typed() {
    install_language_distribution(&real_distribution()).expect("full install");
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

    // A pure capability with no native kernel and no installed reference
    // body refuses typed (an implementation gap, never a silent success).
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
    // the lowered program evaluates end-to-end through the installed
    // authored reference cell (reference-only install = fallback world).
    let distribution = real_distribution();
    install_reference_programs(&distribution).expect("reference-only install");

    let mut package = SemanticPackage::new();
    let cap: CapabilityId = package.push_capability(Capability {
        name: QualifiedName::single(ADD),
        class: CellClass::Pure,
    });
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Variable(QualifiedName::single("y")),
        Span::default(),
    );
    let app = package.push_expr(
        ExprNode::Apply {
            capability: cap,
            arguments: vec![x, y],
        },
        Span::default(),
    );

    let program = lower_definition(&package, app, &["x".to_string(), "y".to_string()][..], &[])
        .expect("Apply lowers to the capability op");

    let (last_op, _) = program.ops.last().expect("non-empty program");
    match last_op {
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => {
            assert_eq!(capability, ADD);
            assert_eq!(*class, CellClass::Pure);
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected ApplyCapability, got {other:?}"),
    }
    let printed = program.print();
    assert!(
        printed.contains("apply-capability") && printed.contains(ADD),
        "byte-deterministic SSA must carry the cell identity: {printed}"
    );

    let out = evaluate_with_budget(
        &program,
        &[Value::I64(2), Value::I64(1)],
        &[],
        EvalBudget::default(),
    );
    i64_add_result(out, 3);

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

/// Negative seed: seeded silent-success scenario is refused.
#[test]
fn negative_seed_names_typed_refusal() {
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/capability_reference_vm.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-VM") || expect_line.contains("E-CELL"),
        "seed expects a typed VM/admission refusal, found: {expect_line}"
    );
}

/// The universal program-as-value carrier: two distinct program literals
/// stay distinct values, the literal survives evaluation into
/// `Value::Program` with its body intact in the canonical dump, and the
/// carrier rides an `ApplyCapability` argument register as an ordinary
/// value (the unknown-name refusal fires only after the register is
/// read — the artifact never triggers domain interpretation).
#[test]
fn program_literal_is_a_distinct_ordinary_value() {
    let nested = |constant: i64| EmirProgram {
        ops: vec![(EmirOp::ConstI64(constant), Span::default())],
        result: EmirValue(0),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let (first, second) = (nested(7), nested(8));
    assert_ne!(
        Value::Program(first.clone()),
        Value::Program(second.clone()),
        "two distinct programs must remain distinct values"
    );

    let literal = EmirProgram {
        ops: vec![(EmirOp::ProgramLiteral(first.clone()), Span::default())],
        result: EmirValue(0),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let carried = evaluate_with_budget(&literal, &[], &[], EvalBudget::default())
        .expect("program literal evaluates");
    assert_eq!(carried, Value::Program(first.clone()));
    let dump = format!("{carried}");
    assert!(
        dump.starts_with("program(") && dump.contains("ConstI64(7)"),
        "canonical dump must preserve the artifact: {dump}"
    );

    let argument = EmirProgram {
        ops: vec![
            (EmirOp::ProgramLiteral(first.clone()), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: "std.stochastic.does_not_exist".to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let refused = evaluate_with_budget(&argument, &[], &[], EvalBudget::default())
        .expect_err("unknown capability refuses");
    match refused {
        EvalFault::Arithmetic { detail, .. } => assert_eq!(
            detail, "no installed reference bytecode or native kernel",
            "carrier-as-argument reaches the seam unchanged"
        ),
        other => panic!("expected the unknown-name refusal, got {other:?}"),
    }
}
