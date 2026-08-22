//! Rust backend: EMIR → deterministic Rust via the rust-ir AST.
//!
//! Phase 1 generates one crate per admission: a struct plus constructor
//! for stateful declarations, a free function (not a method on an empty
//! struct) when there is no state and no constructors, an evaluation
//! item per `evaluate <target>` goal, and `#[test]` functions for the
//! `tests:` section. Everything is std-only, `#![forbid(unsafe_code)]`,
//! and byte-deterministic.

#![forbid(unsafe_code)]

use emath_exec_ir::{
    definition_order, lower_definition, lower_requirement, EmirOp, EmirProgram, EmirValue,
};
use emath_ir::{ConstructionReceipt, ExprId, ExprNode, GoalKind, SemanticPackage, TypeId, TypeNode};
use emath_rust_ir::ast::{
    escape_ident, snake_case, BinOp, Block, EnumDef, EnumVariant, Expr, FnDef, ImplDef, Item,
    Module, Param, Stmt, StructDef, TestDef, Ty, UnOp, Visibility, RUST_KEYWORDS,
};
use emath_rust_ir::render::{render_expr, render_module};
use std::collections::{BTreeMap, BTreeSet};

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
    /// One construction receipt per generated constructor: the obligation
    /// matrix (class + kind per obligation) the emitted code discharges.
    pub receipts: Vec<ConstructionReceipt>,
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
        let mut receipts: Vec<ConstructionReceipt> = Vec::new();

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
            let stateless = declaration.state.is_empty() && declaration.constructors.is_empty();
            let has_evaluate = declaration.goals.iter().any(|goal_id| {
                package
                    .goals
                    .get(goal_id.index())
                    .is_some_and(|goal| goal.kind == GoalKind::Evaluate)
            });
            // No state and no constructors: emit a free function instead of
            // a method on an empty struct. A stateless declaration with
            // nothing to evaluate still keeps a unit struct so the
            // declaration name remains a Rust identifier.
            let emit_free_fn = stateless && has_evaluate;

            let mut used_names = BTreeSet::new();
            for expr in declaration.definitions.values().copied() {
                collect_var_names(package, expr, &mut used_names);
            }
            emit_host_structs(&mut items, declaration, package, &used_names, &name)?;

            items.push(Item::DocComment(format!(
                "`{name}`: a `{}` declaration generated from `.emath`.",
                declaration.kind_label
            )));
            items.push(Item::DocComment(
                "Generated deterministically by emath Phase 1; do not edit.".to_string(),
            ));
            if !emit_free_fn {
                let state_types: Vec<Ty> = declaration
                    .state
                    .iter()
                    .map(|f| self.rust_ty(f.ty, &name))
                    .collect::<Result<_, _>>()?;
                items.push(Item::Struct(StructDef {
                    name: struct_name.clone(),
                    generics: vec![],
                    fields: state_names.iter().cloned().zip(state_types).collect(),
                    derives: vec!["Clone".to_string(), "Debug".to_string()],
                    doc: Vec::new(),
                    visibility: Visibility::Public,
                }));
            }

            let mut methods: Vec<FnDef> = Vec::new();
            let mut evaluate_targets: Vec<String> = Vec::new();

            // --- constructor ----------------------------------------------
            if declaration.constructors.len() > 1 {
                return Err(BackendError::MultipleConstructors(name));
            }
            if let Some(constructor) = declaration.constructors.first() {
                // The receipt records the exact obligation matrix the
                // emitted constructor discharges (all runtime in Phase 1).
                receipts.push(constructor.receipt(&name));
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
            let _ = &struct_name;

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
                if !declaration.definitions.contains_key(&target) {
                    return Err(BackendError::UnknownTarget(target));
                }
                let order = definition_order(package, declaration);
                let Some(end) = order.iter().position(|(name, _)| *name == &target) else {
                    return Err(BackendError::UnknownTarget(target));
                };
                let chain = &order[..=end];
                let mut available = input_names.clone();
                let mut body_stmts = Vec::new();
                for (def_name, def_expr) in chain {
                    let def_name = *def_name;
                    let def_expr = *def_expr;
                    let used = {
                        let mut names = BTreeSet::new();
                        collect_var_names(package, def_expr, &mut names);
                        names
                    };
                    let lowering_inputs = expand_host_inputs(&available, &used);
                    let program = lower_definition(package, def_expr, &lowering_inputs, &state_names)
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    let value = value_expr(&program, &lowering_inputs, &state_names)?;
                    if def_name == &target {
                        body_stmts.push(Stmt::Expr(value));
                    } else {
                        body_stmts.push(Stmt::Let {
                            pattern: escape_ident(def_name),
                            value: Box::new(value),
                        });
                        available.push(def_name.clone());
                    }
                }
                let mut params = if emit_free_fn {
                    Vec::new()
                } else {
                    vec![Param {
                        name: "self".to_string(),
                        ty: Ty::Ref(Box::new(Ty::SelfType)),
                    }]
                };
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
                    statements: body_stmts,
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

            if declaration.kind_label == "model" && !emit_free_fn {
                self.emit_model_step_methods(
                    package,
                    declaration,
                    &name,
                    &input_names,
                    &state_names,
                    &mut methods,
                    &mut assumptions,
                )?;
            }

            if emit_free_fn {
                for method in methods {
                    items.push(Item::Fn(method));
                }
            } else if !methods.is_empty() {
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
                let eval_call = if emit_free_fn {
                    Expr::Call {
                        path: vec![escape_ident(target)],
                        args: eval_args,
                    }
                } else {
                    let instance_name = snake_case(declaration.name.leaf());
                    let instance: Expr = if let Some(constructor) = declaration.constructors.first()
                    {
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
                    } else if !declaration.state.is_empty() {
                        let fields = declaration
                            .state
                            .iter()
                            .map(|field| {
                                if !given_names.contains(&field.name) {
                                    return Err(BackendError::MissingGiven(field.name.clone()));
                                }
                                Ok((
                                    field.name.clone(),
                                    Expr::Var(escape_ident(&field.name)),
                                ))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        Expr::StructLiteral {
                            name: struct_name.clone(),
                            fields,
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
                    Expr::MethodCall {
                        receiver: Box::new(Expr::Var(instance_name)),
                        method: escape_ident(target),
                        args: eval_args,
                    }
                };
                statements.push(Stmt::Let {
                    pattern: "actual".to_string(),
                    value: Box::new(eval_call),
                });
                // Bind each definition to the evaluated result so `expect`
                // expressions can reference definitions by name. Definitions
                // are the surface; declared `outputs:` are a selection of
                // them (Phase 1: one evaluate goal per declaration).
                let mut expect_names: Vec<String> = given_names.clone();
                for definition in declaration.definitions.keys() {
                    if given_names.contains(definition) || definition.starts_with("der_") {
                        continue;
                    }
                    statements.push(Stmt::Let {
                        pattern: escape_ident(definition),
                        value: Box::new(Expr::Var("actual".to_string())),
                    });
                    expect_names.push(definition.clone());
                }
                if let Some(expect) = test.expect {
                    let expect_program =
                        lower_definition(package, expect, &expect_names, &state_names)
                            .map_err(BackendError::Lowering)?;
                    add_obligations(&expect_program, &mut assumptions);
                    // The `expect` expression is a Boolean comparison; assert it
                    // with a real macro invocation (rendered via `Expr::Macro`).
                    statements.push(Stmt::Expr(Expr::Macro {
                        name: "assert".to_string(),
                        args: vec![value_expr(&expect_program, &expect_names, &state_names)?],
                    }));
                } else {
                    // Worked example: execute the computation, assert nothing.
                    let unused = declaration
                        .definitions
                        .keys()
                        .find(|definition| {
                            !given_names.contains(definition) && !definition.starts_with("der_")
                        })
                        .cloned()
                        .unwrap_or_else(|| "actual".to_string());
                    statements.push(Stmt::Let {
                        pattern: "_".to_string(),
                        value: Box::new(Expr::Var(escape_ident(&unused))),
                    });
                }
                items.push(Item::Test(TestDef {
                    name: test_name,
                    body: Stmt::Block(Block { statements }),
                    doc: vec![if test.expect.is_some() {
                        format!("Example test: `{}`.", test.name)
                    } else {
                        format!("Worked example: `{}`.", test.name)
                    }],
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
            receipts,
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

    fn emit_model_step_methods(
        &self,
        package: &SemanticPackage,
        declaration: &emath_ir::Declaration,
        owner: &str,
        input_names: &[String],
        state_names: &[String],
        methods: &mut Vec<FnDef>,
        assumptions: &mut Vec<String>,
    ) -> Result<(), BackendError> {
        if declaration.state.is_empty() {
            return Ok(());
        }
        let order = definition_order(package, declaration);
        for field in &declaration.state {
            let rate_name = format!("der_{}", field.name);
            let Some(end) = order.iter().position(|(name, _)| *name == &rate_name) else {
                return Ok(());
            };
            let chain = &order[..=end];
            let mut available = input_names.to_vec();
            let mut body_stmts = Vec::new();
            for (def_name, def_expr) in chain {
                let used = {
                    let mut names = BTreeSet::new();
                    collect_var_names(package, *def_expr, &mut names);
                    names
                };
                let lowering_inputs = expand_host_inputs(&available, &used);
                let program = lower_definition(package, *def_expr, &lowering_inputs, state_names)
                    .map_err(BackendError::Lowering)?;
                add_obligations(&program, assumptions);
                let value = value_expr(&program, &lowering_inputs, state_names)?;
                if *def_name == &rate_name {
                    body_stmts.push(Stmt::Expr(value));
                } else {
                    body_stmts.push(Stmt::Let {
                        pattern: escape_ident(def_name),
                        value: Box::new(value),
                    });
                    available.push((*def_name).clone());
                }
            }
            let mut params = vec![Param {
                name: "self".to_string(),
                ty: Ty::Ref(Box::new(Ty::SelfType)),
            }];
            for input in input_names {
                let ty = declaration
                    .inputs
                    .iter()
                    .find(|field| &field.name == input)
                    .map(|field| field.ty)
                    .ok_or_else(|| BackendError::UnknownTarget(input.clone()))
                    .and_then(|id| self.rust_ty(id, owner))?;
                params.push(Param {
                    name: escape_ident(input),
                    ty,
                });
            }
            methods.push(FnDef {
                name: escape_ident(&rate_name),
                generics: vec![],
                params,
                ret: self.rust_ty(field.ty, owner)?,
                body: Stmt::Block(Block {
                    statements: body_stmts,
                }),
                doc: vec![format!("Explicit rate `{rate_name}` at the current state.")],
                visibility: Visibility::Public,
                attrs: Vec::new(),
            });
        }

        let mut step_params = vec![Param {
            name: "self".to_string(),
            ty: Ty::Ref(Box::new(Ty::SelfType)),
        }];
        for input in input_names {
            let ty = declaration
                .inputs
                .iter()
                .find(|field| &field.name == input)
                .map(|field| field.ty)
                .ok_or_else(|| BackendError::UnknownTarget(input.clone()))
                .and_then(|id| self.rust_ty(id, owner))?;
            step_params.push(Param {
                name: escape_ident(input),
                ty,
            });
        }
        step_params.push(Param {
            name: "dt".to_string(),
            ty: Ty::F64,
        });
        let input_args: Vec<Expr> = input_names
            .iter()
            .map(|input| Expr::Var(escape_ident(input)))
            .collect();
        methods.push(FnDef {
            name: "step_euler".to_string(),
            generics: vec![],
            params: step_params.clone(),
            ret: Ty::SelfType,
            body: self.step_euler_body(declaration, owner, &input_args)?,
            doc: vec!["Forward Euler step from explicit `der_<state>` rates.".to_string()],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });
        methods.push(FnDef {
            name: "step_rk4".to_string(),
            generics: vec![],
            params: step_params,
            ret: Ty::SelfType,
            body: self.step_rk4_body(declaration, owner, &input_args)?,
            doc: vec!["Classic RK4 step from explicit `der_<state>` rates.".to_string()],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });
        Ok(())
    }

    fn step_euler_body(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        input_args: &[Expr],
    ) -> Result<Stmt, BackendError> {
        let mut statements = Vec::new();
        let mut fields = Vec::new();
        for field in &declaration.state {
            let rate = format!("k1_{}", field.name);
            statements.push(Stmt::Let {
                pattern: rate.clone(),
                value: Box::new(rate_call(&field.name, input_args)),
            });
            let node = self
                .package
                .ty(field.ty)
                .ok_or_else(|| BackendError::UnsupportedType(format!("unknown state type in `{owner}`")))?;
            fields.push((
                field.name.clone(),
                add_scaled_expr(
                    Expr::Field {
                        receiver: Box::new(Expr::SelfValue),
                        field: field.name.clone(),
                    },
                    Expr::Var(rate),
                    Expr::Var("dt".to_string()),
                    node,
                ),
            ));
        }
        statements.push(Stmt::Expr(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        }));
        Ok(Stmt::Block(Block { statements }))
    }

    fn step_rk4_body(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        input_args: &[Expr],
    ) -> Result<Stmt, BackendError> {
        let half = Expr::Bin {
            op: BinOp::Div,
            left: Box::new(Expr::Var("dt".to_string())),
            right: Box::new(Expr::F64(2.0_f64.to_bits())),
        };
        let sixth = Expr::Bin {
            op: BinOp::Div,
            left: Box::new(Expr::Var("dt".to_string())),
            right: Box::new(Expr::F64(6.0_f64.to_bits())),
        };
        let two_sixths = Expr::Bin {
            op: BinOp::Div,
            left: Box::new(Expr::Bin {
                op: BinOp::Mul,
                left: Box::new(Expr::F64(2.0_f64.to_bits())),
                right: Box::new(Expr::Var("dt".to_string())),
            }),
            right: Box::new(Expr::F64(6.0_f64.to_bits())),
        };
        let mut statements = Vec::new();
        statements.extend(rate_lets("self", "k1", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s2".to_string(),
            value: Box::new(self.shifted_state(declaration, owner, "k1", &half)?),
        });
        statements.extend(rate_lets("s2", "k2", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s3".to_string(),
            value: Box::new(self.shifted_state(declaration, owner, "k2", &half)?),
        });
        statements.extend(rate_lets("s3", "k3", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s4".to_string(),
            value: Box::new(self.shifted_state(
                declaration,
                owner,
                "k3",
                &Expr::Var("dt".to_string()),
            )?),
        });
        statements.extend(rate_lets("s4", "k4", declaration, input_args));
        let mut fields = Vec::new();
        for field in &declaration.state {
            let node = self
                .package
                .ty(field.ty)
                .ok_or_else(|| BackendError::UnsupportedType(format!("unknown state type in `{owner}`")))?;
            let mut next = Expr::Field {
                receiver: Box::new(Expr::SelfValue),
                field: field.name.clone(),
            };
            for (scale, prefix) in [
                (&sixth, "k1"),
                (&two_sixths, "k2"),
                (&two_sixths, "k3"),
                (&sixth, "k4"),
            ] {
                next = add_scaled_expr(
                    next,
                    Expr::Var(format!("{prefix}_{}", field.name)),
                    scale.clone(),
                    node,
                );
            }
            fields.push((field.name.clone(), next));
        }
        statements.push(Stmt::Expr(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        }));
        Ok(Stmt::Block(Block { statements }))
    }

    fn shifted_state(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        rate_prefix: &str,
        scale: &Expr,
    ) -> Result<Expr, BackendError> {
        let mut fields = Vec::new();
        for field in &declaration.state {
            let node = self
                .package
                .ty(field.ty)
                .ok_or_else(|| BackendError::UnsupportedType(format!("unknown state type in `{owner}`")))?;
            fields.push((
                field.name.clone(),
                add_scaled_expr(
                    Expr::Field {
                        receiver: Box::new(Expr::SelfValue),
                        field: field.name.clone(),
                    },
                    Expr::Var(format!("{rate_prefix}_{}", field.name)),
                    scale.clone(),
                    node,
                ),
            ));
        }
        Ok(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        })
    }

    fn rust_ty(&self, ty: TypeId, owner: &str) -> Result<Ty, BackendError> {
        let Some(node) = self.package.ty(ty) else {
            return Err(BackendError::UnsupportedType(format!(
                "unknown type id in `{owner}`"
            )));
        };
        self.rust_node(node, owner)
    }

    fn rust_node(&self, node: &TypeNode, owner: &str) -> Result<Ty, BackendError> {
        match node {
            TypeNode::Float64 => Ok(Ty::F64),
            TypeNode::Bool => Ok(Ty::Bool),
            TypeNode::Vector { .. } => Ok(Ty::Named("Vec<f64>".to_string())),
            TypeNode::Matrix { .. } => Ok(Ty::Named("Vec<Vec<f64>>".to_string())),
            TypeNode::Refinement { base, .. } => self.rust_node(base, owner),
            TypeNode::UnitRef { .. } => Ok(Ty::F64),
            TypeNode::Opaque { name, .. } => {
                let leaf = name.leaf();
                if leaf.is_empty() {
                    Err(BackendError::UnsupportedType(format!(
                        "anonymous host type in `{owner}`"
                    )))
                } else {
                    Ok(Ty::Named(escape_ident(leaf)))
                }
            }
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
            if let Some((base, field)) = name.split_once('.') {
                Ok(Expr::Field {
                    receiver: Box::new(Expr::Var(escape_ident(base))),
                    field: field.to_string(),
                })
            } else {
                Ok(Expr::Var(escape_ident(name)))
            }
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
        EmirOp::VectorCreate(elements) => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements.iter().map(|e| operand(program, *e)).collect(),
        }),
        EmirOp::MatrixCreate {
            rows,
            cols,
            elements,
        } => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: (0..*rows)
                .map(|r| Expr::Macro {
                    name: "vec".to_string(),
                    args: (0..*cols)
                        .map(|c| operand(program, elements[r * cols + c]))
                        .collect(),
                })
                .collect(),
        }),
        EmirOp::VectorIndex { vector, index } => Ok(Expr::Index {
            target: Box::new(operand(program, *vector)),
            index: Box::new(Expr::Cast {
                value: Box::new(operand(program, *index)),
                target: Ty::Named("usize".to_string()),
            }),
        }),
        EmirOp::MatrixIndex { matrix, row, col } => Ok(Expr::Index {
            target: Box::new(Expr::Index {
                target: Box::new(operand(program, *matrix)),
                index: Box::new(Expr::Cast {
                    value: Box::new(operand(program, *row)),
                    target: Ty::Named("usize".to_string()),
                }),
            }),
            index: Box::new(Expr::Cast {
                value: Box::new(operand(program, *col)),
                target: Ty::Named("usize".to_string()),
            }),
        }),
        EmirOp::VectorAdd(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorSub(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a - b).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorScale(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().map(|x| x * {}).collect::<Vec<f64>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorDot(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a * b).sum::<f64>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::VectorNorm(v) => Ok(Expr::Raw(format!(
            "{}.iter().map(|x| x * x).sum::<f64>().sqrt()",
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::VectorLength(v) => Ok(Expr::Raw(format!(
            "({}.len() as f64)",
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::MatrixAdd(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a + b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixSub(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a - b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixScale(l, r) => Ok(Expr::Raw(format!(
            "{}.iter().map(|row| row.iter().map(|x| x * {}).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixMulVector(m, v) => Ok(Expr::Raw(format!(
            "{{ let m = &{}; let v = &{}; m.iter().map(|row| row.iter().zip(v.iter()).map(|(a, b)| a * b).sum::<f64>()).collect::<Vec<f64>>() }}",
            render_expr(&operand(program, *m)),
            render_expr(&operand(program, *v)),
        ))),
        EmirOp::MatrixMulMatrix(l, r) => Ok(Expr::Raw(format!(
            "{{ let m1 = &{}; let m2 = &{}; let r1 = m1.len(); let c2 = if m2.is_empty() {{ 0 }} else {{ m2[0].len() }}; let c1 = if m1.is_empty() {{ 0 }} else {{ m1[0].len() }}; (0..r1).map(|i| (0..c2).map(|j| (0..c1).map(|k| m1[i][k] * m2[k][j]).sum::<f64>()).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>() }}",
            render_expr(&operand(program, *l)),
            render_expr(&operand(program, *r)),
        ))),
        EmirOp::MatrixTranspose(m) => Ok(Expr::Raw(format!(
            "{{ let m = &{}; if m.is_empty() {{ vec![] }} else {{ let rows = m.len(); let cols = m[0].len(); (0..cols).map(|c| (0..rows).map(|r| m[r][c]).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>() }} }}",
            render_expr(&operand(program, *m)),
        ))),
    }
}

fn collect_var_names(package: &SemanticPackage, id: ExprId, out: &mut BTreeSet<String>) {
    let Some(expr) = package.expr(id) else {
        return;
    };
    match expr {
        ExprNode::Literal(_) => {}
        ExprNode::Variable(name) => {
            out.insert(name.0.clone());
        }
        ExprNode::Call { arguments, .. } => {
            for argument in arguments {
                collect_var_names(package, *argument, out);
            }
        }
        ExprNode::Unary { value, .. } => collect_var_names(package, *value, out),
        ExprNode::Binary { left, right, .. } => {
            collect_var_names(package, *left, out);
            collect_var_names(package, *right, out);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_var_names(package, *condition, out);
            collect_var_names(package, *then_value, out);
            collect_var_names(package, *else_value, out);
        }
        ExprNode::Record { fields, .. } => {
            for value in fields.values() {
                collect_var_names(package, *value, out);
            }
        }
        ExprNode::Index { value, indices } => {
            collect_var_names(package, *value, out);
            for index in indices {
                collect_var_names(package, *index, out);
            }
        }
        ExprNode::Vector(elements) => {
            for element in elements {
                collect_var_names(package, *element, out);
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows {
                for element in row {
                    collect_var_names(package, *element, out);
                }
            }
        }
        ExprNode::Binder { body, .. } => collect_var_names(package, *body, out),
    }
}

fn expand_host_inputs(inputs: &[String], used: &BTreeSet<String>) -> Vec<String> {
    let mut names = Vec::new();
    for input in inputs {
        let prefix = format!("{input}.");
        let mut fields: Vec<String> = used
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .cloned()
            .collect();
        if fields.is_empty() {
            names.push(input.clone());
        } else {
            fields.sort();
            names.extend(fields);
        }
    }
    names
}

fn emit_host_structs(
    items: &mut Vec<Item>,
    declaration: &emath_ir::Declaration,
    package: &SemanticPackage,
    used: &BTreeSet<String>,
    owner: &str,
) -> Result<(), BackendError> {
    let mut emitted = BTreeSet::new();
    for input in &declaration.inputs {
        let Some(TypeNode::Opaque { name, .. }) = package.ty(input.ty) else {
            continue;
        };
        let type_name = name.leaf();
        if type_name.is_empty() || !emitted.insert(type_name.to_string()) {
            continue;
        }
        let prefix = format!("{}.", input.name);
        let fields: Vec<(String, Ty)> = used
            .iter()
            .filter_map(|name| name.strip_prefix(&prefix))
            .filter(|field| !field.is_empty() && !field.contains('.'))
            .map(|field| (field.to_string(), Ty::F64))
            .collect();
        if fields.is_empty() {
            return Err(BackendError::UnsupportedType(format!(
                "host type `{type_name}` on `{owner}` has no accessed fields"
            )));
        }
        items.push(Item::DocComment(format!(
            "Host-deferred `{type_name}`: field types inferred from uses in `{owner}`."
        )));
        let struct_name = escape_ident(type_name);
        items.push(Item::Struct(StructDef {
            name: struct_name.clone(),
            generics: vec![],
            fields: fields.clone(),
            derives: vec!["Clone".to_string(), "Debug".to_string()],
            doc: Vec::new(),
            visibility: Visibility::Public,
        }));
        items.push(Item::Impl(ImplDef {
            target: struct_name.clone(),
            generics: vec![],
            methods: vec![FnDef {
                name: "new".to_string(),
                generics: vec![],
                params: fields
                    .iter()
                    .map(|(field, ty)| Param {
                        name: field.clone(),
                        ty: ty.clone(),
                    })
                    .collect(),
                ret: Ty::Named(struct_name),
                body: Stmt::Expr(Expr::StructLiteral {
                    name: "Self".to_string(),
                    fields: fields
                        .iter()
                        .map(|(field, _)| (field.clone(), Expr::Var(field.clone())))
                        .collect(),
                }),
                doc: vec!["Construct a host-deferred record from accessed fields.".to_string()],
                visibility: Visibility::Public,
                attrs: Vec::new(),
            }],
            doc: Vec::new(),
        }));
    }
    Ok(())
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

fn rate_call(state_name: &str, input_args: &[Expr]) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(Expr::SelfValue),
        method: escape_ident(&format!("der_{state_name}")),
        args: input_args.to_vec(),
    }
}

fn rate_lets(
    receiver: &str,
    prefix: &str,
    declaration: &emath_ir::Declaration,
    input_args: &[Expr],
) -> Vec<Stmt> {
    declaration
        .state
        .iter()
        .map(|field| Stmt::Let {
            pattern: format!("{prefix}_{}", field.name),
            value: Box::new(Expr::MethodCall {
                receiver: Box::new(Expr::Var(receiver.to_string())),
                method: escape_ident(&format!("der_{}", field.name)),
                args: input_args.to_vec(),
            }),
        })
        .collect()
}

fn add_scaled_expr(value: Expr, rate: Expr, scale: Expr, node: &TypeNode) -> Expr {
    match node {
        TypeNode::Vector { .. } => Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + {} * b).collect::<Vec<f64>>()",
            render_expr(&value),
            render_expr(&rate),
            render_expr(&scale),
        )),
        TypeNode::Matrix { .. } => Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a + {} * b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&value),
            render_expr(&rate),
            render_expr(&scale),
        )),
        _ => Expr::Bin {
            op: BinOp::Add,
            left: Box::new(value),
            right: Box::new(Expr::Bin {
                op: BinOp::Mul,
                left: Box::new(scale),
                right: Box::new(rate),
            }),
        },
    }
}

// (test module relocated to tests/emath-rust-backend)
