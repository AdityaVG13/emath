mod exec_ir {
    use emath_core::{QualifiedName, Span};
    use emath_exec_ir::interp::{Value, evaluate};
    use emath_exec_ir::{EmirExprRef, EmirOp, EmirProgram, EmirValue, lower_definition};
    use emath_ir::{BinderKind, BinderVariable, ExprId, ExprNode, Literal, SemanticPackage};

    fn package_with(expr: ExprNode) -> (SemanticPackage, EmirExprRef) {
        let mut package = SemanticPackage::default();
        package.exprs.push(expr);
        (package, ExprId(0))
    }

    fn hand_program(ops: Vec<EmirOp>) -> EmirProgram {
        let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
        EmirProgram {
            ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
            result: EmirValue(last),
            input_count: 0,
            state_count: 0,
            domain_obligations: Vec::new(),
        }
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

    #[test]
    fn i64_fitting_integer_literal_lowers_to_const_i64() {
        let (package, expr) = package_with(ExprNode::Literal(Literal::Integer(
            "9007199254740993".into(),
        )));
        let program = lower_definition(&package, expr, &[], &[]).unwrap();
        assert!(
            matches!(program.ops[0].0, EmirOp::ConstI64(9007199254740993)),
            "2^53+1 must stay ConstI64, got {:?}",
            program.ops[0].0
        );
        assert_eq!(
            evaluate(&program, &[], &[]).unwrap(),
            Value::I64(9007199254740993)
        );
    }

    /// Boolean literals must stay `ConstBool`. Encoding them as `ConstF64`
    /// 1.0/0.0 collapsed `true` with the float 1.0 and made `require: true`
    /// a type confusion (`Value::F64` instead of `Value::Bool`).
    #[test]
    fn bool_literal_lowers_to_const_bool_not_f64() {
        let (package, expr) = package_with(ExprNode::Literal(Literal::Bool(true)));
        let program = lower_definition(&package, expr, &[], &[]).unwrap();
        assert!(
            matches!(program.ops[0].0, EmirOp::ConstBool(true)),
            "true lowered to {:?}, expected ConstBool(true)",
            program.ops[0].0
        );
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(true));

        let (package, expr) = package_with(ExprNode::Literal(Literal::Bool(false)));
        let program = lower_definition(&package, expr, &[], &[]).unwrap();
        assert!(matches!(program.ops[0].0, EmirOp::ConstBool(false)));
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(false));
    }

    /// `print` used to emit only `op.name()`, so `a-b` and `b-a` dumped
    /// identically and `ConstBool(true)` collided with `ConstBool(false)`.
    #[test]
    fn print_preserves_operand_order_and_const_bool() {
        let a_minus_b = hand_program(vec![
            EmirOp::LoadInput(0),
            EmirOp::LoadInput(1),
            EmirOp::F64Sub(EmirValue(0), EmirValue(1)),
        ]);
        let b_minus_a = hand_program(vec![
            EmirOp::LoadInput(0),
            EmirOp::LoadInput(1),
            EmirOp::F64Sub(EmirValue(1), EmirValue(0)),
        ]);
        let print_ab = a_minus_b.print();
        let print_ba = b_minus_a.print();
        assert_ne!(
            print_ab, print_ba,
            "operand-swapped subtract must not share a dump:\n{print_ab}\n{print_ba}"
        );
        assert!(
            print_ab.contains("f64-sub %0 %1"),
            "expected register operands in dump, got {print_ab}"
        );
        assert!(print_ba.contains("f64-sub %1 %0"), "got {print_ba}");

        let true_dump = hand_program(vec![EmirOp::ConstBool(true)]).print();
        let false_dump = hand_program(vec![EmirOp::ConstBool(false)]).print();
        assert_ne!(true_dump, false_dump, "ConstBool payload was dropped");
        assert!(true_dump.contains("const-bool true"), "got {true_dump}");
        assert!(false_dump.contains("const-bool false"), "got {false_dump}");

        let one = hand_program(vec![EmirOp::ConstF64(1.0f64.to_bits())]).print();
        let two = hand_program(vec![EmirOp::ConstF64(2.0f64.to_bits())]).print();
        assert_ne!(one, two, "ConstF64 payload was dropped");
        assert_ne!(
            true_dump, one,
            "ConstBool(true) must not dump as ConstF64(1.0)"
        );
    }

    /// Forall/exists init used `ConstF64(1.0/0.0)`, losing the boolean
    /// identity the fold combine actually computes.
    #[test]
    fn forall_vacuous_init_is_const_bool() {
        let mut package = SemanticPackage::default();
        let start = package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let end = package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let domain = package.push_expr(ExprNode::Vector(vec![start, end]), Span::default());
        let body = package.push_expr(ExprNode::Literal(Literal::Bool(true)), Span::default());
        let forall = package.push_expr(
            ExprNode::Binder {
                kind: BinderKind::ForAll,
                variables: vec![BinderVariable {
                    name: "i".to_string(),
                    domain,
                }],
                body,
            },
            Span::default(),
        );
        let program = lower_definition(&package, forall, &[], &[]).unwrap();
        assert!(
            program
                .ops
                .iter()
                .any(|(op, _)| matches!(op, EmirOp::ConstBool(true))),
            "forall init must be ConstBool(true), got {}",
            program.print()
        );
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(true));
    }
}
