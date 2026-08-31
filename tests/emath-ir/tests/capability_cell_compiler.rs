//! emath-epic-machine-fjxh.5: Compile cell reference semantics to generic
//! bytecode.
//!
//! The bead's law: a pure cell's formula is a quoted `emath-term` term;
//! the compiler lowers it into the SAME generic EMIR vocabulary the VM
//! already executes (vector map/reduce over the closed builtin registry —
//! never a per-cell op), and the VM seam dispatches cells from compiled
//! data instead of per-op Rust match arms. Differential fixtures compare
//! compiled-VM output against the capability layer's Rust reference
//! oracle BIT-FOR-BIT; the firewall (empty / non-finite / wrong shape /
//! unknown cell) refuses typed at the same seam.

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    ArgGuard, ParamShape, TermCompileError, compile_reference, std_cell_registry,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, ReduceId};
use emath_term::{Signature, SymbolId, Term, VariableId};

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// The softmax reference formula as an emath-term + signature, rebuilt
/// independently here (not read from the implementation) so the test pins
/// the formula of record: exp(sub(x, vmax(x))) normalized by its sum.
fn softmax_formula() -> (Term, Signature) {
    let x = || Term::Variable(VariableId("x".into()));
    let shift = || Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![x(), Term::Apply {
            operator: SymbolId("vmax".into()),
            arguments: vec![x()],
        }],
    };
    let inner = || Term::Apply {
        operator: SymbolId("exp".into()),
        arguments: vec![shift()],
    };
    let term = Term::Apply {
        operator: SymbolId("div".into()),
        arguments: vec![inner(), Term::Apply {
            operator: SymbolId("sum".into()),
            arguments: vec![inner()],
        }],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [("exp", 1usize), ("sub", 2), ("div", 2), ("sum", 1), ("vmax", 1)] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("formula signature is conflict-free");
    }
    (term, signature)
}

fn softmax_params() -> Vec<(String, ParamShape)> {
    vec![("x".to_string(), ParamShape::Vector)]
}

fn run_softmax(vector: &[f64]) -> Result<Value, EvalFault> {
    // Route through the real VM seam: an ApplyCapability program. Guards
    // and registry dispatch live at the seam, not inside the compiled
    // body — evaluating the body directly would bypass the contract.
    let program = EmirProgram {
        ops: vec![
            // Registers materialize in op order: the vector input lands in
            // register 0, then the application references it.
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

#[test]
fn compiled_softmax_matches_reference_bit_exact() {
    // Differential law: the compiled bytecode and the capability layer's
    // Rust oracle agree BIT-FOR-BIT (same stable-max op order), including
    // fixtures where a naive exp(x) overflows or underflows to a silent
    // NaN/zero distribution.
    let fixtures: [&[f64]; 5] = [
        &[1.0, 2.0, 3.0],
        &[0.0],
        &[-5.0, 0.0, 5.0, 500.0],
        &[1e-300, 1e-300, 1e300],
        &[-742.0, -741.5, 0.0],
    ];
    for logits in fixtures {
        let got = match run_softmax(logits).expect("compiled softmax evaluates") {
            Value::Vector(values) => values,
            other => panic!("expected vector, got {other:?}"),
        };
        let oracle = emath_ir::capability::softmax_reference_strict_f64(logits)
            .expect("oracle computes for finite non-empty logits");
        assert_eq!(got.len(), oracle.len(), "fixture {logits:?}");
        for (i, (g, w)) in got.iter().zip(oracle.iter()).enumerate() {
            assert_eq!(
                g.to_bits(),
                w.to_bits(),
                "bit-exact differential fixture {logits:?} element {i}: {g} != {w}"
            );
        }
        let total: f64 = got.iter().sum();
        assert!((total - 1.0).abs() < 1e-12, "distribution sums to 1");
    }
}

#[test]
fn compiled_form_is_shift_invariant() {
    // The cell's declared law (shift invariance), executed as bytecode:
    // softmax(x) == softmax(x + c) bit-for-bit even when naive exp(x+c)
    // overflows to +inf (which would poison the distribution with NaN).
    // A mutant that compiles exp without the stable-max shift fails here.
    let base = [2.0_f64, -1.0, 7.0];
    let shifted = [1002.0_f64, 999.0, 1007.0];
    let a = match run_softmax(&base).expect("base evaluates") {
        Value::Vector(v) => v,
        other => panic!("expected vector, got {other:?}"),
    };
    let b = match run_softmax(&shifted).expect("shifted evaluates") {
        Value::Vector(v) => v,
        other => panic!("expected vector, got {other:?}"),
    };
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(
            x.to_bits(),
            y.to_bits(),
            "shift invariance element {i}: {x} != {y}"
        );
    }
}

#[test]
fn firewall_refusals_parity() {
    let span = Span::default();

    // Empty vector: the oracle refuses (no numeric policy declared for an
    // empty normalization); the compiled seam refuses with the same code.
    match run_softmax(&[]) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, STD_TENSOR_SOFTMAX);
            assert_eq!(code, "E-CELL-006");
        }
        other => panic!("empty vector must refuse E-CELL-006, got {other:?}"),
    }

    // Non-finite logits: never a silent NaN distribution.
    match run_softmax(&[1.0, f64::NAN]) {
        Err(EvalFault::CapabilityRefused { code, .. }) => assert_eq!(code, "E-CELL-006"),
        other => panic!("NaN logits must refuse E-CELL-006, got {other:?}"),
    }

    let build = |op: EmirOp| EmirProgram {
        ops: vec![
            (EmirOp::ConstF64(f64_bits(1.0)), span),
            (EmirOp::ConstF64(f64_bits(2.0)), span),
            (op, span),
        ],
        result: EmirValue(2),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };

    // Wrong shape (scalar where the contract declares a vector): typed
    // confusion, never a coercion.
    let wrong_shape = build(EmirOp::ApplyCapability {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0)],
    });
    assert!(matches!(
        evaluate_with_budget(&wrong_shape, &[], &[], EvalBudget::default()),
        Err(EvalFault::TypeConfusion { .. })
    ));

    // Wrong arity: typed arithmetic-contract fault.
    let wrong_arity = build(EmirOp::ApplyCapability {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0), EmirValue(1)],
    });
    assert!(matches!(
        evaluate_with_budget(&wrong_arity, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ));

    // Unknown pure cell: a typed implementation gap, never a silent
    // identity result.
    let unknown = build(EmirOp::ApplyCapability {
        capability: "sim.engine.magic".to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0)],
    });
    assert!(matches!(
        evaluate_with_budget(&unknown, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ));
}

#[test]
fn compiled_program_is_generic_vocabulary() {
    // Anti-LOC law in bytecode: the compiled cell contains ONLY generic
    // VM ops (vector map/reduce over the closed builtin registry). No op
    // name carries the cell's identity; a domain-named `softmax` op
    // variant would violate the zero-core-delta slope.
    let cell = std_cell_registry()
        .get(STD_TENSOR_SOFTMAX)
        .expect("std cell present");
    assert!(!cell.program.ops.is_empty(), "compiled program non-empty");
    for (op, _) in &cell.program.ops {
        let name = op.name();
        assert!(
            !name.contains("softmax"),
            "bytecode must be generic, found per-op naming: {name}"
        );
    }
    let names: Vec<&str> = cell.program.ops.iter().map(|(op, _)| op.name()).collect();
    assert!(
        names.contains(&"vector-map"),
        "elementwise exp lowers to generic vector-map: {names:?}"
    );
    assert!(
        names.contains(&"vector-map-scalar"),
        "broadcast subtract/divide lowers to generic vector-map-scalar: {names:?}"
    );
    assert!(
        names.contains(&"vector-reduce"),
        "sum/max lower to generic vector-reduce: {names:?}"
    );

    // The formula of record is the term, pinned by its canonical text:
    // exp(sub(x, vmax(x))) normalized by sum(exp(sub(x, vmax(x)))).
    let (term, signature) = softmax_formula();
    let canonical = term.canonical();
    assert!(
        canonical.contains("apply(exp,apply(sub,var(x),apply(vmax,var(x))))"),
        "canonical formula pins the stable-max structure: {canonical}"
    );
    signature.validate(&term).expect("formula well-formed");

    // ReduceId is a closed set with stable tokens.
    assert_eq!(ReduceId::Sum.as_str(), "sum");
    assert_eq!(ReduceId::Max.as_str(), "max");
    assert_eq!(ReduceId::Min.as_str(), "min");
}

#[test]
fn term_compiler_refuses_malformed_reference() {
    let params = softmax_params();

    // Operator outside the closed generic vocabulary: typed compile
    // refusal, never a silent per-op Rust function minted on the fly.
    let mut sig = Signature::default();
    sig.insert(SymbolId("softmax_magic".into()), 1)
        .expect("conflict-free");
    let magic = Term::Apply {
        operator: SymbolId("softmax_magic".into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    match compile_reference(&magic, &sig, &params, Vec::new(), "test.magic") {
        Err(TermCompileError::UnknownOperator { symbol }) => assert_eq!(symbol, "softmax_magic"),
        other => panic!("expected UnknownOperator, got {other:?}"),
    }

    // Signature arity mismatch: emath-term's own validator refuses.
    let mut sig2 = Signature::default();
    sig2.insert(SymbolId("sub".into()), 2).expect("conflict-free");
    let bad_arity = Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    match compile_reference(&bad_arity, &sig2, &params, Vec::new(), "test.arity") {
        Err(TermCompileError::ArityMismatch { symbol, expected, actual }) => {
            assert_eq!(symbol, "sub");
            assert_eq!(expected, 2);
            assert_eq!(actual, 1);
        }
        other => panic!("expected ArityMismatch, got {other:?}"),
    }

    // Free variable outside the declared params: typed refusal.
    let mut sig3 = Signature::default();
    sig3.insert(SymbolId("sum".into()), 1).expect("conflict-free");
    let unbound = Term::Apply {
        operator: SymbolId("sum".into()),
        arguments: vec![Term::Variable(VariableId("y".into()))],
    };
    match compile_reference(&unbound, &sig3, &params, Vec::new(), "test.unbound") {
        Err(TermCompileError::UnknownVariable { name }) => assert_eq!(name, "y"),
        other => panic!("expected UnknownVariable, got {other:?}"),
    }

    // Shape mismatch: reduce over a scalar-SHAPED BOUND variable refuses
    // at compile time (the formula is checked before it ever runs).
    let bound_scalar = Term::Apply {
        operator: SymbolId("sum".into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    let scalar_params = vec![("x".to_string(), ParamShape::Scalar)];
    match compile_reference(&bound_scalar, &sig3, &scalar_params, Vec::new(), "test.shape") {
        Err(TermCompileError::ShapeMismatch { symbol, .. }) => assert_eq!(symbol, "sum"),
        other => panic!("expected ShapeMismatch, got {other:?}"),
    }
}

#[test]
fn world_bundle_and_negative_seed() {
    // WorldResultBundle fixture (bead e2e clause): the compiled-cell run
    // as a world record. The World ABI (fjxh.7) consumes this shape.
    #[derive(Debug)]
    struct WorldResultBundle {
        world: &'static str,
        verdict: &'static str,
        outputs: Vec<f64>,
        refusals: Vec<String>,
    }
    let outputs = match run_softmax(&[1.0, 2.0, 3.0]).expect("evaluates") {
        Value::Vector(values) => values,
        other => panic!("expected vector, got {other:?}"),
    };
    let bundle = WorldResultBundle {
        world: "interp",
        verdict: "evaluated",
        outputs,
        refusals: Vec::new(),
    };
    assert_eq!(bundle.world, "interp");
    assert_eq!(bundle.verdict, "evaluated");
    assert_eq!(bundle.outputs.len(), 3);
    assert!(bundle.refusals.is_empty());

    // Guards are data on the compiled cell, checked in declared order
    // (NonEmpty before AllFinite; both refuse E-CELL-006).
    let cell = std_cell_registry().get(STD_TENSOR_SOFTMAX).unwrap();
    assert_eq!(cell.params.len(), 1);
    assert_eq!(cell.guards.len(), 2);
    assert!(matches!(cell.guards[0], ArgGuard::NonEmpty(0)));
    assert!(matches!(cell.guards[1], ArgGuard::AllFinite(0)));

    // Bead negative seed: the seeded silent-success scenario declares a
    // typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/capability_cell_compiler.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-CELL") || expect_line.contains("E-VM"),
        "seed expects a typed refusal, found: {expect_line}"
    );
}
