//! Compile cell reference semantics to generic
//! bytecode.
//!
//! The law: a pure cell's formula is a quoted `emath-term` term;
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
    ArgGuard, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, ReduceId};
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

/// The softmax reference formula as an emath-term + signature, rebuilt
/// independently here (not read from the implementation) so the test pins
/// the formula of record: exp(sub(x, vmax(x))) normalized by its sum.
fn softmax_formula() -> (Term, Signature) {
    let x = || Term::Variable(VariableId("x".into()));
    let shift = || Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![
            x(),
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![x()],
            },
        ],
    };
    let inner = || Term::Apply {
        operator: SymbolId("exp".into()),
        arguments: vec![shift()],
    };
    let term = Term::Apply {
        operator: SymbolId("div".into()),
        arguments: vec![
            inner(),
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![inner()],
            },
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("exp", 1usize),
        ("sub", 2),
        ("div", 2),
        ("sum", 1),
        ("vmax", 1),
    ] {
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
fn intent() {
    let mut p = Probe::new("Compile cell reference semantics to generic");
    p.case("compiled_softmax_matches_reference_bit_exact", |p| {

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
            other => { p.fail("compiled_softmax_matches_reference_bit_exact#1", format!("expected vector, got {other:?}")); return; },
        };
        let oracle = emath_ir::capability::softmax_reference_strict_f64(logits)
            .expect("oracle computes for finite non-empty logits");
        p.eq(format!("fixture {logits:?}"), got.len(), oracle.len());
        for (i, (g, w)) in got.iter().zip(oracle.iter()).enumerate() {
            p.eq(format!("bit-exact differential fixture {logits:?} element {i}: {g} != {w}"), g.to_bits(), w.to_bits());
        }
        let total: f64 = got.iter().sum();
        p.demand("distribution sums to 1", (total - 1.0).abs() < 1e-12, "distribution sums to 1");
    }

    });
    p.case("compiled_form_is_shift_invariant", |p| {

    // The cell's declared law (shift invariance), executed as bytecode:
    // softmax(x) == softmax(x + c) bit-for-bit even when naive exp(x+c)
    // overflows to +inf (which would poison the distribution with NaN).
    // A mutant that compiles exp without the stable-max shift fails here.
    let base = [2.0_f64, -1.0, 7.0];
    let shifted = [1002.0_f64, 999.0, 1007.0];
    let a = match run_softmax(&base).expect("base evaluates") {
        Value::Vector(v) => v,
        other => { p.fail("compiled_form_is_shift_invariant#1", format!("expected vector, got {other:?}")); return; },
    };
    let b = match run_softmax(&shifted).expect("shifted evaluates") {
        Value::Vector(v) => v,
        other => { p.fail("compiled_form_is_shift_invariant#2", format!("expected vector, got {other:?}")); return; },
    };
    p.eq("compiled_form_is_shift_invariant#3", a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        p.eq(format!("shift invariance element {i}: {x} != {y}"), x.to_bits(), y.to_bits());
    }

    });
    p.case("firewall_refusals_parity", |p| {

    let span = Span::default();

    // Empty vector: the oracle refuses (no numeric policy declared for an
    // empty normalization); the compiled seam refuses with the same code.
    match run_softmax(&[]) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            p.eq("firewall_refusals_parity#1", capability, STD_TENSOR_SOFTMAX.to_string());
            p.demand("firewall_refusals_parity#2", code == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", code));
        }
        other => { p.fail("firewall_refusals_parity#3", format!("empty vector must refuse E-CELL-006, got {other:?}")); return; },
    }

    // Non-finite logits: never a silent NaN distribution.
    match run_softmax(&[1.0, f64::NAN]) {
        Err(EvalFault::CapabilityRefused { code, .. }) => { p.demand("firewall_refusals_parity#4", code == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", code)); },
        other => { p.fail("firewall_refusals_parity#5", format!("NaN logits must refuse E-CELL-006, got {other:?}")); return; },
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
    p.demand("firewall_refusals_parity#6", matches!(
        evaluate_with_budget(&wrong_shape, &[], &[], EvalBudget::default()),
        Err(EvalFault::TypeConfusion { .. })
    ), "firewall_refusals_parity#6: matches!(\n        evaluate_with_budget(&wrong_shape, &[], &[], EvalBudget::default()),\n        Err(E");

    // Wrong arity: typed arithmetic-contract fault.
    let wrong_arity = build(EmirOp::ApplyCapability {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0), EmirValue(1)],
    });
    p.demand("firewall_refusals_parity#7", matches!(
        evaluate_with_budget(&wrong_arity, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ), "firewall_refusals_parity#7: matches!(\n        evaluate_with_budget(&wrong_arity, &[], &[], EvalBudget::default()),\n        Err(E");

    // Unknown pure cell: a typed implementation gap, never a silent
    // identity result.
    let unknown = build(EmirOp::ApplyCapability {
        capability: "sim.engine.magic".to_string(),
        class: CellClass::Pure,
        args: vec![EmirValue(0)],
    });
    p.demand("firewall_refusals_parity#8", matches!(
        evaluate_with_budget(&unknown, &[], &[], EvalBudget::default()),
        Err(EvalFault::Arithmetic { .. })
    ), "firewall_refusals_parity#8: matches!(\n        evaluate_with_budget(&unknown, &[], &[], EvalBudget::default()),\n        Err(EvalF");

    });
    p.case("compiled_program_is_generic_vocabulary", |p| {

    // Anti-LOC law in bytecode: the compiled cell contains ONLY generic
    // VM ops (vector map/reduce over the closed builtin registry). No op
    // name carries the cell's identity; a domain-named `softmax` op
    // variant would violate the zero-core-delta slope.
    let registry = std_cell_registry();
    let cell = registry
        .get(STD_TENSOR_SOFTMAX)
        .expect("std cell present");
    p.demand("compiled program non-empty", !cell.program.ops.is_empty(), "compiled program non-empty");
    for (op, _) in &cell.program.ops {
        let name = op.name();
        p.demand(format!("bytecode must be generic, found per-op naming: {name}"), !name.contains("softmax"), format!("bytecode must be generic, found per-op naming: {name}"));
    }
    let names: Vec<&str> = cell.program.ops.iter().map(|(op, _)| op.name()).collect();
    p.demand(format!("elementwise exp lowers to generic vector-map: {names:?}"), names.contains(&"vector-map"), format!("elementwise exp lowers to generic vector-map: {names:?}"));
    p.demand(format!("broadcast subtract/divide lowers to generic vector-map-scalar: {names:?}"), names.contains(&"vector-map-scalar"), format!("broadcast subtract/divide lowers to generic vector-map-scalar: {names:?}"));
    p.demand(format!("sum/max lower to generic vector-reduce: {names:?}"), names.contains(&"vector-reduce"), format!("sum/max lower to generic vector-reduce: {names:?}"));

    // The formula of record is the term, pinned by its canonical text:
    // exp(sub(x, vmax(x))) normalized by sum(exp(sub(x, vmax(x)))).
    let (term, signature) = softmax_formula();
    let canonical = term.canonical();
    p.demand(format!("canonical formula pins the stable-max structure: {canonical}"), canonical.contains("apply(exp,apply(sub,var(x),apply(vmax,var(x))))"), format!("canonical formula pins the stable-max structure: {canonical}"));
    signature.validate(&term).expect("formula well-formed");

    // ReduceId is a closed set with stable tokens.
    p.demand("compiled_program_is_generic_vocabulary#7", ReduceId::Sum.as_str() == "sum", format!("expected {:?}, got {:?}", "sum", ReduceId::Sum.as_str()));
    p.demand("compiled_program_is_generic_vocabulary#8", ReduceId::Max.as_str() == "max", format!("expected {:?}, got {:?}", "max", ReduceId::Max.as_str()));
    p.demand("compiled_program_is_generic_vocabulary#9", ReduceId::Min.as_str() == "min", format!("expected {:?}, got {:?}", "min", ReduceId::Min.as_str()));

    });
    p.case("term_compiler_refuses_malformed_reference", |p| {

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
        Err(TermCompileError::UnknownOperator { symbol }) => { p.demand("term_compiler_refuses_malformed_reference#1", symbol == "softmax_magic", format!("expected {:?}, got {:?}", "softmax_magic", symbol)); },
        other => { p.fail("term_compiler_refuses_malformed_reference#2", format!("expected UnknownOperator, got {other:?}")); return; },
    }

    // Signature arity mismatch: emath-term's own validator refuses.
    let mut sig2 = Signature::default();
    sig2.insert(SymbolId("sub".into()), 2)
        .expect("conflict-free");
    let bad_arity = Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    match compile_reference(&bad_arity, &sig2, &params, Vec::new(), "test.arity") {
        Err(TermCompileError::ArityMismatch {
            symbol,
            expected,
            actual,
        }) => {
            p.demand("term_compiler_refuses_malformed_reference#3", symbol == "sub", format!("expected {:?}, got {:?}", "sub", symbol));
            p.eq("term_compiler_refuses_malformed_reference#4", expected, 2);
            p.eq("term_compiler_refuses_malformed_reference#5", actual, 1);
        }
        other => { p.fail("term_compiler_refuses_malformed_reference#6", format!("expected ArityMismatch, got {other:?}")); return; },
    }

    // Free variable outside the declared params: typed refusal.
    let mut sig3 = Signature::default();
    sig3.insert(SymbolId("sum".into()), 1)
        .expect("conflict-free");
    let unbound = Term::Apply {
        operator: SymbolId("sum".into()),
        arguments: vec![Term::Variable(VariableId("y".into()))],
    };
    match compile_reference(&unbound, &sig3, &params, Vec::new(), "test.unbound") {
        Err(TermCompileError::UnknownVariable { name }) => { p.demand("term_compiler_refuses_malformed_reference#7", name == "y", format!("expected {:?}, got {:?}", "y", name)); },
        other => { p.fail("term_compiler_refuses_malformed_reference#8", format!("expected UnknownVariable, got {other:?}")); return; },
    }

    // Shape mismatch: reduce over a scalar-SHAPED BOUND variable refuses
    // at compile time (the formula is checked before it ever runs).
    let bound_scalar = Term::Apply {
        operator: SymbolId("sum".into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    let scalar_params = vec![("x".to_string(), ParamShape::Scalar)];
    match compile_reference(
        &bound_scalar,
        &sig3,
        &scalar_params,
        Vec::new(),
        "test.shape",
    ) {
        Err(TermCompileError::ShapeMismatch { symbol, .. }) => { p.demand("term_compiler_refuses_malformed_reference#9", symbol == "sum", format!("expected {:?}, got {:?}", "sum", symbol)); },
        other => { p.fail("term_compiler_refuses_malformed_reference#10", format!("expected ShapeMismatch, got {other:?}")); return; },
    }

    });
    p.case("world_bundle_and_negative_seed", |p| {

    // WorldResultBundle fixture: the compiled-cell run
    // as a world record. The World ABI consumes this shape.
    #[derive(Debug)]
    struct WorldResultBundle {
        world: &'static str,
        verdict: &'static str,
        outputs: Vec<f64>,
        refusals: Vec<String>,
    }
    let outputs = match run_softmax(&[1.0, 2.0, 3.0]).expect("evaluates") {
        Value::Vector(values) => values,
        other => { p.fail("world_bundle_and_negative_seed#1", format!("expected vector, got {other:?}")); return; },
    };
    let bundle = WorldResultBundle {
        world: "interp",
        verdict: "evaluated",
        outputs,
        refusals: Vec::new(),
    };
    p.demand("world_bundle_and_negative_seed#2", bundle.world == "interp", format!("expected {:?}, got {:?}", "interp", bundle.world));
    p.demand("world_bundle_and_negative_seed#3", bundle.verdict == "evaluated", format!("expected {:?}, got {:?}", "evaluated", bundle.verdict));
    p.eq("world_bundle_and_negative_seed#4", bundle.outputs.len(), 3);
    p.demand("world_bundle_and_negative_seed#5", bundle.refusals.is_empty(), "world_bundle_and_negative_seed#5: bundle.refusals.is_empty()");

    // Guards are data on the compiled cell, checked in declared order
    // (NonEmpty before AllFinite; both refuse E-CELL-006).
    let registry = std_cell_registry();
    let cell = registry.get(STD_TENSOR_SOFTMAX).expect("std cell present");
    p.eq("world_bundle_and_negative_seed#6", cell.params.len(), 1);
    p.eq("world_bundle_and_negative_seed#7", cell.guards.len(), 2);
    p.demand("world_bundle_and_negative_seed#8", matches!(cell.guards[0], ArgGuard::NonEmpty(0)), "world_bundle_and_negative_seed#8: matches!(cell.guards[0], ArgGuard::NonEmpty(0))");
    p.demand("world_bundle_and_negative_seed#9", matches!(cell.guards[1], ArgGuard::AllFinite(0)), "world_bundle_and_negative_seed#9: matches!(cell.guards[1], ArgGuard::AllFinite(0))");

    // Negative seed: the seeded silent-success scenario declares a
    // typed refusal.
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/capability_cell_compiler.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects a typed refusal, found: {expect_line}"), expect_line.contains("E-CELL") || expect_line.contains("E-VM"), format!("seed expects a typed refusal, found: {expect_line}"));

    });
    p.finish();
}











