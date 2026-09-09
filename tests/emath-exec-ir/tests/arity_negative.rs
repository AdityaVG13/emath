#![forbid(unsafe_code)]
//! Negative tests: exec-ir call arity is enforced in every build, debug or
//! release (bug-hunt residual: debug_assert allowed empty/oversized arg
//! lists to panic or drop operands silently).
//!
//! Arity today lives in the admission layer (universal operator spellings
//! report E-TYPE-012 with the counts); any raw named call that still
//! reaches executable lowering is a typed refusal, never a panic.

use emath_core::{FileId, QualifiedName, Span};
use emath_exec_ir::lower_definition;
use emath_ir::{ExprNode, Literal, SemanticPackage};
use emath_test_harness::{Probe, Source, boot};

const OWNER: Span = Span {
    file: FileId(0),
    start: 0,
    end: 0,
};

fn push_literal(package: &mut SemanticPackage, bits: u64) -> emath_ir::ExprId {
    package.push_expr(ExprNode::Literal(Literal::FloatBits(bits)), OWNER)
}

fn lower_raw_call(name: &str, arity: usize) -> String {
    let mut package = SemanticPackage::new();
    let arguments = (0..arity)
        .map(|_| push_literal(&mut package, 1.0f64.to_bits()))
        .collect();
    let call = package.push_expr(
        ExprNode::Call {
            function: QualifiedName(name.into()),
            arguments,
        },
        OWNER,
    );
    lower_definition(&package, call, &[], &[]).unwrap_err()
}

#[test]
fn arity_negative_contracts() {
    boot();
    let mut ph = Probe::new(
        "arity is a typed error everywhere: admission reports E-TYPE-012 with both counts; raw executable-level named calls refuse typed instead of panicking",
    );
    // Executable-level raw named calls (the shape that used to hit the
    // debug_assert): empty and oversized arg lists must be a typed Err,
    // never a panic and never dropped operands.
    ph.case("empty-unary-call-is-err-not-panic", |ph| {
        let error = lower_raw_call("exp", 0);
        ph.contains("names-function", &error, "`exp`");
        ph.contains("typed-fence", &error, "legacy named call");
    });
    ph.case("oversize-binary-call-is-err-not-panic", |ph| {
        let error = lower_raw_call("pow", 3);
        ph.contains("names-function", &error, "`pow`");
        ph.contains("typed-fence", &error, "legacy named call");
    });
    // Admission-level arity: the universal operator spellings report
    // E-TYPE-012 with the function name and both counts in every build.
    ph.case("admission-arity-empty-min", |ph| {
        Source::from_str("arity-min", "package arity_probe\n\nemath function WrongArityMin:\n    inputs:\n        a: Float64\n    outputs:\n        m: Float64\n    definitions:\n        m = min()\n    tests:\n        example <bad>:\n            expect m == 1.0\n").must_refuse(ph, &["E-TYPE-012"]);
    });
    ph.case("admission-arity-oversized-abs", |ph| {
        Source::from_str("arity-abs", "package arity_probe\n\nemath function WrongArityAbs:\n    inputs:\n        a: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = abs(a, 1.0)\n    tests:\n        example <bad>:\n            expect y == 1.0\n").must_refuse(ph, &["E-TYPE-012"]);
    });
    // Positive control: the same spelling with the right arity admits and
    // its numeric example passes.
    ph.case("single-argument-call-still-works", |ph| {
        Source::from_str("arity-good", "package arity_probe\n\nemath function AbsWorks:\n    inputs:\n        a: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = abs(a)\n    tests:\n        example <good>:\n            given a = -3.0\n            expect y == 3.0\n").eval_tests(ph);
    });
    ph.finish();
}
