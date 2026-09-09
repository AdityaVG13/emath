//! Lowering contracts for executable IR construction.
//!
//! Literals must keep their type identity through lowering (`ConstI64`
//! stays exact past 2^53, booleans stay `ConstBool`), arity is enforced
//! with a typed error, and `print` must preserve operand order and
//! payloads. The F042 arity negatives live in `arity_negative.rs`
//! (same crate, shared target via this module include).

mod exec_ir {
    use emath_core::{QualifiedName, Span};
    use emath_exec_ir::interp::{Value, evaluate};
    use emath_exec_ir::{EmirExprRef, EmirOp, EmirProgram, EmirValue, lower_definition};
    use emath_ir::{BinderKind, BinderVariable, ExprId, ExprNode, Literal, SemanticPackage};
    use emath_test_harness::Probe;

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
    fn lowering_contracts() {
        let mut ph = Probe::new(
            "call arity and literal identity survive lowering: strict-f64 fence, exact I64, ConstBool, lossless print",
        );
        // Zero-argument `exp` is a typed arity error that names the
        // function.
        ph.case("raw-named-call-is-typed-refusal", |ph| {
            // A raw `Call` that admission can no longer produce (universal
            // spellings resolve to ops; everything else to FeatureID
            // applications) must be a typed error at lowering, never a panic.
            let (package, expr) = package_with(ExprNode::Call {
                function: QualifiedName::single("exp"),
                arguments: vec![],
            });
            let error = lower_definition(&package, expr, &[], &[]).unwrap_err();
            ph.contains("names-function", &error, "`exp`");
            ph.contains("typed-fence", &error, "legacy named call");
        });
        // A 400-digit integer literal cannot be an exact f64.
        ph.case("oversized-integer-literal-is-refused", |ph| {
            let (package, expr) =
                package_with(ExprNode::Literal(Literal::Integer("9".repeat(400))));
            let error = lower_definition(&package, expr, &[], &[]).unwrap_err();
            ph.contains("strict-f64", &error, "strict-f64");
        });
        // 2^53+1 is not an f64 integer: it must stay ConstI64 and
        // evaluate back exactly.
        ph.case("i64-fitting-integer-literal-lowers-to-const-i64", |ph| {
            let (package, expr) = package_with(ExprNode::Literal(Literal::Integer(
                "9007199254740993".into(),
            )));
            let program = lower_definition(&package, expr, &[], &[]).unwrap();
            ph.demand("op", matches!(program.ops[0].0, EmirOp::ConstI64(9007199254740993)), format!("got {:?}", program.ops[0].0));
            ph.eq("eval", evaluate(&program, &[], &[]).unwrap(), Value::I64(9007199254740993));
        });
        // Boolean literals must stay `ConstBool`. Encoding them as
        // `ConstF64` 1.0/0.0 collapsed `true` with the float 1.0 and made
        // `require: true` a type confusion.
        ph.case("bool-literal-lowers-to-const-bool-not-f64", |ph| {
            for (value, expected) in [(true, EmirOp::ConstBool(true)), (false, EmirOp::ConstBool(false))] {
                let (package, expr) = package_with(ExprNode::Literal(Literal::Bool(value)));
                let program = lower_definition(&package, expr, &[], &[]).unwrap();
                ph.demand(
                    format!("lowered {value}"),
                    matches!(&program.ops[0].0, EmirOp::ConstBool(v) if *v == value),
                    format!("expected ConstBool({value}), got {:?}", program.ops[0].0),
                );
                ph.eq(
                    format!("evaluated {value}"),
                    evaluate(&program, &[], &[]).unwrap(),
                    Value::Bool(value),
                );
            }
        });
        // `print` used to emit only `op.name()`, so `a-b` and `b-a`
        // dumped identically and ConstBool payloads collided.
        ph.case("print-preserves-operand-order-and-const-bool", |ph| {
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
            ph.ne("operand-swap-dump", print_ab.clone(), print_ba.clone());
            ph.contains("ab-order", &print_ab, "f64-sub %0 %1");
            ph.contains("ba-order", &print_ba, "f64-sub %1 %0");
            let true_dump = hand_program(vec![EmirOp::ConstBool(true)]).print();
            let false_dump = hand_program(vec![EmirOp::ConstBool(false)]).print();
            ph.ne("bool-payload", true_dump.clone(), false_dump.clone());
            ph.contains("bool-true", &true_dump, "const-bool true");
            ph.contains("bool-false", &false_dump, "const-bool false");
            let one = hand_program(vec![EmirOp::ConstF64(1.0f64.to_bits())]).print();
            let two = hand_program(vec![EmirOp::ConstF64(2.0f64.to_bits())]).print();
            ph.ne("f64-payload", one.clone(), two.clone());
            ph.ne("bool-vs-float", true_dump, one);
        });
        // Forall/exists init used `ConstF64(1.0/0.0)`, losing the boolean
        // identity the fold combine actually computes.
        ph.case("forall-vacuous-init-is-const-bool", |ph| {
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
            ph.demand(
                "const-bool-true",
                program
                    .ops
                    .iter()
                    .any(|(op, _)| matches!(op, EmirOp::ConstBool(true))),
                format!("forall init must be ConstBool(true), got {}", program.print()),
            );
            ph.eq("eval", evaluate(&program, &[], &[]).unwrap(), Value::Bool(true));
        });
        ph.finish();
    }
}

// F042: the empty/oversized arity negatives live in
// `arity_negative.rs` (same crate, shared target via this module
// include).
mod arity_negative;
