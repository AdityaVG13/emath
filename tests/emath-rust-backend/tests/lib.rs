use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_ir::{
    BinaryOp, CompileSpec, Constructor, Declaration, DeclarationId, DeterminismPolicy,
    EvidenceLevel, ExactnessPolicy, ExprId, ExprNode, FallbackPolicy, Field, Goal, GoalId,
    GoalKind, GoalPayload, GoalRequirements, Literal, ObligationClass, ObligationKind,
    SemanticPackage, TargetProfile, TestCase, TypeId, TypeNode, Visibility,
};
use emath_rust_backend::BackendInput;
use emath_rust_ir::ast::{Item, StructDef};
use emath_rust_ir::render::render_module;

/// A minimal package: one declaration `named` with an `x: Float64`
/// input, nothing else. Enough to exercise struct emission, which is
/// where declaration names become Rust source.
fn package_for(named: &str) -> SemanticPackage {
    let mut package = SemanticPackage::new();
    package.types.push(TypeNode::Float64);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName(named.to_string()),
        kind: QualifiedName("policy".to_string()),
        kind_label: "policy".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: Vec::new(),
        state: Vec::new(),
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    package
}

#[test]
fn keyword_declaration_name_is_escaped_in_generated_rust() {
    // `type` is a Rust keyword; the generated struct must be `type_`
    // so the crate compiles (`emath custom <type>` negative control
    // from the l2pb.4 repair).
    let package = package_for("type");
    let output = BackendInput {
        package: &package,
        crate_name: "type".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("keyword-named declaration must generate");
    let struct_items: Vec<&StructDef> = output
        .module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(struct_def) => Some(struct_def),
            _ => None,
        })
        .collect();
    assert!(
        struct_items.iter().all(|def| def.name != "type"),
        "raw keyword must never be emitted: {struct_items:?}"
    );
    assert!(
        struct_items.iter().any(|def| def.name == "type_"),
        "expected escaped struct `type_`, got {struct_items:?}"
    );
    let rendered = render_module(&output.module).code;
    assert!(
        rendered.contains("struct type_"),
        "rendered module must name the escaped struct, got:\n{rendered}"
    );
    assert!(
        output
            .module
            .items
            .iter()
            .all(|item| !matches!(item, Item::Struct(def) if def.name == "type")),
        "no unescaped keyword struct may reach the output"
    );
}

#[test]
fn generated_constructor_carries_its_construction_receipt() {
    let mut package = package_for("Scorer");
    // exprs: 0 = `scale` (param), 1 = `0.0`, 2 = `scale > 0.0`.
    package
        .exprs
        .push(ExprNode::Variable(QualifiedName("scale".to_string())));
    package
        .exprs
        .push(ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())));
    package.exprs.push(ExprNode::Binary {
        operation: BinaryOp::Greater,
        left: ExprId(0),
        right: ExprId(1),
    });
    package.expr_spans = vec![Span::default(); package.exprs.len()];
    let declaration = &mut package.declarations[0];
    declaration.state = vec![Field {
        name: "scale".to_string(),
        ty: TypeId(0),
        visibility: Visibility::Private,
        source: Span::default(),
    }];
    declaration.constructors.push(Constructor {
        name: "new".to_string(),
        parameters: vec![Field {
            name: "scale".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        preconditions: vec![ExprId(2)],
        assignments: BTreeMap::from([("scale".to_string(), ExprId(0))]),
        postconditions: vec![ExprId(2)],
        defaults: BTreeMap::new(),
        error_type: None,
        is_public: true,
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "scorer".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("constructor package must generate");
    assert_eq!(output.receipts.len(), 1, "one receipt per constructor");
    let receipt = &output.receipts[0];
    assert_eq!(receipt.declaration, "Scorer");
    assert_eq!(receipt.obligations.len(), 2);
    assert!(
        receipt
            .obligations
            .iter()
            .all(|obligation| obligation.class == ObligationClass::Runtime),
        "Phase 1 discharges every textual obligation at runtime"
    );
    assert_eq!(receipt.obligations[0].kind, ObligationKind::Precondition);
    assert_eq!(receipt.obligations[1].kind, ObligationKind::Postcondition);
    assert!(
        receipt.open_obligations().is_empty(),
        "no obligation may remain open on a runtime-discharged receipt"
    );
}

#[test]
fn keyword_crate_name_is_escaped_in_manifest() {
    // Cargo package names may be keywords, but the generated crate
    // must keep a sane rust-identifier crate name for `lib.rs`
    // (`extern crate`/name collisions in dev builds).
    let package = package_for("Demo");
    let output = BackendInput {
        package: &package,
        crate_name: "fn".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("keyword crate name must generate");
    let manifest = output
        .files
        .get("Cargo.toml")
        .expect("generate must emit a manifest");
    assert!(
        manifest.contains("name = \"fn_\""),
        "keyword crate name must be escaped in the manifest, got:\n{manifest}"
    );
    assert!(
        !manifest.contains("name = \"fn\""),
        "unescaped keyword crate name must not reach the manifest"
    );
}

#[test]
fn expect_less_example_generates_computation_without_assert() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: x,
            right: x,
        },
        Span::default(),
    );
    let four = package.push_expr(
        ExprNode::Literal(Literal::Integer("4".to_string())),
        Span::default(),
    );
    let mut given = BTreeMap::new();
    given.insert("x".to_string(), four);
    let test_id = package.push_test(TestCase {
        name: "four_squared".to_string(),
        given,
        expect: None,
        source: Span::default(),
    });
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Square"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: vec![test_id],
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "square".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("worked example must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("let actual") || lib.contains("fn y"),
        "worked example must execute the definition, got:\n{lib}"
    );
    assert!(
        lib.contains("let _ ="),
        "worked example must bind the computed value without asserting, got:\n{lib}"
    );
    assert!(
        !lib.contains("assert!("),
        "worked example must not assert, got:\n{lib}"
    );
}

#[test]
fn constant_only_declaration_generates_parameterless_method() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let three = package.push_expr(
        ExprNode::Literal(Literal::Integer("3".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: three,
            right: seven,
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("TwentyOne"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "twenty_one".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("constant-only declaration must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("fn y()") || lib.contains("fn y(&self)"),
        "no-input declaration must generate a parameterless evaluator, got:\n{lib}"
    );
    assert!(
        !lib.contains("fn y(&self,") && !lib.contains("fn y(&self ,") && !lib.contains("fn y(,"),
        "no-input evaluator must not take extra parameters, got:\n{lib}"
    );
}

#[test]
fn stateless_declaration_emits_free_function() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: x,
            right: x,
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "square".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("square".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("square"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "square".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "square".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("stateless square must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("pub fn square(") && lib.contains("x: f64"),
        "stateless case must emit a free function, got:\n{lib}"
    );
    assert!(
        !lib.contains("struct square") && !lib.contains("&self"),
        "stateless case must not emit a unit struct + method, got:\n{lib}"
    );
    assert!(
        output
            .anchors
            .iter()
            .any(|anchor| anchor.label == "fn square"),
        "source map must anchor the free function, got {:?}",
        output.anchors
    );
}

#[test]
fn chained_definitions_emit_let_bindings_in_source_order() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let file = Span::default().file;
    let two = package.push_expr(
        ExprNode::Literal(Literal::Integer("2".to_string())),
        Span::new(file, 1, 2),
    );
    let a_var = package.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::new(file, 3, 4),
    );
    let b_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: a_var,
            right: a_var,
        },
        Span::new(file, 5, 6),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "b".to_string(),
        expression: Some(b_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("a".to_string(), two);
    definitions.insert("b".to_string(), b_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Chain"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "b".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "chain".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("chained definitions must lower");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("let a =") && lib.contains("pub fn b("),
        "evaluate b must let-bind earlier definition a, got:\n{lib}"
    );
}
