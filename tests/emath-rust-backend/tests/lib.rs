use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use emath_core::{QualifiedName, Span};
use emath_exec_ir::{EmirOp, EmirProgram, EmirValue};
use emath_ir::{
    BinaryOp, BinderKind, BinderVariable, CompileSpec, Constructor, Declaration, DeclarationId,
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExprId, ExprNode, Extent, FallbackPolicy,
    Field, Goal, GoalId, GoalKind, GoalPayload, GoalRequirements, Literal, ObligationClass,
    ObligationKind, SemanticPackage, SliceAxis, TargetProfile, TestCase, TypeId, TypeNode, UnaryOp,
    Visibility,
};
use emath_rust_backend::{BackendInput, render_op_expr_for_tests};
use emath_rust_backend::rust_ir::ast::{Item, StructDef};
use emath_rust_backend::rust_ir::render::{render_expr, render_module};

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

#[test]
fn sequence_recurrence_generates_shared_runtime_call() {
    let mut package = SemanticPackage::new();
    let zero = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())),
        Span::default(),
    );
    let one = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
        Span::default(),
    );
    let initial = package.push_expr(ExprNode::Vector(vec![zero, one]), Span::default());
    let recurrence = package.push_expr(ExprNode::Vector(vec![one, one]), Span::default());
    let budget = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(64.0_f64.to_bits())),
        Span::default(),
    );
    let sequence = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("generating_function"),
            arguments: vec![initial, recurrence, budget],
        },
        Span::default(),
    );
    let index = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(10.0_f64.to_bits())),
        Span::default(),
    );
    let coefficient = package.push_expr(
        ExprNode::Index {
            value: sequence,
            indices: vec![index],
        },
        Span::default(),
    );
    let source = generate_fn("sequence_coefficient", &[], coefficient, &mut package);
    assert!(
        source.contains("emath_rt::sequence_generate"),
        "generated Rust must use the shared recurrence kernel:\n{source}"
    );
}

#[test]
fn special_function_generates_embedded_reference_evaluator() {
    let mut package = SemanticPackage::new();
    let five = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(5.0_f64.to_bits())),
        Span::default(),
    );
    let gamma = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("gamma"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let source = generate_fn("gamma_five", &[], gamma, &mut package);
    assert!(
        source.contains("emath_rt::special::SpecialFn::Gamma")
            && source.contains("evaluate_strict"),
        "generated Rust must embed and invoke the strict evaluator:\n{source}"
    );
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

fn op_program(ops: Vec<(EmirOp, Span)>, result: EmirValue) -> EmirProgram {
    EmirProgram {
        ops,
        result,
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// `IntNullspace` lowers through the same exact-integer kernel as
/// the interpreter, with both typed refusal paths and no domain
/// naming.
#[test]
fn int_nullspace_lowers_to_exact_kernel() {
    let span = Span::default();
    let ops = vec![
        (EmirOp::LoadInput(0), span),
        (EmirOp::LoadInput(1), span),
        (EmirOp::IntNullspace(EmirValue(1)), span),
    ];
    let program = op_program(ops, EmirValue(2));
    let expr = render_op_expr_for_tests(
        &EmirOp::IntNullspace(EmirValue(1)),
        &program,
        &[],
        &[],
        &BTreeSet::new(),
    )
    .expect("matrix-operand IntNullspace must lower");
    let rendered = render_expr(&expr);
    assert!(
        rendered.contains("primitive_int_nullvector"),
        "IntNullspace must call the exact-integer kernel, got:\n{rendered}"
    );
    assert!(
        rendered.contains("E-NULLSPACE-001") && rendered.contains("E-NULLSPACE-002"),
        "IntNullspace must emit both typed refusals, got:\n{rendered}"
    );
    assert!(
        rendered.contains("as i64"),
        "IntNullspace must widen exact integer entries, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("chem"),
        "IntNullspace codegen must carry no domain naming, got:\n{rendered}"
    );
}

/// `ExactProductDelta` compares the products in u128 before any f64
/// cast, refuses typed on entry/length/overflow, and widens the
/// exact magnitude with the sign of `p - q`.
#[test]
fn exact_product_delta_compares_u128_before_cast() {
    let span = Span::default();
    let ops = vec![
        (EmirOp::LoadInput(0), span),
        (EmirOp::LoadInput(1), span),
        (EmirOp::LoadInput(2), span),
        (EmirOp::ExactProductDelta(EmirValue(1), EmirValue(2)), span),
    ];
    let program = op_program(ops, EmirValue(3));
    let expr = render_op_expr_for_tests(
        &EmirOp::ExactProductDelta(EmirValue(1), EmirValue(2)),
        &program,
        &[],
        &[],
        &BTreeSet::new(),
    )
    .expect("vector-operand ExactProductDelta must lower");
    let rendered = render_expr(&expr);
    assert!(
        rendered.contains("__pp == __qq"),
        "ExactProductDelta must compare products in u128 before any cast, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("as f64 - __qq as f64") && !rendered.contains("__pp as f64"),
        "ExactProductDelta must not compare via lossy f64 casts, got:\n{rendered}"
    );
    assert!(
        rendered.contains("checked_mul")
            && rendered.contains("E-EXACT-001")
            && rendered.contains("E-EXACT-002"),
        "ExactProductDelta must carry exact u128 overflow and entry refusals, got:\n{rendered}"
    );
    assert!(
        rendered.contains("(__big - __small) as f64"),
        "ExactProductDelta must widen the exact magnitude, got:\n{rendered}"
    );
}

/// A statically non-matrix operand is a typed lowering refusal,
/// never a fabricated vector (interp TypeConfusion parity).
#[test]
fn int_nullspace_non_matrix_operand_refuses_typed() {
    let span = Span::default();
    let ops = vec![
        (EmirOp::ConstF64(1.0f64.to_bits()), span),
        (EmirOp::IntNullspace(EmirValue(0)), span),
    ];
    let program = op_program(ops, EmirValue(1));
    let err = render_op_expr_for_tests(
        &EmirOp::IntNullspace(EmirValue(0)),
        &program,
        &[],
        &[],
        &BTreeSet::new(),
    )
    .expect_err("scalar-operand IntNullspace must refuse");
    assert!(
        err.to_string().contains("matrix operand"),
        "refusal must name the shape rule, got: {err}"
    );
}

// ── aj8d pass 5: strict Rust backend Option/Result parity ─────────────
// The nine carrier ops must lower through the REAL generation path
// (SemanticPackage → emitter → EmirProgram → BackendInput::generate →
// rust-ir) into executable native Option<T>/Result<T, E> code with typed
// shape errors. Behavior is proven by compiling the generated crate and
// RUNNING it (rustc-direct, no new cargo crate; test-only infra).

fn carrier_int(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(name, TypeNode::Int, y_expr, package);
}

fn carrier_bool(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(name, TypeNode::Bool, y_expr, package);
}

fn carrier_vec(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(
        name,
        TypeNode::Vector {
            element: Box::new(TypeNode::Float64),
            extent: None,
        },
        y_expr,
        package,
    );
}

fn carrier_decl(name: &str, output_ty: TypeNode, y_expr: ExprId, package: &mut SemanticPackage) {
    let ty = package.push_type(output_ty);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_expr),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_expr);
    package.declarations.push(Declaration {
        id: DeclarationId(package.declarations.len() as u32),
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
}

/// Real-path generation of the test package → generated src/lib.rs.
fn generate_carrier_lib(package: &SemanticPackage) -> String {
    BackendInput {
        package,
        crate_name: "opt_result_carrier".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("option/result carrier package must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone()
}

/// Generated user code (everything before the embedded `emath_rt` module).
fn user_section(lib: &str) -> &str {
    lib.split("mod emath_rt").next().unwrap_or(lib)
}

/// Compile the generated crate and run `main_body` against it with
/// rustc-direct (edition 2024, std-only generated crate; the embedded
/// emath_rt kernel source is pure std). Behavior failures surface as
/// non-zero exit (assert!) with the message in stderr.
fn run_generated(lib: &str, main_body: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "emath_opt_carrier_run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdtemp: {e}"))?;
    let result = run_generated_in(&dir, lib, main_body);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn run_generated_in(dir: &std::path::Path, lib: &str, main_body: &str) -> Result<(), String> {
    let lib_path = dir.join("generated.rs");
    std::fs::write(&lib_path, lib).map_err(|e| format!("write lib: {e}"))?;
    let driver = format!(
        "#[path = \"{}\"]\nmod generated;\nfn main() {{\n{main_body}\n}}\n",
        lib_path.display()
    );
    let main_path = dir.join("main.rs");
    let bin_path = dir.join("run");
    std::fs::write(&main_path, driver).map_err(|e| format!("write driver: {e}"))?;
    let comp = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-name")
        .arg("opt_carrier_run")
        .current_dir(dir)
        .arg(&main_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .map_err(|e| format!("rustc spawn failed (is rustc on PATH?): {e}"))?;
    if !comp.status.success() {
        return Err(format!(
            "generated crate failed to compile:\n{}\n--- generated lib ---\n{lib}",
            String::from_utf8_lossy(&comp.stderr)
        ));
    }
    let run = Command::new(&bin_path)
        .output()
        .map_err(|e| format!("run generated binary: {e}"))?;
    if !run.status.success() {
        return Err(format!(
            "generated behavior failed (exit {:?}):\n{}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(())
}

/// some(some carrier) round trips its payload; none returns the eager
/// default; is_some polarity holds — all in EXECUTED generated Rust.
#[test]
fn option_carrier_generated_behaviors() {
    let mut package = SemanticPackage::new();
    let five = package.push_expr(
        ExprNode::Literal(Literal::Integer("5".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let some = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let roundtrip = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![some, seven],
        },
        Span::default(),
    );
    carrier_int("opt_roundtrip", roundtrip, &mut package);
    let none = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_none"),
            arguments: Vec::new(),
        },
        Span::default(),
    );
    let default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![none, seven],
        },
        Span::default(),
    );
    carrier_int("opt_default", default, &mut package);
    let some2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let pol_some = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![some2],
        },
        Span::default(),
    );
    carrier_bool("opt_polarity_some", pol_some, &mut package);
    let none2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_none"),
            arguments: Vec::new(),
        },
        Span::default(),
    );
    let pol_none = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![none2],
        },
        Span::default(),
    );
    carrier_bool("opt_polarity_none", pol_none, &mut package);
    let v1 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
        Span::default(),
    );
    let v2 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(2.0_f64.to_bits())),
        Span::default(),
    );
    let vec_payload = package.push_expr(ExprNode::Vector(vec![v1, v2]), Span::default());
    let some_vec = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![vec_payload],
        },
        Span::default(),
    );
    let v9 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(9.0_f64.to_bits())),
        Span::default(),
    );
    let v8 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(8.0_f64.to_bits())),
        Span::default(),
    );
    let vec_default = package.push_expr(ExprNode::Vector(vec![v9, v8]), Span::default());
    let vec_pick = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![some_vec, vec_default],
        },
        Span::default(),
    );
    carrier_vec("vec_pick", vec_pick, &mut package);

    let lib = generate_carrier_lib(&package);
    let user = user_section(&lib);
    assert!(
        lib.contains("Option::<i64>::Some") && lib.contains("Option::<i64>::None"),
        "generated carriers must be native Option<i64>, got:\n{lib}"
    );
    assert!(
        lib.contains("Option::<Vec<f64>>::Some") && lib.contains(".unwrap_or("),
        "vector payload and eager unwrap_or gate must be emitted, got:\n{lib}"
    );
    assert!(
        !user.contains(".unwrap()") && !user.contains("expect(") && !user.contains("panic!"),
        "generated user code must contain no panicking unwrap / expect / panic, got:\n{user}"
    );
    run_generated(&lib, r#"
        assert_eq!(generated::opt_roundtrip(), 5i64, "some carrier round trip");
        assert_eq!(generated::opt_default(), 7i64, "none returns eager default");
        assert_eq!(generated::opt_polarity_some(), true, "option_is_some(some) is true");
        assert_eq!(generated::opt_polarity_none(), false, "option_is_some(none) is false");
        assert_eq!(generated::vec_pick(), vec![1.0_f64, 2.0_f64], "vector payload round trip");
    "#)
    .expect("generated option carrier behavior must hold at runtime");
}

/// ok/err carry their payloads, is_ok polarity holds, unwrap_or's
/// default is taken only on Err, and error_of composes the error as an
/// Option (Ok → None, Err → Some(payload)) — in EXECUTED generated Rust.
#[test]
fn result_carrier_generated_behaviors() {
    let mut package = SemanticPackage::new();
    let five = package.push_expr(
        ExprNode::Literal(Literal::Integer("5".to_string())),
        Span::default(),
    );
    let six = package.push_expr(
        ExprNode::Literal(Literal::Integer("6".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let nine = package.push_expr(
        ExprNode::Literal(Literal::Integer("9".to_string())),
        Span::default(),
    );
    let ok = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let ok_carry = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or"),
            arguments: vec![ok, seven],
        },
        Span::default(),
    );
    carrier_int("res_ok_carry", ok_carry, &mut package);
    let err = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let err_default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or"),
            arguments: vec![err, seven],
        },
        Span::default(),
    );
    carrier_int("res_err_default", err_default, &mut package);
    let ok2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let is_ok_true = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_is_ok"),
            arguments: vec![ok2],
        },
        Span::default(),
    );
    carrier_bool("res_is_ok_true", is_ok_true, &mut package);
    let err2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let is_ok_false = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_is_ok"),
            arguments: vec![err2],
        },
        Span::default(),
    );
    carrier_bool("res_is_ok_false", is_ok_false, &mut package);
    let ok3 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let err_of_ok = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![ok3],
        },
        Span::default(),
    );
    let err_of_ok_default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![err_of_ok, nine],
        },
        Span::default(),
    );
    carrier_int("err_of_ok", err_of_ok_default, &mut package);
    let err3 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let err_of_err = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![err3],
        },
        Span::default(),
    );
    let err_of_err_payload = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![err_of_err, nine],
        },
        Span::default(),
    );
    carrier_int("err_of_err", err_of_err_payload, &mut package);

    let lib = generate_carrier_lib(&package);
    let user = user_section(&lib);
    assert!(
        lib.contains("Result::<i64, i64>::Ok") && lib.contains("Result::<i64, i64>::Err"),
        "generated carriers must be native Result<i64, i64>, got:\n{lib}"
    );
    assert!(
        lib.contains("match ") && lib.contains("Option::<i64>::Some"),
        "error_of must compose the error as an Option carrier, got:\n{lib}"
    );
    assert!(
        !user.contains(".unwrap()") && !user.contains("expect(") && !user.contains("panic!"),
        "generated user code must contain no panicking unwrap / expect / panic, got:\n{user}"
    );
    run_generated(&lib, r#"
        assert_eq!(generated::res_ok_carry(), 5i64, "ok payload round trips");
        assert_eq!(generated::res_err_default(), 7i64, "err payload is not the unwrap value; default is");
        assert_eq!(generated::res_is_ok_true(), true, "result_is_ok(ok) is true");
        assert_eq!(generated::res_is_ok_false(), false, "result_is_ok(err) is false");
        assert_eq!(generated::err_of_ok(), 9i64, "error_of(ok) -> None -> default");
        assert_eq!(generated::err_of_err(), 6i64, "error_of(err) -> Some(payload)");
    "#)
    .expect("generated result carrier behavior must hold at runtime");
}

/// Wrong-carrier use is a TYPED lowering refusal (interp TypeConfusion
/// parity), surfaced as `BackendError::Lowering`, never a panic and
/// never a silent scalar shadow.
#[test]
fn wrong_carrier_use_refuses_typed() {
    // option_is_some over a scalar carrier slot.
    let mut package = SemanticPackage::new();
    let scalar = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(5.0_f64.to_bits())),
        Span::default(),
    );
    let bogus = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![scalar],
        },
        Span::default(),
    );
    carrier_bool("bogus_opt", bogus, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "bogus".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("option_is_some over a scalar must refuse typed");
    let msg = err.to_string();
    assert!(
        msg.contains("Option carrier") && msg.contains("TypeConfusion"),
        "refusal must name the carrier rule and mirror interp TypeConfusion, got: {msg}"
    );

    // result_error_of over a scalar carrier slot.
    let mut package = SemanticPackage::new();
    let scalar = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(5.0_f64.to_bits())),
        Span::default(),
    );
    let bogus = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![scalar],
        },
        Span::default(),
    );
    carrier_bool("bogus_res", bogus, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "bogus_res".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("result_error_of over a scalar must refuse typed");
    let msg = err.to_string();
    assert!(
        msg.contains("Result carrier") && msg.contains("TypeConfusion"),
        "refusal must name the carrier rule and mirror interp TypeConfusion, got: {msg}"
    );
}

// ── aj8d pass 8: hardened backend typed refusals ────────────────────

fn literal_int(p: &mut SemanticPackage, s: &str) -> ExprId {
    p.push_expr(
        ExprNode::Literal(Literal::Integer(s.to_string())),
        Span::default(),
    )
}

/// A carrier passed into the eager-default slot of an `unwrap_or` is a
/// typed payload-kind conflict, never a silent scalar shadow of the
/// default. (Backend `CarrierPayloadTypes` resolves i64 from the carrier
/// producer vs f64 from the carrier default on the SAME register → REFUSE.)
#[test]
fn unwrap_or_carrier_default_refuses_payload_conflict() {
    // Option: option_unwrap_or(option_some(5), option_some(7)) — the
    // default slot is a genuine scalar, not a carrier.
    let mut package = SemanticPackage::new();
    let five = literal_int(&mut package, "5");
    let seven = literal_int(&mut package, "7");
    let c = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some".to_string()),
            arguments: vec![five],
        },
        Span::default(),
    );
    let def_car = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some".to_string()),
            arguments: vec![seven],
        },
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or".to_string()),
            arguments: vec![c, def_car],
        },
        Span::default(),
    );
    carrier_int("opt_carrier_default", y, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "opt_carrier_default".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("unwrap_or carrier default must refuse typed");
    let msg = err.to_string();
    assert!(
        msg.contains("payload kind conflict")
            && msg.contains("i64")
            && msg.contains("Option"),
        "Option carrier-as-default must refuse via payload-kind conflict, got: {msg}"
    );

    // Result: result_unwrap_or(result_ok(5), result_ok(7)) — same rule.
    let mut rpackage = SemanticPackage::new();
    let rfive = literal_int(&mut rpackage, "5");
    let rseven = literal_int(&mut rpackage, "7");
    let rc = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok".to_string()),
            arguments: vec![rfive],
        },
        Span::default(),
    );
    let rdef_car = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok".to_string()),
            arguments: vec![rseven],
        },
        Span::default(),
    );
    let ry = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or".to_string()),
            arguments: vec![rc, rdef_car],
        },
        Span::default(),
    );
    carrier_int("res_carrier_default", ry, &mut rpackage);
    let rerr = BackendInput {
        package: &rpackage,
        crate_name: "res_carrier_default".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("result unwrap_or carrier default must refuse typed");
    let rmsg = rerr.to_string();
    assert!(
        rmsg.contains("payload kind conflict")
            && rmsg.contains("i64")
            && rmsg.contains("Result"),
        "Result carrier-as-default must refuse via payload-kind conflict, got: {rmsg}"
    );
}

/// A `Field<7>` output field is not representable as a built-in Phase 1
/// Rust type; rust.library refuses it typed (naming the prime-field
/// spelling), never treating it as f64. (Sema ADMITS Field<7>; the
/// backend is the enforcement boundary for field-FIELD types.)
#[test]
fn field_prime_output_decl_refuses_typed() {
    let mut package = SemanticPackage::new();
    let y = literal_int(&mut package, "7");
    carrier_decl("field_out", TypeNode::FieldPrime { modulus: 7 }, y, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "field_out".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("Field<7> output field must refuse typed, not become f64");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("field<7>") && msg.contains("unsupported type"),
        "FieldPrime output must refuse naming Field<7> as an unsupported type, got: {err}"
    );
}

/// A `Field<7>` input field refuses typed for the same reason a scalar
/// Step is admitted — the field spelling is not a native Phase 1 scalar.
#[test]
fn field_prime_input_decl_refuses_typed() {
    let mut package = SemanticPackage::new();
    let ft = package.push_type(TypeNode::FieldPrime { modulus: 7 });
    let fty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x".to_string())),
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "x".to_string(),
        expression: Some(x),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("x".to_string(), x);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("field_in"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty: ft,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "x".to_string(),
            ty: fty,
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
    let err = BackendInput {
        package: &package,
        crate_name: "field_in".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("Field<7> input field must refuse typed");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("field<7>") && msg.contains("unsupported type"),
        "FieldPrime input must refuse naming Field<7> as an unsupported type, got: {err}"
    );
}

/// A `Option<Int>` / `Result<Int, Bool>` input FIELD also refuses typed
/// at the backend (Phase 1 has no native carrier RUST type on decl
/// fields; carrier VALUES only exist in expressions via the nine ops,
/// confirmed in pass 5). This pins the boundary: no field of a carrier
/// or field spelling can silently lower to f64.
#[test]
fn carrier_field_decl_refuses_typed() {
    for (spelling, node) in [
        (
            "Option<Int>",
            TypeNode::OptionType(Box::new(TypeNode::Int)),
        ),
        (
            "Result<Int, Bool>",
            TypeNode::Result {
                ok: Box::new(TypeNode::Int),
                error: Box::new(TypeNode::Bool),
            },
        ),
    ] {
        let mut package = SemanticPackage::new();
        let ft = package.push_type(node);
        let fty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x".to_string())),
            Span::default(),
        );
        let goal_id = package.push_goal(Goal {
            id: GoalId(0),
            kind: GoalKind::Evaluate,
            target: "x".to_string(),
            expression: Some(x),
            requirements: eval_requirements(),
            payload: GoalPayload::default(),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("x".to_string(), x);
        package.declarations.push(Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("carrier_field"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![Field {
                name: "x".to_string(),
                ty: ft,
                visibility: Visibility::Public,
                source: Span::default(),
            }],
            outputs: vec![Field {
                name: "x".to_string(),
                ty: fty,
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
        let err = BackendInput {
            package: &package,
            crate_name: "carrier_field".to_string(),
            version: "0.1.0".to_string(),
        }
        .generate()
        .expect_err("carrier input field must refuse typed");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains(&spelling.to_lowercase()) && msg.contains("unsupported type"),
            "`{spelling}` input field must refuse naming its spelling, got: {err}"
        );
    }
}

// ── aj8d pass 4: TEXT-driven carrier parity + nested carrier parity ───
// Build the SemanticPackage by parsing REAL .emath source (sema →
// package), then generate + execute the native Option/Result Rust. These
// close the loop the hand-built pass-5 tests start: the USER surface's
// executable carriers carry through to generated native types, and
// nested carriers now resolve to native nested types (Option<Option<T>>)
// instead of collapsing the inner payload to f64.

fn text_package(source: &str) -> SemanticPackage {
    emath_syntax::install_source_parser();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let file = session.load_text("aj8d-backend-text.emath", source);
    let planned = session.plan(file);
    let errors: Vec<String> = planned
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect();
    assert!(errors.is_empty(), "text must admit: {errors:?}\n{source}");
    planned.package
}

/// Generate src/lib.rs from real .emath text (sema → package → backend).
fn text_lib(source: &str) -> String {
    let package = text_package(source);
    BackendInput {
        package: &package,
        crate_name: "opt_result_carrier".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("text package must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone()
}

/// A declared `Option<Int>` OUTPUT whose definition lifts its input into
/// option_some: the generated free function carries the native Option<i64>
/// OUT of the declaration, and the runtime value is Some(5).
#[test]
fn text_option_int_output_carrier_out() {
    let lib = text_lib(
        "emath function f:\n    inputs:\n        k: Int\n    outputs:\n        o: Option<Int>\n    definitions:\n        o = option_some(k)\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    assert!(
        lib.contains("Option::<i64>::Some"),
        "generated code must carry Option<i64> natively, got:\n{lib}"
    );
    run_generated(&lib, "assert_eq!(generated::f(5), Some(5i64));")
        .expect("generated Option<Int> output must equal Some(5) at runtime");
}

/// NESTED carrier: option_some(option_some(k)) typed as
/// Option<Option<Int>> emits the native nested type and round-trips to
/// Some(Some(5)).
#[test]
fn text_nested_option_option_int() {
    let lib = text_lib(
        "emath function n:\n    inputs:\n        k: Int\n    outputs:\n        o: Option<Option<Int>>\n    definitions:\n        o = option_some(option_some(k))\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    assert!(
        lib.contains("Option::<Option<i64>>::Some"),
        "nested carrier must emit Option<Option<i64>>, got:\n{lib}"
    );
    run_generated(&lib, "assert_eq!(generated::n(5), Some(Some(5i64)));")
        .expect("nested Option<Option<i64>> must equal Some(Some(5)) at runtime");
}

/// Nested Some(None): outer Some carries an inner None (the tag-vs-content
/// distinction is preserved through the nested native type). The declared
/// input names the I/O surface (L3 E-SEC-130) while the nested carrier
/// stays constant.
#[test]
fn text_nested_some_none() {
    let lib = text_lib(
        "emath function sc:\n    inputs:\n        x: Float64\n    outputs:\n        o: Option<Option<Float64>>\n    definitions:\n        o = option_some(option_none())\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    run_generated(&lib, "assert_eq!(generated::sc(0.0), Some::<Option<f64>>(None));")
        .expect("nested Some(None) must hold at runtime");
}

/// map-by-declared-composition over a scalar input: the generated Rust
/// shows a native Option produced by the composed if/else (no carrier
/// input field — inputs stay scalar, the carrier is the output).
#[test]
fn text_map_by_composition_emits_native_option() {
    let lib = text_lib(
        "emath function m:\n    inputs:\n        x: Float64\n    outputs:\n        o: Option<Float64>\n    definitions:\n        o = if x > 0.0 : option_some(2.0 * x) else : option_none()\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    assert!(
        lib.contains("Option::<f64>::Some") && lib.contains("None"),
        "map-by-composition must emit native Option with a None arm, got:\n{lib}"
    );
    run_generated(
        &lib,
        "assert_eq!(generated::m(3.0), Some(6.0));\nassert_eq!(generated::m(-1.0), None);",
    )
    .expect("map-by-composition must yield Some(6) for positive, None for negative");
}

// ── aj8d pass 6: int_rem exact-Euclidean remainder; field ops as data ──
// Field +/*/inverse are user capability-cell DATA over the universal
// int_rem primitive. Generated Rust must emit exact `.rem_euclid(` and the
// runtime values must match the interpreter (field7_add(3,4)=0,
// field7_mul(3,4)=5, int_rem(-1,7)=6 — the negative case the truncated `%`
// mutant must kill).

/// field7_add: `c = int_rem(a + b, 7)` with an Int output and an evaluate
/// goal — the generated free fn computes the exact Field<7> sum.
#[test]
fn text_int_rem_field_add_executes() {
    let src = "emath function field7_add:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a + b, 7)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(src);
    assert!(
        lib.contains(".rem_euclid("),
        "int_rem must emit exact Rust `.rem_euclid(`, got:\n{lib}"
    );
    run_generated(
        &lib,
        "assert_eq!(generated::field7_add(3, 4), 0);\nassert_eq!(generated::field7_add(6, 5), 4);",
    )
    .expect("generated field7_add must compute exact Field<7> sums");
}

/// field7_mul: `c = int_rem(a * b, 7)`. Generated Rust matches.
#[test]
fn text_int_rem_field_mul_executes() {
    let src = "emath function field7_mul:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a * b, 7)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(src);
    run_generated(
        &lib,
        "assert_eq!(generated::field7_mul(3, 4), 5);\nassert_eq!(generated::field7_mul(3, 5), 1);",
    )
    .expect("generated field7_mul must compute exact Field<7> products");
}

/// Euclid sign law in generated Rust: int_rem(-1, 7) == 6. This assertion
/// FAILS under the truncated-`%` mutant (which yields -1), so it must be
/// present BEFORE the remainder-sign mutation runs.
#[test]
fn text_int_rem_sign_law_generated() {
    let src = "emath function irs:\n    inputs:\n        a: Int\n        m: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, m)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(src);
    run_generated(
        &lib,
        "assert_eq!(generated::irs(-1, 7), 6);",
    )
    .expect("generated int_rem(-1, 7) must be the Euclidean 6 (not a truncated -1)");
}
