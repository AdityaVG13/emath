#![forbid(unsafe_code)]
//! Negative tests: exec-ir call arity is enforced in every build, debug or
//! release (bug-hunt residual: debug_assert allowed empty/oversized arg
//! lists to panic or drop operands silently).

use emath_core::{FileId, QualifiedName, Span};
use emath_exec_ir::lower_definition;
use emath_ir::{ExprNode, Literal, SemanticPackage};

const OWNER: Span = Span {
    file: FileId(0),
    start: 0,
    end: 0,
};

#[test]
fn empty_unary_call_is_err_not_panic() {
    let mut package = SemanticPackage::new();
    let bad = package.push_expr(
        ExprNode::Call {
            function: QualifiedName("exp".into()),
            arguments: vec![],
        },
        OWNER,
    );
    assert!(lower_definition(&package, bad, &[], &[]).is_err());
}

#[test]
fn oversize_binary_call_is_err_not_panic() {
    let mut package = SemanticPackage::new();
    let one = package.push_expr(ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())), OWNER);
    let two = package.push_expr(ExprNode::Literal(Literal::FloatBits(2.0f64.to_bits())), OWNER);
    let three = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(3.0f64.to_bits())),
        OWNER,
    );
    let bad = package.push_expr(
        ExprNode::Call {
            function: QualifiedName("pow".into()),
            arguments: vec![one, two, three],
        },
        OWNER,
    );
    assert!(lower_definition(&package, bad, &[], &[]).is_err());
}

#[test]
fn single_arg_unary_call_still_lowers() {
    let mut package = SemanticPackage::new();
    let one = package.push_expr(ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())), OWNER);
    let good = package.push_expr(
        ExprNode::Call {
            function: QualifiedName("exp".into()),
            arguments: vec![one],
        },
        OWNER,
    );
    assert!(lower_definition(&package, good, &[], &[]).is_ok());
}
