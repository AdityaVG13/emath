//! Native symbolic simplification, rewrite matching, and polynomial decisions.

use emath_core::{QualifiedName, Span};
use emath_ir::{
    BinaryOp, ExprNode, Literal, RewritePattern, RewriteRule, SemanticPackage, SymbolicExpr,
    apply_rewrite, decide_univariate_polynomial_identity, simplify_expression,
    symbolic_oracle_contract,
};

fn integer(package: &mut SemanticPackage, value: i128) -> emath_ir::ExprId {
    package.push_expr(
        ExprNode::Literal(Literal::Integer(value.to_string())),
        Span::default(),
    )
}

fn variable(package: &mut SemanticPackage, name: &str) -> emath_ir::ExprId {
    package.push_expr(
        ExprNode::Variable(QualifiedName(name.into())),
        Span::default(),
    )
}

fn binary(
    package: &mut SemanticPackage,
    operation: BinaryOp,
    left: emath_ir::ExprId,
    right: emath_ir::ExprId,
) -> emath_ir::ExprId {
    package.push_expr(
        ExprNode::Binary {
            operation,
            left,
            right,
        },
        Span::default(),
    )
}

#[test]
fn native_simplify_and_pattern_rewrite_compute() {
    let mut package = SemanticPackage::default();
    let x = variable(&mut package, "x");
    let zero = integer(&mut package, 0);
    let one = integer(&mut package, 1);
    let add_zero = binary(&mut package, BinaryOp::ExactAdd, x, zero);
    let root = binary(&mut package, BinaryOp::ExactMul, add_zero, one);

    let simplified = simplify_expression(&mut package, root).unwrap();
    assert_eq!(
        package.expr(simplified.expression),
        Some(&ExprNode::Variable(QualifiedName("x".into())))
    );
    assert_eq!(
        simplified.rewrites,
        ["add-zero-right", "multiply-one-right"]
    );

    let rule = RewriteRule::new(
        "double",
        RewritePattern::Binary {
            operation: BinaryOp::ExactAdd,
            left: Box::new(RewritePattern::Capture("a".into())),
            right: Box::new(RewritePattern::Capture("a".into())),
        },
        RewritePattern::Binary {
            operation: BinaryOp::ExactMul,
            left: Box::new(RewritePattern::Integer(2)),
            right: Box::new(RewritePattern::Capture("a".into())),
        },
        "structural-checked",
    )
    .unwrap();
    let expression = SymbolicExpr::Binary {
        operation: BinaryOp::ExactAdd,
        left: Box::new(SymbolicExpr::Variable("x".into())),
        right: Box::new(SymbolicExpr::Variable("x".into())),
    };
    assert!(apply_rewrite(&expression, &rule).unwrap().is_some());
}

#[test]
fn univariate_polynomial_identity_is_decided_exactly() {
    let mut package = SemanticPackage::default();
    let x = variable(&mut package, "x");
    let one = integer(&mut package, 1);
    let left_plus = binary(&mut package, BinaryOp::ExactAdd, x, one);
    let left_minus = binary(&mut package, BinaryOp::ExactSub, x, one);
    let left = binary(&mut package, BinaryOp::ExactMul, left_plus, left_minus);
    let two = integer(&mut package, 2);
    let squared = binary(&mut package, BinaryOp::StrictFloatPow, x, two);
    let right = binary(&mut package, BinaryOp::ExactSub, squared, one);

    let decision = decide_univariate_polynomial_identity(&package, left, right, "x").unwrap();
    assert!(decision.equal);
    assert_eq!(decision.left_coefficients, [-1, 0, 1]);
    assert_eq!(decision.right_coefficients, [-1, 0, 1]);
}

#[test]
fn unsupported_claims_and_false_authority_refuse_by_name() {
    let mut package = SemanticPackage::default();
    let call = package.push_expr(
        ExprNode::Call {
            function: QualifiedName("sin".into()),
            arguments: Vec::new(),
        },
        Span::default(),
    );
    let zero = integer(&mut package, 0);
    let refusal = decide_univariate_polynomial_identity(&package, call, zero, "x").unwrap_err();
    assert_eq!(refusal.code, "E-SYM-003");

    let authority = RewriteRule::new(
        "unsupported-proof-claim",
        RewritePattern::Integer(0),
        RewritePattern::Integer(0),
        "proved",
    )
    .unwrap_err();
    assert_eq!(authority.code, "E-SYM-004");

    let contract = symbolic_oracle_contract();
    assert_eq!(contract.schema.0, "emath.symbolic/v1");
}
