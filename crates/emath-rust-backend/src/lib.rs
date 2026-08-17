//! Rust backend: EMIR → deterministic Rust via the rust-ir AST.
//!
//! Phase 1 generates one crate per admission: a struct per declaration, a
//! constructor with enforced invariants, an evaluation method per
//! `evaluate <target>` goal, and `#[test]` functions for the `tests:`
//! section. Everything is std-only, `#![forbid(unsafe_code)]`, and
//! byte-deterministic.

#![forbid(unsafe_code)]

use emath_exec_ir::{EmirOp, EmirProgram, EmirValue, lower_definition, lower_requirement};
use emath_ir::{GoalKind, SemanticPackage, TypeId, TypeNode};
use emath_rust_ir::ast::{
    BinOp, Block, EnumDef, EnumVariant, Expr, FnDef, ImplDef, Item, Module, Param, RUST_KEYWORDS,
    Stmt, StructDef, TestDef, Ty, UnOp, Visibility, escape_ident, snake_case,
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

#[derive(Clone, Debug)]
pub struct BackendOutput {
    /// Relative path → file content (includes `Cargo.toml` and `src/lib.rs`).
    pub files: BTreeMap<String, String>,
    pub anchors: Vec<BackendAnchor>,
    /// Domain obligations surfaced from lowering, first-encounter order.
    pub assumptions: Vec<String>,
    /// The generated module, so the build path can run
    /// `CrateProfile::validate` (`E-CODEGEN-002`/`E-CODEGEN-004`) on the
    /// exact items that were rendered.
    pub module: Module,
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
            // The declaration name becomes Rust source: keywords and
            // reserved identifiers are escaped (`type` -> `type_`), never
            // emitted raw.
            let struct_name = escape_ident(&name);
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
                name: struct_name.clone(),
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
                        variants: {
                            let mut variants = vec![EnumVariant {
                                name: "FailedPrecondition".to_string(),
                                doc: vec![
                                    "A constructor `require` invariant did not hold.".to_string(),
                                ],
                            }];
                            if !constructor.postconditions.is_empty() {
                                variants.push(EnumVariant {
                                    name: "FailedPostcondition".to_string(),
                                    doc: vec![
                                        "A constructor `ensure`/`invariant` did not hold after field init.".to_string(),
                                    ],
                                });
                            }
                            variants
                        },
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
                // Postconditions (`ensure` / `invariant`) hold after field
                // init: each is checked before the value escapes the
                // constructor, mirroring the `require` gate above.
                for (index, postcondition) in constructor.postconditions.iter().enumerate() {
                    let program = lower_requirement(package, *postcondition, &param_names)
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    let check_name = format!("__post_ok{index}");
                    let negated = Expr::Un {
                        op: UnOp::Not,
                        value: Box::new(value_expr(&program, &param_names, &[])?),
                    };
                    statements.push(Stmt::Let {
                        pattern: check_name.clone(),
                        value: Box::new(negated),
                    });
                    statements.push(Stmt::Expr(Expr::IfElse {
                        condition: Box::new(Expr::Var(check_name)),
                        then: Box::new(Stmt::Block(Block {
                            statements: vec![Stmt::Return(Expr::Call {
                                path: vec!["Err".to_string()],
                                args: vec![Expr::Path(vec![
                                    error_name.clone(),
                                    "FailedPostcondition".to_string(),
                                ])],
                            })],
                        })),
                        else_value: Box::new(Stmt::Block(Block::default())),
                    }));
                }
                statements.push(Stmt::Expr(Expr::Call {
                    path: vec!["Ok".to_string()],
                    args: vec![Expr::StructLiteral {
                        name: "Self".to_string(),
                        fields: field_values,
                    }],
                }));

                let article = match name.chars().next() {
                    Some('A' | 'E' | 'I' | 'O' | 'U' | 'a' | 'e' | 'i' | 'o' | 'u') => "an",
                    _ => "a",
                };
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
                        "Construct {article} `{name}`; every `require` and `ensure` invariant is checked."
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
            // Goals attach by their declared ids on the declaration, never
            // by span geometry (an overlapping offset in another file must
            // not cross-attach a goal).
            let mut goals: Vec<&emath_ir::Goal> = declaration
                .goals
                .iter()
                .filter_map(|goal_id| package.goals.get(goal_id.index()))
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
                    "multiple evaluate goals per declaration are outside the Phase 1 subset; declare one `goals:` target or split the declaration"
                        .to_string(),
                ));
            }
            goals.clear();

            if !methods.is_empty() {
                items.push(Item::Impl(ImplDef {
                    target: struct_name.clone(),
                    generics: vec![],
                    methods,
                    doc: Vec::new(),
                }));
            }

            // --- tests ------------------------------------------------------
            // Tests attach by their declared ids on the declaration, never
            // by span geometry.
            for test_id in &declaration.tests {
                let Some(test) = package.tests.get(test_id.index()) else {
                    continue;
                };
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
                    // The generated API is `Struct::new(params) -> Result<Self,
                    // ConfigError>`, so the instance is an associated-call
                    // followed by `expect`.
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
                            path: vec![struct_name.clone(), constructor.name.clone()],
                            args,
                        }),
                        method: "expect".to_string(),
                        args: vec![Expr::Str(
                            "constructor invariants must hold for this example".to_string(),
                        )],
                    }
                } else {
                    Expr::StructLiteral {
                        name: struct_name.clone(),
                        fields: Vec::new(),
                    }
                };
                statements.push(Stmt::Let {
                    pattern: escape_ident(&instance_name),
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
                // Bind each definition to the evaluated result so `expect`
                // expressions can reference definitions by name. Definitions
                // are the surface; declared `outputs:` are a selection of
                // them (Phase 1: one evaluate goal per declaration).
                let mut expect_names: Vec<String> = given_names.clone();
                for definition in declaration.definitions.keys() {
                    if given_names.contains(definition) {
                        continue;
                    }
                    statements.push(Stmt::Let {
                        pattern: escape_ident(definition),
                        value: Box::new(Expr::Var("actual".to_string())),
                    });
                    expect_names.push(definition.clone());
                }
                let expect_program = lower_definition(package, test.expect, &expect_names, &[])
                    .map_err(BackendError::Lowering)?;
                add_obligations(&expect_program, &mut assumptions);
                // The `expect` expression is a Boolean comparison; assert it
                // with a real macro invocation (rendered via `Expr::Macro`).
                statements.push(Stmt::Expr(Expr::Macro {
                    name: "assert".to_string(),
                    args: vec![value_expr(&expect_program, &expect_names, &[])?],
                }));
                items.push(Item::Test(TestDef {
                    name: test_name,
                    body: Stmt::Block(Block { statements }),
                    doc: vec![format!("Example test: `{}`.", test.name)],
                    // Strict-f64 example tests compare exact float values; the
                    // workspace lints `-D clippy::float_cmp` would otherwise
                    // deny the generated assertion.
                    attrs: vec!["#[allow(clippy::float_cmp)]".to_string()],
                }));
            }
        }

        let module = Module { items };
        let rendered = render_module(&module);
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
            module,
        })
    }

    #[must_use]
    fn cargo_manifest(&self) -> String {
        format!(
            "# Generated by emath Phase 1 (deterministic; do not edit).\n\
             [package]\n\
             name = \"{}\"\n\
             version = \"{}\"\n\
             edition = \"2024\"\n\
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
    // A Rust keyword as a crate name does not compile; escape it with the
    // same `_` suffix the identifier path uses (`type` -> `type_`).
    if RUST_KEYWORDS.contains(&out.as_str()) {
        out.push('_');
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

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::{QualifiedName, Span};
    use emath_ir::{Declaration, DeclarationId, Field};

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
                visibility: emath_ir::Visibility::Public,
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
            compile_spec: emath_ir::CompileSpec::default(),
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
}
