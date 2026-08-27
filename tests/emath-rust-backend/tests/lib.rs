use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_ir::{
    BinaryOp, BinderKind, BinderVariable, CompileSpec, Constructor, Declaration, DeclarationId,
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExprId, ExprNode, Extent, FallbackPolicy,
    Field, Goal, GoalId, GoalKind, GoalPayload, GoalRequirements, Literal, ObligationClass,
    ObligationKind, SemanticPackage, SliceAxis, TargetProfile, TestCase, TypeId, TypeNode, UnaryOp,
    Visibility,
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
        algebraic: Vec::new(),
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
        algebraic: Vec::new(),
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
    // The embedded `emath_rt` module may legitimately contain `assert!`
    // kernel guards; the invariant applies to the generated user code.
    let user_code = lib.split("mod emath_rt").next().unwrap_or(lib);
    assert!(
        !user_code.contains("assert!("),
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
        algebraic: Vec::new(),
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
        lib.contains("fn TwentyOne()") || lib.contains("fn TwentyOne(&self)"),
        "no-input declaration must generate a parameterless evaluator, got:\n{lib}"
    );
    assert!(
        !lib.contains("fn TwentyOne(&self,")
            && !lib.contains("fn TwentyOne(&self ,")
            && !lib.contains("fn TwentyOne(,"),
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
        algebraic: Vec::new(),
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
        !lib.contains("struct square")
            && !lib.contains("fn square(&self")
            && !lib.contains("fn square(self"),
        "stateless case must not emit a unit struct + method, got:\n{}",
        extract_fn(lib, "square")
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
        algebraic: Vec::new(),
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
        lib.contains("let a =") && lib.contains("pub fn Chain("),
        "evaluate b must let-bind earlier definition a, got:\n{lib}"
    );
}

#[test]
fn causalized_model_emits_newton_step_methods() {
    // Fully implicit DAEs (Newton-solved residuals) now codegen the same
    // causalized Newton solve the interpreter runs: embedded Gaussian
    // helpers, residual closures over the flat solve vector, the 30/1e-9
    // budget mirrored from `causal_newton`, and Result-typed steps.
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let v = package.push_expr(
        ExprNode::Variable(QualifiedName::single("V")),
        Span::default(),
    );
    let i = package.push_expr(
        ExprNode::Variable(QualifiedName::single("I")),
        Span::default(),
    );
    let q = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.q")),
        Span::default(),
    );
    // `(V - I) - q`
    let v_minus_i = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: v,
            right: i,
        },
        Span::default(),
    );
    let residual_expr = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: v_minus_i,
            right: q,
        },
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_q".to_string(), i);
    package
        .expr_spans
        .resize(package.exprs.len(), Span::default());
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Causalized"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: vec![Field {
            name: "V".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: Vec::new(),
        state: vec![Field {
            name: "q".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        algebraic: vec![Field {
            name: "I".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        constructors: Vec::new(),
        definitions,
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
    package.residuals.insert(
        DeclarationId(0),
        vec![emath_ir::ModelResidual {
            expr: residual_expr,
            components: 1,
            algebraic: vec!["I".to_string()],
            rates: Vec::new(),
        }],
    );
    let output = BackendInput {
        package: &package,
        crate_name: "causalized".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("causalized model must now codegen its causalized Newton solve");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("fn __emath_gaussian_solve") && lib.contains("fn __emath_max_abs"),
        "causalized codegen must embed the Newton helpers, got:\n{lib}"
    );
    assert!(
        lib.contains("fn step_euler(") && lib.contains("fn step_rk4("),
        "causalized model must emit both step methods, got:\n{lib}"
    );
    assert!(
        lib.contains("Result<Self, String>"),
        "causalized steps must surface non-convergence as a typed Result, got:\n{lib}"
    );
    assert!(
        lib.contains("for _ in 0..30u32") && lib.contains("__emath_max_abs(&__f) < 0.000000001"),
        "causalized steps must mirror the interpreter Newton budget and tolerance, got:\n{lib}"
    );
    assert!(
        lib.contains("_rates[") && lib.contains("x[0]"),
        "causalized stages must drive residual closures through the solve vector, got:\n{lib}"
    );
    assert!(
        lib.contains("__emath_gaussian_solve"),
        "causalized Jacobian solve must call the embedded Gaussian helper, got:\n{lib}"
    );
    assert!(
        lib.contains("I: f64") && lib.contains("q: f64"),
        "causalized struct must hold algebraic I with state q so a step can return a consistent DAE point, got:\n{lib}"
    );
    assert!(
        lib.contains("__proj_alg") && lib.contains("__advanced"),
        "causalized steps must re-solve algebraic unknowns at the accepted state, got:\n{lib}"
    );
    assert!(
        lib.contains("internal: Newton rate vector has the wrong width"),
        "causalized steps must refuse a wrong-width rate vector as Result, not panic, got:\n{lib}"
    );
}

#[test]
fn model_emits_explicit_step_methods() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.x")),
        Span::default(),
    );
    let rate = package.push_expr(
        ExprNode::Unary {
            operation: emath_ir::UnaryOp::Negate,
            value: x,
        },
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_x".to_string(), rate);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Decay"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
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
    let output = BackendInput {
        package: &package,
        crate_name: "decay".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("model must emit step methods");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    assert!(
        lib.contains("fn der_x(") && lib.contains("fn step_euler(") && lib.contains("fn step_rk4("),
        "model must emit der_x/step_euler/step_rk4, got:\n{lib}"
    );
}

fn eval_requirements() -> GoalRequirements {
    GoalRequirements {
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
    }
}

fn generate_fn(
    name: &str,
    inputs: &[&str],
    y_def: ExprId,
    package: &mut SemanticPackage,
) -> String {
    let ty = package.push_type(TypeNode::Float64);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single(name),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: inputs
            .iter()
            .map(|input| Field {
                name: (*input).to_string(),
                ty,
                visibility: Visibility::Public,
                source: Span::default(),
            })
            .collect(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
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
        package,
        crate_name: name.to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .unwrap_or_else(|err| panic!("{name} must generate: {err}"));
    output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs")
        .clone()
}

fn extract_fn(src: &str, name: &str) -> String {
    let marker = format!("pub fn {name}");
    match src.find(&marker) {
        Some(start) => src[start..].chars().take(800).collect(),
        None => panic!("missing `{marker}` in:\n{src}"),
    }
}

/// Single-use inlining must keep non-associative grouping: `a - (b - c)`
/// is not `a - b - c`, and `(a + b) * c` is not `a + b * c`.
#[test]
fn flatten_preserves_non_associative_grouping() {
    let mut nested_sub = SemanticPackage::new();
    let a = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let c = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("c")),
        Span::default(),
    );
    let inner = nested_sub.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: b,
            right: c,
        },
        Span::default(),
    );
    let y = nested_sub.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: a,
            right: inner,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("nested_sub", &["a", "b", "c"], y, &mut nested_sub),
        "nested_sub",
    );
    assert!(
        src.contains("(a) - ((b) - (c))") || src.contains("a - (b - c)"),
        "right-assoc subtraction must stay grouped, got:\n{src}"
    );

    let mut grouped_mul = SemanticPackage::new();
    let a = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let c = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("c")),
        Span::default(),
    );
    let sum = grouped_mul.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatAdd,
            left: a,
            right: b,
        },
        Span::default(),
    );
    let y = grouped_mul.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: sum,
            right: c,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("grouped_mul", &["a", "b", "c"], y, &mut grouped_mul),
        "grouped_mul",
    );
    assert!(
        src.contains("((a) + (b)) * (c)") || src.contains("(a + b) * c"),
        "add-then-mul must stay grouped, got:\n{src}"
    );
}

/// Flattening used to emit braceless `if cond then else`, which is not
/// valid Rust and dropped the Select from generated crates. Arms must be
/// blocks so the value matches eager SSA (taken arm) and the crate compiles.
#[test]
fn flatten_select_emits_blocked_if() {
    let mut abs_if = SemanticPackage::new();
    let a = abs_if.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let zero = abs_if.push_expr(
        ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())),
        Span::default(),
    );
    let cond = abs_if.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::Greater,
            left: a,
            right: zero,
        },
        Span::default(),
    );
    let neg = abs_if.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Negate,
            value: a,
        },
        Span::default(),
    );
    let y = abs_if.push_expr(
        ExprNode::If {
            condition: cond,
            then_value: a,
            else_value: neg,
        },
        Span::default(),
    );
    let src = extract_fn(&generate_fn("abs_if", &["a"], y, &mut abs_if), "abs_if");
    assert!(
        src.contains("if") && src.contains('{') && src.contains("else"),
        "Select must render as `if cond {{ t }} else {{ e }}`, got:\n{src}"
    );
    assert!(
        !src.contains(") (a)") && !src.contains(") a\n"),
        "braceless `if cond (a)` is invalid Rust, got:\n{src}"
    );
}

/// Method receivers must parenthesize inlined sums: `(a + b).sin()`, not
/// `a + b.sin()`.
#[test]
fn flatten_method_receiver_parenthesizes_sum() {
    let mut sin_add = SemanticPackage::new();
    let a = sin_add.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = sin_add.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let sum = sin_add.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatAdd,
            left: a,
            right: b,
        },
        Span::default(),
    );
    let y = sin_add.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Sin,
            value: sum,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("sin_add", &["a", "b"], y, &mut sin_add),
        "sin_add",
    );
    assert!(
        src.contains("((a) + (b)).sin()") || src.contains("(a + b).sin()"),
        "sin of a sum must parenthesize the receiver, got:\n{src}"
    );
}

fn generate_typed(
    name: &str,
    output_ty: TypeNode,
    y_def: ExprId,
    package: &mut SemanticPackage,
) -> String {
    let ty = package.push_type(output_ty);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single(name),
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
        algebraic: Vec::new(),
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
        package,
        crate_name: name.to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .unwrap_or_else(|err| panic!("{name} must generate: {err}"));
    output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs")
        .clone()
}

/// `2^53+1` cannot round-trip through f64. rust.library must emit an i64
/// literal, matching interp `Value::I64`, not `(value as f64)`.
#[test]
fn const_i64_past_f64_mantissa_stays_i64() {
    let mut package = SemanticPackage::new();
    let y = package.push_expr(
        ExprNode::Literal(Literal::Integer("9007199254740993".to_string())),
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("past_mantissa", TypeNode::Int, y, &mut package),
        "past_mantissa",
    );
    assert!(
        src.contains("9007199254740993i64") || src.contains("9007199254740993"),
        "ConstI64 past the f64 mantissa must stay i64, got:\n{src}"
    );
    assert!(
        !src.contains("9007199254740992") && !src.contains("9007199254740993.0"),
        "must not round 2^53+1 through f64, got:\n{src}"
    );
    assert!(
        src.contains("-> i64"),
        "Int output must return i64, got:\n{src}"
    );
}

/// Mixed Int/Float64 `==` used to widen through `as f64`, so this
/// constant pair folded to `true`. Exact compare folds to `false`.
#[test]
fn mixed_i64_f64_eq_folds_false_not_widened_true() {
    let mut package = SemanticPackage::new();
    let n = package.push_expr(
        ExprNode::Literal(Literal::Integer("9007199254740993".to_string())),
        Span::default(),
    );
    let x = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(((1i64 << 53) as f64).to_bits())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::Equal,
            left: n,
            right: x,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("mixed_eq", TypeNode::Bool, y, &mut package),
        "mixed_eq",
    );
    let body = src.split("\npub fn").next().unwrap_or(&src);
    assert!(
        body.contains("false"),
        "2^53+1 == 2^53.0 must fold to false, got:\n{body}"
    );
    assert!(
        !body.contains("true"),
        "widening as f64 would fold this pair to true, got:\n{body}"
    );
}

/// `factorial(20)` is exact i64 in interp and emath-rt; codegen must call
/// the i64 kernel and not cast the result to f64.
#[test]
fn factorial_twenty_calls_i64_kernel() {
    let mut package = SemanticPackage::new();
    let n = package.push_expr(
        ExprNode::Literal(Literal::Integer("20".to_string())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("factorial"),
            arguments: vec![n],
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("fact20", TypeNode::Int, y, &mut package),
        "fact20",
    );
    assert!(
        src.contains("emath_rt::factorial"),
        "factorial must call the i64 kernel, got:\n{src}"
    );
    assert!(
        !src.contains("as f64"),
        "factorial(20) must not be recast to f64, got:\n{src}"
    );
    assert!(
        src.contains("-> i64"),
        "factorial Int output must return i64, got:\n{src}"
    );
}

#[test]
fn rank3_stencil_and_tensor_scale_generate_shared_runtime_calls() {
    let mut package = SemanticPackage::new();
    let values: Vec<_> = (0..27)
        .map(|value| {
            package.push_expr(
                ExprNode::Literal(Literal::FloatBits((value as f64).to_bits())),
                Span::default(),
            )
        })
        .collect();
    let tensor = package.push_expr(
        ExprNode::Tensor {
            shape: vec![3, 3, 3],
            elements: values,
        },
        Span::default(),
    );
    let spacing = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())),
        Span::default(),
    );
    let laplacian = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("laplacian_3d"),
            arguments: vec![tensor, spacing],
        },
        Span::default(),
    );
    let scale = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(0.25f64.to_bits())),
        Span::default(),
    );
    let output = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::TensorScale,
            left: laplacian,
            right: scale,
        },
        Span::default(),
    );
    let source = generate_typed(
        "spatial3d",
        TypeNode::Tensor {
            element: Box::new(TypeNode::Float64),
            shape: vec![Extent::Fixed(3), Extent::Fixed(3), Extent::Fixed(3)],
        },
        output,
        &mut package,
    );
    assert!(source.contains("stencil_3d_checked"), "{source}");
    assert!(source.contains("tensor_scale"), "{source}");
}

/// `einsum("ik,kj->ij", A, B)` must call the emath-rt kernel, not emit
/// `panic!("einsum ... not yet implemented")`.
#[test]
fn einsum_codegen_calls_rt_kernel_not_panic_stub() {
    let mut package = SemanticPackage::new();
    let f = |p: &mut SemanticPackage, v: f64| {
        p.push_expr(
            ExprNode::Literal(Literal::FloatBits(v.to_bits())),
            Span::default(),
        )
    };
    let a11 = f(&mut package, 1.0);
    let a12 = f(&mut package, 2.0);
    let a21 = f(&mut package, 3.0);
    let a22 = f(&mut package, 4.0);
    let b11 = f(&mut package, 5.0);
    let b12 = f(&mut package, 6.0);
    let b21 = f(&mut package, 7.0);
    let b22 = f(&mut package, 8.0);
    let a = package.push_expr(
        ExprNode::Matrix(vec![vec![a11, a12], vec![a21, a22]]),
        Span::default(),
    );
    let b = package.push_expr(
        ExprNode::Matrix(vec![vec![b11, b12], vec![b21, b22]]),
        Span::default(),
    );
    let sub = package.push_expr(
        ExprNode::Literal(Literal::Text("ik,kj->ij".to_string())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("einsum"),
            arguments: vec![sub, a, b],
        },
        Span::default(),
    );
    let src = generate_typed(
        "ein_mm",
        TypeNode::Matrix {
            element: Box::new(TypeNode::Float64),
            rows: None,
            cols: None,
        },
        y,
        &mut package,
    );
    assert!(
        !src.contains("not yet implemented"),
        "einsum must not emit a panic stub, got:\n{src}"
    );
    assert!(
        src.contains("einsum_as_matrix") && src.contains("EinsumIn::einsum_operand"),
        "einsum must call the rt kernel, got:\n{src}"
    );
}

/// rust.library used to emit panicking `v[i as usize]`. OOB is a typed
/// `Result` via `vec_index_checked`.
#[test]
fn vector_index_codegen_uses_checked_helper_not_index() {
    let mut package = SemanticPackage::new();
    let v_ty = package.push_type(TypeNode::Vector {
        element: Box::new(TypeNode::Float64),
        extent: None,
    });
    let f_ty = package.push_type(TypeNode::Float64);
    let v = package.push_expr(
        ExprNode::Variable(QualifiedName::single("v")),
        Span::default(),
    );
    let i = package.push_expr(
        ExprNode::Variable(QualifiedName::single("i")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Index {
            value: v,
            indices: vec![i],
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("idx"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![
            Field {
                name: "v".to_string(),
                ty: v_ty,
                visibility: Visibility::Public,
                source: Span::default(),
            },
            Field {
                name: "i".to_string(),
                ty: f_ty,
                visibility: Visibility::Public,
                source: Span::default(),
            },
        ],
        outputs: vec![Field {
            name: "y".to_string(),
            ty: f_ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
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
    let src = BackendInput {
        package: &package,
        crate_name: "idx".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("index must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone();
    let fn_src = extract_fn(&src, "idx");
    assert!(
        fn_src.contains("vec_index_checked"),
        "vector index must call the checked helper, got:\n{fn_src}"
    );
    assert!(
        !fn_src.contains("as usize") && !fn_src.contains("]["),
        "must not emit panicking [], got:\n{fn_src}"
    );
    assert!(
        fn_src.contains("Result<") && fn_src.contains("String"),
        "index evaluate must return Result, got:\n{fn_src}"
    );
}

/// `t[0, :, :]` used to clone the whole tensor. It must emit the slice
/// kernel and produce a matrix (tensor-face.emath identity).
#[test]
fn tensor_face_slice_codegen_is_not_a_clone() {
    let mut package = SemanticPackage::new();
    let f = |p: &mut SemanticPackage, v: f64| {
        p.push_expr(
            ExprNode::Literal(Literal::FloatBits(v.to_bits())),
            Span::default(),
        )
    };
    let elems: Vec<_> = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        .into_iter()
        .map(|v| f(&mut package, v))
        .collect();
    let t = package.push_expr(
        ExprNode::Tensor {
            shape: vec![2, 2, 2],
            elements: elems,
        },
        Span::default(),
    );
    let zero = f(&mut package, 0.0);
    let two = f(&mut package, 2.0);
    let y = package.push_expr(
        ExprNode::Slice {
            value: t,
            axes: vec![
                SliceAxis::Point(zero),
                SliceAxis::Range {
                    start: zero,
                    end: two,
                },
                SliceAxis::Range {
                    start: zero,
                    end: two,
                },
            ],
        },
        Span::default(),
    );
    let src = generate_typed(
        "tensor_face",
        TypeNode::Matrix {
            element: Box::new(TypeNode::Float64),
            rows: None,
            cols: None,
        },
        y,
        &mut package,
    );
    let fn_src = extract_fn(&src, "tensor_face");
    assert!(
        !fn_src.contains("tensor slice axes") && !fn_src.contains("t.clone()"),
        "tensor slice must not be a no-op clone, got:\n{fn_src}"
    );
    assert!(
        fn_src.contains("tensor_slice_as_matrix") && fn_src.contains("SliceAxis"),
        "t[0, :, :] must call the slice kernel, got:\n{fn_src}"
    );
    assert!(
        fn_src.contains("emath_rt::Tensor") && fn_src.contains("shape: vec![2, 2, 2]"),
        "rank-3 literal must keep shape, got:\n{fn_src}"
    );
}

/// `product i in 1..=20: i` stays on the exact i64 fold (interp
/// `Value::I64(20!)`), not f64 `fold_mul`.
#[test]
fn integer_product_fold_uses_i64_kernel() {
    let mut package = SemanticPackage::new();
    let start = package.push_expr(
        ExprNode::Literal(Literal::Integer("1".to_string())),
        Span::default(),
    );
    // EMIR fold is half-open; inclusive 1..=20 is the vector [1, 21].
    let end = package.push_expr(
        ExprNode::Literal(Literal::Integer("21".to_string())),
        Span::default(),
    );
    let domain = package.push_expr(ExprNode::Vector(vec![start, end]), Span::default());
    let body = package.push_expr(
        ExprNode::Variable(QualifiedName::single("i")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Binder {
            kind: BinderKind::Product,
            variables: vec![BinderVariable {
                name: "i".to_string(),
                domain,
            }],
            body,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("prod20", TypeNode::Int, y, &mut package),
        "prod20",
    );
    assert!(
        src.contains("fold_mul_i64"),
        "integer product must use fold_mul_i64, got:\n{src}"
    );
    assert!(
        !src.contains("fold_mul(") || src.contains("fold_mul_i64"),
        "integer product must not use the f64 fold_mul kernel, got:\n{src}"
    );
}

/// Folded IEEE non-finite constants (`sqrt(-1)` → NaN, `1/0` → Inf) must
/// render as valid Rust, not Debug `NaN`/`inf` identifiers.
#[test]
fn folded_nonfinite_constants_emit_valid_rust() {
    let mut sqrt_neg = SemanticPackage::new();
    let neg1 = sqrt_neg.push_expr(
        ExprNode::Literal(Literal::Integer("-1".to_string())),
        Span::default(),
    );
    let y = sqrt_neg.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Sqrt,
            value: neg1,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("sqrt_neg", TypeNode::Float64, y, &mut sqrt_neg),
        "sqrt_neg",
    );
    assert!(
        src.contains("f64::from_bits("),
        "sqrt(-1) must emit from_bits, not Debug NaN, got:\n{src}"
    );
    assert!(
        !src.contains("NaN"),
        "bare `NaN` is not valid Rust, got:\n{src}"
    );

    let mut div0 = SemanticPackage::new();
    let one = div0.push_expr(
        ExprNode::Literal(Literal::Integer("1".to_string())),
        Span::default(),
    );
    let zero = div0.push_expr(
        ExprNode::Literal(Literal::Integer("0".to_string())),
        Span::default(),
    );
    let y = div0.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatDiv,
            left: one,
            right: zero,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("div0", TypeNode::Float64, y, &mut div0),
        "div0",
    );
    assert!(
        src.contains("f64::from_bits("),
        "1/0 must emit from_bits, not Debug inf, got:\n{src}"
    );
    assert!(
        !src.contains("inf") && !src.contains("Inf"),
        "bare `inf` is not valid Rust, got:\n{src}"
    );

    let mut log0 = SemanticPackage::new();
    let zero = log0.push_expr(
        ExprNode::Literal(Literal::Integer("0".to_string())),
        Span::default(),
    );
    let y = log0.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Log,
            value: zero,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("log0", TypeNode::Float64, y, &mut log0),
        "log0",
    );
    assert!(
        src.contains("f64::from_bits("),
        "log(0) must emit from_bits, not Debug -inf, got:\n{src}"
    );
}

/// `sign(0)` is mathematical 0, not IEEE `signum` (±1 at ±0). rust.library
/// must emit the same zero-check the interp builtin uses.
#[test]
fn sign_zero_uses_mathematical_sgn() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("sign"),
            arguments: vec![x],
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("sgn"),
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
        algebraic: Vec::new(),
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
        crate_name: "sgn".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("sign must generate");
    let src = extract_fn(
        output
            .files
            .get("src/lib.rs")
            .expect("generated crate has src/lib.rs"),
        "sgn",
    );
    assert!(
        src.contains("== 0.0") && src.contains("signum"),
        "sign must use mathematical sgn (0 at 0), got:\n{src}"
    );
}
