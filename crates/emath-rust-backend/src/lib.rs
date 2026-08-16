//! Rust backend: EMIR → deterministic Rust via the rust-ir AST.
//!
//! Phase 1 generates one crate per admission: a struct per declaration, a
//! constructor with enforced invariants, an evaluation method per
//! `evaluate <target>` goal, and `#[test]` functions for the `tests:`
//! section. Everything is std-only, `#![forbid(unsafe_code)]`, and
//! byte-deterministic.

#![forbid(unsafe_code)]

use emath_exec_ir::{lower_definition, lower_requirement, EmirOp, EmirProgram, EmirValue};
use emath_ir::{GoalKind, SemanticPackage, TypeId, TypeNode};
use emath_rust_ir::ast::{
    escape_ident, snake_case, BinOp, Block, EnumDef, EnumVariant, Expr, FnDef, ImplDef, Item,
    Module, Param, Stmt, StructDef, TestDef, Ty, UnOp, Visibility,
};
use emath_rust_ir::render::render_module;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct BackendInput<'a> {
    pub package: &'a SemanticPackage,
    pub crate_name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendAnchor {
    pub label: String,
    pub file: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendOutput {
    /// Relative path → file content (includes `Cargo.toml` and `src/lib.rs`).
    pub files: BTreeMap<String, String>,
    pub anchors: Vec<BackendAnchor>,
    /// Domain obligations surfaced from lowering, first-encounter order.
    pub assumptions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    NoEvaluateGoal(String),
    UnknownTarget(String),
    MissingInput(String),
    MissingGiven(String),
    UnsupportedType(String),
    MultipleConstructors(String),
    Lowering(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEvaluateGoal(name) => {
                write!(
                    f,
                    "declaration `{name}` needs an `evaluate` goal in Phase 1"
                )
            }
            Self::UnknownTarget(name) => write!(f, "evaluate target `{name}` is not a definition"),
            Self::MissingInput(name) => write!(f, "test body does not supply input `{name}`"),
            Self::MissingGiven(name) => {
                write!(
                    f,
                    "test body does not supply constructor parameter `{name}`"
                )
            }
            Self::UnsupportedType(detail) => write!(f, "unsupported type in Phase 1: {detail}"),
            Self::MultipleConstructors(name) => write!(
                f,
                "declaration `{name}` has multiple constructors (Phase 1 supports one)"
            ),
            Self::Lowering(detail) => write!(f, "EMIR lowering failed: {detail}"),
        }
    }
}

impl std::error::Error for BackendError {}

const DEFAULT_ERROR_TYPE: &str = "ConfigError";

impl BackendInput<'_> {
    /// Run the whole backend: structure + methods + tests + crate files.
    pub fn generate(&self) -> Result<BackendOutput, BackendError> {
        let package = self.package;
        let mut items: Vec<Item> = Vec::new();
        let mut anchors: Vec<BackendAnchor> = Vec::new();
        let mut assumptions: Vec<String> = Vec::new();
        let mut emitted_error_types: Vec<String> = Vec::new();

        items.push(Item::RawAttribute("#![forbid(unsafe_code)]".to_string()));
        items.push(Item::RawAttribute("#![allow(dead_code)]".to_string()));

        for declaration in &package.declarations {
            let name = declaration.name.leaf().to_string();
            let state_names: Vec<String> =
                declaration.state.iter().map(|f| f.name.clone()).collect();
            let input_names: Vec<String> =
                declaration.inputs.iter().map(|f| f.name.clone()).collect();

            // --- struct ----------------------------------------------------
            let state_types: Vec<Ty> = declaration
                .state
                .iter()
                .map(|f| self.rust_ty(f.ty, &name))
                .collect::<Result<_, _>>()?;
            items.push(Item::DocComment(format!(
                "`{name}`: a `{}` declaration generated from `.emath`.",
                declaration.kind_label
            )));
            items.push(Item::DocComment(
                "Generated deterministically by emath Phase 1; do not edit.".to_string(),
            ));
            items.push(Item::Struct(StructDef {
                name: name.clone(),
                generics: vec![],
                fields: state_names.iter().cloned().zip(state_types).collect(),
                derives: vec!["Clone".to_string(), "Debug".to_string()],
                doc: Vec::new(),
                visibility: Visibility::Public,
            }));

            let mut methods: Vec<FnDef> = Vec::new();
            let mut evaluate_targets: Vec<String> = Vec::new();

            // --- constructor ----------------------------------------------
            if declaration.constructors.len() > 1 {
                return Err(BackendError::MultipleConstructors(name));
            }
            if let Some(constructor) = declaration.constructors.first() {
                let error_name = self.error_type_name(constructor.error_type).to_string();
                if !emitted_error_types.contains(&error_name) {
                    emitted_error_types.push(error_name.clone());
                    items.push(Item::DocComment(
                        "Configuration error type returned by failed constructors.".to_string(),
                    ));
                    items.push(Item::Enum(EnumDef {
                        name: error_name.clone(),
                        variants: vec![EnumVariant {
                            name: "FailedPrecondition".to_string(),
                            doc: vec![
                                "A constructor `require` invariant did not hold.".to_string(),
                            ],
                        }],
                        derives: vec![
                            "Clone".to_string(),
                            "Debug".to_string(),
                            "PartialEq".to_string(),
                        ],
                        doc: Vec::new(),
                        visibility: Visibility::Public,
                    }));
                }

                let param_names: Vec<String> = constructor
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let param_types: Vec<Ty> = constructor
                    .parameters
                    .iter()
                    .map(|p| self.rust_ty(p.ty, &name))
                    .collect::<Result<_, _>>()?;
                // The constructor is an associated function: no receiver.
                let params: Vec<Param> = param_names
                    .iter()
                    .cloned()
                    .zip(param_types)
                    .map(|(param_name, ty)| Param {
                        name: param_name,
                        ty,
                    })
                    .collect();

                let mut statements: Vec<Stmt> = Vec::new();
                // Invariants are enforced in generated code: the constructor
                // is a controlled entry point, not a pass-through.
                for (index, precondition) in constructor.preconditions.iter().enumerate() {
                    let program = lower_requirement(package, *precondition, &param_names)
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    let ok_name = format!("__ok{index}");
                    let negated = Expr::Un {
                        op: UnOp::Not,
                        value: Box::new(value_expr(&program, &param_names, &[])?),
                    };
                    statements.push(Stmt::Let {
                        pattern: ok_name.clone(),
                        value: Box::new(negated),
                    });
                    statements.push(Stmt::Expr(Expr::IfElse {
                        condition: Box::new(Expr::Var(ok_name)),
                        then: Box::new(Stmt::Block(Block {
                            statements: vec![Stmt::Return(Expr::Call {
                                path: vec!["Err".to_string()],
                                args: vec![Expr::Path(vec![
                                    error_name.clone(),
                                    "FailedPrecondition".to_string(),
                                ])],
                            })],
                        })),
                        else_value: Box::new(Stmt::Block(Block::default())),
                    }));
                }

                // `Self:` assignments establish field values, emitted in
                // struct definition order (state declaration order) so the
                // literal matches the struct field layout exactly.
                let mut field_values: Vec<(String, Expr)> = Vec::new();
                for field_def in &declaration.state {
                    let expr_id =
                        *constructor
                            .assignments
                            .get(&field_def.name)
                            .ok_or_else(|| {
                                BackendError::Lowering(format!(
                                    "no `Self:` assignment for `{}`",
                                    field_def.name
                                ))
                            })?;
                    let program = lower_definition(package, expr_id, &param_names, &[])
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    field_values.push((
                        field_def.name.clone(),
                        value_expr(&program, &param_names, &[])?,
                    ));
                }
                statements.push(Stmt::Expr(Expr::Call {
                    path: vec!["Ok".to_string()],
                    args: vec![Expr::StructLiteral {
                        name: "Self".to_string(),
                        fields: field_values,
                    }],
                }));

                methods.push(FnDef {
                    name: escape_ident(&constructor.name),
                    generics: vec![],
                    params,
                    ret: Ty::Result {
                        ok: Box::new(Ty::SelfType),
                        error: Box::new(Ty::Named(error_name)),
                    },
                    body: Stmt::Block(Block { statements }),
                    doc: vec![format!(
                        "Construct a `{name}`; every `require` invariant is checked."
                    )],
                    visibility: if constructor.is_public {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    },
                    attrs: Vec::new(),
                });
            }
            let _ = name;

            // --- evaluation methods ----------------------------------------
            let mut goals: Vec<&emath_ir::Goal> = package
                .goals
                .iter()
                .filter(|goal| declaration.source.contains(goal.source.start))
                .filter(|goal| goal.kind == GoalKind::Evaluate)
                .collect();
            for goal in &goals {
                let target = goal.target.clone();
                let Some(expr) = declaration.definitions.get(&target).copied() else {
                    return Err(BackendError::UnknownTarget(target));
                };
                let program = lower_definition(package, expr, &input_names, &state_names)
                    .map_err(BackendError::Lowering)?;
                add_obligations(&program, &mut assumptions);
                let mut params = vec![Param {
                    name: "self".to_string(),
                    ty: Ty::Ref(Box::new(Ty::SelfType)),
                }];
                for input in &input_names {
                    let ty = declaration
                        .inputs
                        .iter()
                        .find(|f| &f.name == input)
                        .map(|f| f.ty)
                        .ok_or_else(|| BackendError::UnknownTarget(input.clone()))
                        .and_then(|id| self.rust_ty(id, &name))?;
                    params.push(Param {
                        name: escape_ident(input),
                        ty,
                    });
                }
                let body = Stmt::Block(Block {
                    statements: vec![Stmt::Expr(value_expr(
                        &program,
                        &input_names,
                        &state_names,
                    )?)],
                });
                evaluate_targets.push(target.clone());
                methods.push(FnDef {
                    name: escape_ident(&target),
                    generics: vec![],
                    params,
                    ret: Ty::F64,
                    body,
                    doc: vec![format!("Evaluate `{target}` (strict-f64, Phase 1).")],
                    visibility: Visibility::Public,
                    attrs: Vec::new(),
                });
            }
            if goals.len() > 1 {
                return Err(BackendError::NoEvaluateGoal(
                    "Phase 1 supports one evaluate goal per declaration".to_string(),
                ));
            }
            goals.clear();

            if !methods.is_empty() {
                items.push(Item::Impl(ImplDef {
                    target: declaration.name.leaf().to_string(),
                    generics: vec![],
                    methods,
                    doc: Vec::new(),
                }));
            }

            // --- tests ------------------------------------------------------
            for test in package
                .tests
                .iter()
                .filter(|test| declaration.source.contains(test.source.start))
            {
                let test_name = snake_case(&test.name);
                let given_names: Vec<String> = test.given.keys().cloned().collect();
                let mut statements: Vec<Stmt> = Vec::new();
                let mut seen: Vec<String> = Vec::new();
                for given_name in &given_names {
                    let program = lower_definition(package, test.given[given_name], &seen, &[])
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    statements.push(Stmt::Let {
                        pattern: escape_ident(given_name),
                        value: Box::new(value_expr(&program, &seen, &[])?),
                    });
                    seen.push(given_name.clone());
                }
                let instance_name = snake_case(declaration.name.leaf());
                let instance: Expr = if let Some(constructor) = declaration.constructors.first() {
                    let args: Vec<Expr> = constructor
                        .parameters
                        .iter()
                        .map(|p| {
                            if !given_names.contains(&p.name) {
                                return Err(BackendError::MissingGiven(p.name.clone()));
                            }
                            Ok(Expr::Var(escape_ident(&p.name)))
                        })
                        .collect::<Result<_, _>>()?;
                    Expr::MethodCall {
                        receiver: Box::new(Expr::Call {
                            path: vec![declaration.name.leaf().to_string()],
                            args,
                        }),
                        method: "expect".to_string(),
                        args: vec![Expr::Str(
                            "constructor invariants must hold for this example".to_string(),
                        )],
                    }
                } else {
                    Expr::StructLiteral {
                        name: declaration.name.leaf().to_string(),
                        fields: Vec::new(),
                    }
                };
                statements.push(Stmt::Let {
                    pattern: instance_name.clone(),
                    value: Box::new(instance),
                });
                let Some(target) = evaluate_targets.first() else {
                    return Err(BackendError::NoEvaluateGoal(
                        declaration.name.leaf().to_string(),
                    ));
                };
                let mut eval_args: Vec<Expr> = Vec::new();
                for input in &declaration.inputs {
                    if !given_names.contains(&input.name) {
                        return Err(BackendError::MissingInput(input.name.clone()));
                    }
                    eval_args.push(Expr::Var(escape_ident(&input.name)));
                }
                statements.push(Stmt::Let {
                    pattern: "actual".to_string(),
                    value: Box::new(Expr::MethodCall {
                        receiver: Box::new(Expr::Var(instance_name)),
                        method: escape_ident(target),
                        args: eval_args,
                    }),
                });
                // Bind each output to the evaluated result so `expect`
                // expressions can reference outputs by name.
                let mut expect_names: Vec<String> = given_names.clone();
                for output in &declaration.outputs {
                    if given_names.contains(&output.name) {
                        continue;
                    }
                    statements.push(Stmt::Let {
                        pattern: escape_ident(&output.name),
                        value: Box::new(Expr::Var("actual".to_string())),
                    });
                    expect_names.push(output.name.clone());
                }
                let expect_program = lower_definition(package, test.expect, &expect_names, &[])
                    .map_err(BackendError::Lowering)?;
                add_obligations(&expect_program, &mut assumptions);
                statements.push(Stmt::Expr(Expr::Call {
                    path: vec!["assert_eq".to_string()],
                    args: vec![
                        Expr::Var("actual".to_string()),
                        value_expr(&expect_program, &expect_names, &[])?,
                    ],
                }));
                items.push(Item::Test(TestDef {
                    name: test_name,
                    body: Stmt::Block(Block { statements }),
                    doc: vec![format!("Example test: `{}`.", test.name)],
                }));
            }
        }

        let rendered = render_module(&Module { items });
        anchors.extend(rendered.anchors.into_iter().map(|anchor| BackendAnchor {
            label: anchor.label,
            file: "src/lib.rs".to_string(),
            start: anchor.start,
            end: anchor.end,
        }));
        let files = BTreeMap::from([
            ("Cargo.toml".to_string(), self.cargo_manifest()),
            ("src/lib.rs".to_string(), rendered.code),
        ]);
        Ok(BackendOutput {
            files,
            anchors,
            assumptions,
        })
    }

    #[must_use]
    fn cargo_manifest(&self) -> String {
        format!(
            "# Generated by emath Phase 1 (deterministic; do not edit).\n\
             [package]\n\
             name = \"{}\"\n\
             version = \"{}\"\n\
             edition = \"2021\"\n\
             description = \"Generated from an .emath declaration (strict-f64 native).\"\n\
             license = \"MIT OR Apache-2.0\"\n\
             \n\
             [lib]\n\
             path = \"src/lib.rs\"\n\
             \n\
             [dependencies]\n",
            sanitize_crate_name(&self.crate_name),
            sanitize_version(&self.version)
        )
    }

    fn rust_ty(&self, ty: TypeId, owner: &str) -> Result<Ty, BackendError> {
        let Some(node) = self.package.ty(ty) else {
            return Err(BackendError::UnsupportedType(format!(
                "unknown type id in `{owner}`"
            )));
        };
        match node {
            TypeNode::Float64 => Ok(Ty::F64),
            TypeNode::Bool => Ok(Ty::Bool),
            other => Err(BackendError::UnsupportedType(format!(
                "`{}` in `{owner}`",
                other.display_name()
            ))),
        }
    }

    fn error_type_name(&self, error_type: Option<TypeId>) -> &str {
        let Some(ty) = error_type.and_then(|id| self.package.ty(id)) else {
            return DEFAULT_ERROR_TYPE;
        };
        if let TypeNode::Other(name) = ty {
            if !name.leaf().is_empty() && name.leaf() != "Self" {
                return "ConfigError";
            }
        }
        DEFAULT_ERROR_TYPE
    }
}

fn sanitize_crate_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out = "emath_artifact".to_string();
    }
    if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        out.insert_str(0, "emath_");
    }
    out
}

fn sanitize_version(version: &str) -> String {
    if version.is_empty() {
        return "0.1.0".to_string();
    }
    let mut out = String::new();
    let mut digits = 0;
    for ch in version.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            out.push(ch);
            if ch == '.' {
                digits = 0;
            } else {
                digits += 1;
            }
        } else if ch == '-' {
            out.push('-');
        } else {
            break;
        }
        if digits > 4 {
            break;
        }
    }
    while out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() {
        "0.1.0".to_string()
    } else {
        out
    }
}

fn add_obligations(program: &EmirProgram, out: &mut Vec<String>) {
    for obligation in &program.domain_obligations {
        let text = obligation.as_str();
        if !out.iter().any(|existing| existing == text) {
            out.push(text.to_string());
        }
    }
}

/// Render the program as an expression. Multi-op programs become a block
/// `{ let __e0 = ...; ...; __eN }`; single-op programs inline directly.
fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
) -> Result<Expr, BackendError> {
    if program.ops.len() == 1 {
        return op_expr(&program.ops[0].0, program, names, states);
    }
    let mut statements = Vec::new();
    for (index, (op, _)) in program.ops.iter().enumerate() {
        let expr = op_expr(op, program, names, states)?;
        if index == program.ops.len() - 1 {
            // Tail: the final value is the block expression itself.
            statements.push(Stmt::Expr(expr));
        } else {
            statements.push(Stmt::Let {
                pattern: format!("__e{index}"),
                value: Box::new(expr),
            });
        }
    }
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}

/// Operand reference: every op is materialized as `__e<i>`.
fn operand(_program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Var(format!("__e{}", value.0))
}

fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::LoadInput(index) => {
            let name = names
                .get(*index as usize)
                .ok_or_else(|| BackendError::Lowering("load-input out of range".into()))?;
            Ok(Expr::Var(escape_ident(name)))
        }
        EmirOp::LoadState(index) => {
            let name = states
                .get(*index as usize)
                .ok_or_else(|| BackendError::Lowering("load-state out of range".into()))?;
            Ok(Expr::Field {
                receiver: Box::new(Expr::SelfValue),
                field: name.clone(),
            })
        }
        EmirOp::F64Add(l, r) => Ok(Expr::Bin {
            op: BinOp::Add,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Sub(l, r) => Ok(Expr::Bin {
            op: BinOp::Sub,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Mul(l, r) => Ok(Expr::Bin {
            op: BinOp::Mul,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Div(l, r) => Ok(Expr::Bin {
            op: BinOp::Div,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(operand(program, *l)),
            right: Box::new(operand(program, *r)),
        }),
        EmirOp::Neg(value) => Ok(Expr::Un {
            op: UnOp::Neg,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::Exp(value) => Ok(unary_method("exp", *value, program)),
        EmirOp::Ln(value) => Ok(unary_method("ln", *value, program)),
        EmirOp::Sqrt(value) => Ok(unary_method("sqrt", *value, program)),
        EmirOp::Sin(value) => Ok(unary_method("sin", *value, program)),
        EmirOp::Cos(value) => Ok(unary_method("cos", *value, program)),
        EmirOp::Tan(value) => Ok(unary_method("tan", *value, program)),
        EmirOp::Tanh(value) => Ok(unary_method("tanh", *value, program)),
        EmirOp::Abs(value) => Ok(unary_method("abs", *value, program)),
        EmirOp::Floor(value) => Ok(unary_method("floor", *value, program)),
        EmirOp::Ceil(value) => Ok(unary_method("ceil", *value, program)),
        EmirOp::Min(l, r) => Ok(binary_method("min", *l, *r, program)),
        EmirOp::Max(l, r) => Ok(binary_method("max", *l, *r, program)),
        EmirOp::Atan2(l, r) => Ok(binary_method("atan2", *l, *r, program)),
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(operand(program, *value)),
            method: "is_finite".to_string(),
            args: Vec::new(),
        }),
        EmirOp::Lt(l, r) => Ok(comparison(BinOp::Lt, *l, *r, program)),
        EmirOp::Le(l, r) => Ok(comparison(BinOp::Le, *l, *r, program)),
        EmirOp::Gt(l, r) => Ok(comparison(BinOp::Gt, *l, *r, program)),
        EmirOp::Ge(l, r) => Ok(comparison(BinOp::Ge, *l, *r, program)),
        EmirOp::Eq(l, r) => Ok(comparison(BinOp::Eq, *l, *r, program)),
        EmirOp::Ne(l, r) => Ok(comparison(BinOp::Ne, *l, *r, program)),
        EmirOp::And(l, r) => Ok(comparison(BinOp::And, *l, *r, program)),
        EmirOp::Or(l, r) => Ok(comparison(BinOp::Or, *l, *r, program)),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => Ok(Expr::IfElse {
            condition: Box::new(operand(program, *condition)),
            then: Box::new(Stmt::Expr(operand(program, *then_value))),
            else_value: Box::new(Stmt::Expr(operand(program, *else_value))),
        }),
    }
}

fn unary_method(method: &str, value: EmirValue, program: &EmirProgram) -> Expr {
    Expr::Un {
        op: UnOp::Method(method.to_string()),
        value: Box::new(operand(program, value)),
    }
}

fn binary_method(method: &str, left: EmirValue, right: EmirValue, program: &EmirProgram) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(operand(program, left)),
        method: method.to_string(),
        args: vec![operand(program, right)],
    }
}

fn comparison(op: BinOp, left: EmirValue, right: EmirValue, program: &EmirProgram) -> Expr {
    Expr::Bin {
        op,
        left: Box::new(operand(program, left)),
        right: Box::new(operand(program, right)),
    }
}
