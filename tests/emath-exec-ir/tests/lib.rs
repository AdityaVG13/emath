mod exec_ir {
    use emath_core::QualifiedName;
    use emath_exec_ir::{EmirExprRef, lower_definition};
    use emath_ir::{ExprId, ExprNode, Literal, SemanticPackage};

    fn package_with(expr: ExprNode) -> (SemanticPackage, EmirExprRef) {
        let mut package = SemanticPackage::default();
        package.exprs.push(expr);
        (package, ExprId(0))
    }

    #[test]
    fn call_with_wrong_arity_is_refused() {
        let (package, expr) = package_with(ExprNode::Call {
            function: QualifiedName::single("exp"),
            arguments: vec![],
        });
        let error = lower_definition(&package, expr, &[], &[]).unwrap_err();
        assert!(error.contains("expects"), "got {error:?}");
    }

    #[test]
    fn oversized_integer_literal_is_refused() {
        let (package, expr) = package_with(ExprNode::Literal(Literal::Integer("9".repeat(400))));
        let error = lower_definition(&package, expr, &[], &[]).unwrap_err();
        assert!(error.contains("strict-f64"), "got {error:?}");
    }
}
