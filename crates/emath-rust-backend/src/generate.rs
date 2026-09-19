//! The backend codegen pipeline (`BackendInput::generate`).

use super::*;
use emath_exec_ir::{EmirOp, EmirProgram};

impl BackendInput<'_> {
    /// Run the whole backend: structure + methods + tests + crate files.
    #[allow(unreachable_code, unused_variables)]
    pub fn generate(&self) -> Result<BackendOutput, BackendError> {
        return Err(BackendError::Lowering(
            "E-KIND-GONE: SIR/goals generation is not constructor surface. Emit constructor functions with emit_constructor_program.".into(),
        ));
        let package = self.package;
        let mut items: Vec<Item> = Vec::new();
        let mut anchors: Vec<BackendAnchor> = Vec::new();
        let mut assumptions: Vec<String> = Vec::new();
        let mut emitted_error_types: Vec<String> = Vec::new();
        let mut newton_helpers_emitted = false;
        let mut receipts: Vec<ConstructionReceipt> = Vec::new();

        items.push(Item::RawAttribute("#![forbid(unsafe_code)]".to_string()));
        items.push(Item::RawAttribute("#![allow(dead_code)]".to_string()));
        // The emath-runtime kernel module is embedded into every generated
        // crate so artifacts stay self-contained (no external dependency)
        // while all math kernels live in exactly one place: emath-rt.
        // Generated expressions call `emath_rt::<kernel>(...)`.
        // The outer `#[allow(dead_code)]` keeps hosts that strip `#![...]`
        // inner attributes (e.g. the demo-host `include!` driver) warning-
        // free: an outer attribute on the module survives that strip.
        items.push(Item::RawAttribute(format!(
            "#[allow(dead_code)]\npub mod emath_rt {{\n{}\n}}",
            emath_rt::SOURCE
        )));

        emit_authored_records(package, &mut items)?;
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
            let evaluate_goals: Vec<&emath_ir::Goal> = declaration
                .goals
                .iter()
                .filter_map(|goal_id| package.goals.get(goal_id.index()))
                .filter(|goal| goal.kind == GoalKind::Evaluate)
                .collect();
            let has_evaluate = !evaluate_goals.is_empty();
            // No state and no constructors: emit a free function instead of
            // a method on an empty struct — but only for a single evaluate
            // target, since the free function is named after the
            // declaration. Multiple evaluate goals (the `E-SEC-133`
            // ergonomics default: every definition evaluates) take the
            // unit-struct + per-target-method form, where each method is
            // named after its definition. A stateless declaration with
            // nothing to evaluate still keeps a unit struct so the
            // declaration name remains a Rust identifier.
            let emit_free_fn = stateless && has_evaluate && evaluate_goals.len() == 1;

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
                "Generated deterministically by emath; do not edit.".to_string(),
            ));
            if !emit_free_fn {
                let mut struct_fields: Vec<(String, Ty)> = declaration
                    .state
                    .iter()
                    .map(|field| {
                        self.rust_ty(field.ty, &name)
                            .map(|ty| (field.name.clone(), ty))
                    })
                    .collect::<Result<_, _>>()?;
                // Algebraic unknowns are part of the DAE extended state:
                // a successful `step_*` projects them so the residual at
                // the returned point is ~0.
                for field in &declaration.algebraic {
                    struct_fields.push((field.name.clone(), self.rust_ty(field.ty, &name)?));
                }
                items.push(Item::Struct(StructDef {
                    name: struct_name.clone(),
                    generics: vec![],
                    fields: struct_fields,
                    field_visibility: Visibility::Private,
                    derives: vec!["Clone".to_string(), "Debug".to_string()],
                    doc: Vec::new(),
                    visibility: Visibility::Public,
                }));
            }

            let mut methods: Vec<FnDef> = Vec::new();
            let mut evaluate_targets: Vec<String> = Vec::new();
            let mut result_eval_targets: BTreeSet<String> = BTreeSet::new();
            let input_kinds = field_value_kinds(package, declaration);

            // --- constructor ----------------------------------------------
            if declaration.constructors.len() > 1 {
                return Err(BackendError::MultipleConstructors(name));
            }
            if let Some(constructor) = declaration.constructors.first() {
                // The receipt records the exact obligation matrix the
                // emitted constructor discharges (all runtime today).
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
                        value: Box::new(value_expr(&program, &param_names, &[], &input_kinds)?),
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
                        coerce_to_ty(
                            value_expr(&program, &param_names, &[], &input_kinds)?,
                            program_kind(&program, &param_names, &[], &input_kinds),
                            &self.rust_ty(field_def.ty, &name)?,
                        ),
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
                        value: Box::new(value_expr(&program, &param_names, &[], &input_kinds)?),
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
            let goals = evaluate_goals;
            for goal in &goals {
                let target = goal.target.clone();
                // `der_*` definitions on models are rate functions owned by
                // `emit_model_step_methods` (state-typed signature). The
                // goals-omitted ergonomics default turns every definition
                // into an evaluate goal, including rates; emitting both
                // duplicates the method name (E0592) and the goal-loop
                // variant mistypes the return as f64. Skip rate targets.
                if declaration.kind_label == "model" && target.starts_with("der_") {
                    continue;
                }
                if !declaration.definitions.contains_key(&target) {
                    return Err(BackendError::UnknownTarget(target));
                }
                let order = definition_order(package, declaration);
                let Some(end) = order.iter().position(|(name, _)| *name == &target) else {
                    return Err(BackendError::UnknownTarget(target));
                };
                let chain = &order[..=end];
                // Algebraic unknowns live on `Self` (extended DAE state).
                // Bind them as locals so definitions that mention them
                // (e.g. `der_q = I`) lower as ordinary names.
                let mut available = input_names.clone();
                for field in &declaration.algebraic {
                    available.push(field.name.clone());
                }
                let inner_ret = declaration
                    .outputs
                    .iter()
                    .find(|field| field.name == target)
                    .map(|field| self.rust_output_ty(field.ty, &name))
                    .transpose()?
                    .unwrap_or(Ty::F64);
                let mut eval_kinds = input_kinds.clone();
                let mut body_stmts = Vec::new();
                let mut can_fault = false;
                if !emit_free_fn {
                    for field in &declaration.algebraic {
                        let scalar = matches!(
                            self.solve_width(
                                field.ty,
                                &name,
                                &format!("algebraic `{}`", field.name)
                            ),
                            Ok(1)
                        );
                        let from_self = Expr::Field {
                            receiver: Box::new(Expr::SelfValue),
                            field: field.name.clone(),
                        };
                        body_stmts.push(Stmt::Let {
                            pattern: escape_ident(&field.name),
                            value: Box::new(if scalar {
                                from_self
                            } else {
                                Expr::MethodCall {
                                    receiver: Box::new(from_self),
                                    method: "clone".to_string(),
                                    args: Vec::new(),
                                }
                            }),
                        });
                    }
                }
                for (def_name, def_expr) in chain {
                    let def_name = *def_name;
                    let def_expr = *def_expr;
                    let used = {
                        let mut names = BTreeSet::new();
                        collect_var_names(package, def_expr, &mut names);
                        names
                    };
                    let lowering_inputs = expand_host_inputs(&available, &used);
                    let program =
                        lower_definition(package, def_expr, &lowering_inputs, &state_names)
                            .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    let kind = program_kind(&program, &lowering_inputs, &state_names, &eval_kinds);
                    let value = value_expr(&program, &lowering_inputs, &state_names, &eval_kinds)?;
                    can_fault |= program_may_fault(&program);
                    if def_name == &target {
                        let expr = coerce_to_ty(value, kind, &inner_ret);
                        body_stmts.push(Stmt::Expr(if can_fault {
                            Expr::Call {
                                path: vec!["Ok".to_string()],
                                args: vec![expr],
                            }
                        } else {
                            expr
                        }));
                    } else {
                        let kind = refine_capability_result_kind(&program, kind);
                        eval_kinds.insert(def_name.clone(), kind);
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
                let fn_name = if emit_free_fn {
                    escape_ident(&name)
                } else {
                    escape_ident(&target)
                };
                evaluate_targets.push(target.clone());
                if can_fault {
                    result_eval_targets.insert(target.clone());
                }
                let ret = if can_fault {
                    Ty::Result {
                        ok: Box::new(inner_ret),
                        error: Box::new(Ty::Named("String".to_string())),
                    }
                } else {
                    inner_ret
                };
                let doc = if matches!(ret, Ty::I64) {
                    format!("Evaluate `{target}` (exact i64).")
                } else if can_fault {
                    format!(
                        "Evaluate `{target}` (strict-f64). Index/slice out of bounds is `Err`."
                    )
                } else {
                    format!("Evaluate `{target}` (strict-f64).")
                };
                methods.push(FnDef {
                    name: fn_name,
                    generics: vec![],
                    params,
                    ret,
                    body,
                    doc: vec![doc],
                    visibility: Visibility::Public,
                    attrs: Vec::new(),
                });
            }
            // Multiple evaluate goals per declaration are the documented
            // ergonomics default (`E-SEC-133`: every definition defaults
            // to `evaluate`), so the goal loop emits one method per
            // target; no per-declaration cap.
            drop(goals);

            if declaration.kind_label == "model" && !emit_free_fn {
                self.emit_model_step_methods(
                    package,
                    declaration,
                    &name,
                    &input_names,
                    &state_names,
                    &mut items,
                    &mut methods,
                    &mut assumptions,
                    &mut newton_helpers_emitted,
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
                let test_name = format!("{}_{}", snake_case(&name), snake_case(&test.name));
                let given_names: Vec<String> = test.given.keys().cloned().collect();
                let mut statements: Vec<Stmt> = Vec::new();
                let mut seen: Vec<String> = Vec::new();
                for given_name in &given_names {
                    let program = lower_definition(package, test.given[given_name], &seen, &[])
                        .map_err(BackendError::Lowering)?;
                    add_obligations(&program, &mut assumptions);
                    let kind = program_kind(&program, &seen, &[], &input_kinds);
                    let value = value_expr(&program, &seen, &[], &input_kinds)?;
                    let field_ty = declaration
                        .inputs
                        .iter()
                        .chain(declaration.state.iter())
                        .chain(declaration.algebraic.iter())
                        .chain(
                            declaration
                                .constructors
                                .iter()
                                .flat_map(|c| c.parameters.iter()),
                        )
                        .find(|field| &field.name == given_name)
                        .map(|field| field.ty);
                    let value = if let Some(ty) = field_ty {
                        coerce_to_ty(value, kind, &self.rust_ty(ty, &name)?)
                    } else {
                        value
                    };
                    statements.push(Stmt::Let {
                        pattern: escape_ident(given_name),
                        value: Box::new(value),
                    });
                    seen.push(given_name.clone());
                }
                if evaluate_targets.is_empty() {
                    return Err(BackendError::NoEvaluateGoal(
                        declaration.name.leaf().to_string(),
                    ));
                }
                // Every evaluate target is called and bound to its
                // definition name, so `expect` rows observe the whole
                // definition surface (the `E-SEC-133` default promises
                // `evaluate` for every definition). `actual` is rebound
                // per target; the shadowed binding below captures each
                // target's own value.
                let mut expect_names: Vec<String> = input_names.clone();
                for target in &evaluate_targets {
                    let mut eval_args: Vec<Expr> = Vec::new();
                    for input in &declaration.inputs {
                        if !given_names.contains(&input.name) {
                            return Err(BackendError::MissingInput(input.name.clone()));
                        }
                        eval_args.push(Expr::Var(escape_ident(&input.name)));
                    }
                    let eval_call = if emit_free_fn {
                        Expr::Call {
                            path: vec![escape_ident(&name)],
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
                            let mut fields = declaration
                                .state
                                .iter()
                                .map(|field| {
                                    if !given_names.contains(&field.name) {
                                        return Err(BackendError::MissingGiven(field.name.clone()));
                                    }
                                    Ok((field.name.clone(), Expr::Var(escape_ident(&field.name))))
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            for field in &declaration.algebraic {
                                if !given_names.contains(&field.name) {
                                    return Err(BackendError::MissingGiven(field.name.clone()));
                                }
                                fields.push((field.name.clone(), Expr::Var(escape_ident(&field.name))));
                            }
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
                    let eval_call = if result_eval_targets.contains(target) {
                        Expr::MethodCall {
                            receiver: Box::new(eval_call),
                            method: "expect".to_string(),
                            args: vec![Expr::Str("index in bounds".to_string())],
                        }
                    } else {
                        eval_call
                    };
                    statements.push(Stmt::Let {
                        pattern: "actual".to_string(),
                        value: Box::new(eval_call),
                    });
                    for definition in declaration.definitions.keys() {
                        if definition.starts_with("der_") {
                            continue;
                        }
                        if definition == target {
                            statements.push(Stmt::Let {
                                pattern: escape_ident(definition),
                                value: Box::new(Expr::Var("actual".to_string())),
                            });
                        }
                    }
                    if !expect_names.contains(target) {
                        expect_names.push(target.clone());
                    }
                }
                if let Some(expect) = test.expect {
                    // A `test.expect` observes the inputs plus every
                    // evaluate target (bound above). Name a definition that
                    // no evaluate goal binds (e.g. a model's `der_*` rate)
                    // and the refusal says so instead of silently binding
                    // the wrong value.
                    {
                        let mut referenced = BTreeSet::new();
                        collect_var_names(package, expect, &mut referenced);
                        for name in referenced {
                            if !expect_names.contains(&name)
                                && declaration.definitions.contains_key(&name)
                            {
                                return Err(BackendError::NoEvaluateGoal(format!(
                                    "test `{test_name}` expects `{name}`, but no evaluate goal binds it; add an `evaluate <{name}>:` goal or assert on a bound target"
                                )));
                            }
                        }
                    }
                    let mut expect_kinds = input_kinds.clone();
                    for target in &evaluate_targets {
                        if declaration
                            .outputs
                            .iter()
                            .any(|field| &field.name == target && type_is_i64(package, field.ty))
                        {
                            for definition in declaration.definitions.keys() {
                                expect_kinds.insert(definition.clone(), ValueKind::I64);
                            }
                        }
                    }
                    let expect_program =
                        lower_definition(package, expect, &expect_names, &state_names)
                            .map_err(BackendError::Lowering)?;
                    add_obligations(&expect_program, &mut assumptions);
                    // The `expect` expression is a Boolean comparison; assert it
                    // with a real macro invocation (rendered via `Expr::Macro`).
                    statements.push(Stmt::Expr(Expr::Macro {
                        name: "assert".to_string(),
                        args: vec![value_expr(
                            &expect_program,
                            &expect_names,
                            &state_names,
                            &expect_kinds,
                        )?],
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
}


/// Emit only layouts reachable through the admitted capability value ABI.
fn emit_authored_records(package: &SemanticPackage, items: &mut Vec<Item>) -> Result<(), BackendError> {
    fn collect_kind(kind: ValueKind, records: &mut BTreeSet<String>) {
        match kind {
            ValueKind::Record(name) => { records.insert(name); }
            ValueKind::Vector(element) | ValueKind::Matrix(element) => collect_kind(*element, records),
            _ => {}
        }
    }
    fn calls(program: &EmirProgram, queue: &mut Vec<String>) {
        for (op, _) in &program.ops {
            match op {
                EmirOp::ApplyCapability { capability, .. } => queue.push(capability.clone()),
                EmirOp::Iterate { body, stop, .. } => { if let Some(stop) = stop { calls(stop, queue); } calls(body, queue); }
                EmirOp::Fold { body, .. } | EmirOp::Collect { body, .. } => calls(body, queue),
                EmirOp::Branch { then_body, else_body, .. } => { calls(then_body, queue); calls(else_body, queue); }
                _ => {}
            }
        }
    }
    let mut queue = package.capabilities.iter().map(|capability| capability.name.0.clone()).collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    let mut records = BTreeSet::new();
    while let Some(capability) = queue.pop() {
        if !visited.insert(capability.clone()) { continue; }
        if let Some(signature) = emath_exec_ir::native_kernel::installed_signature(&capability) {
            for ty in signature.inputs.iter().chain(std::iter::once(&signature.output)) { collect_kind(ValueKind::from_signature(ty), &mut records); }
        }
        if let Some(cell) = emath_exec_ir::native_kernel::installed_reference_cell(&capability) { calls(&cell.program, &mut queue); }
    }
    let mut emitted = BTreeSet::new();
    while let Some(name) = records.pop_first() {
        if !emitted.insert(name.clone()) { continue; }
        let Some(layout) = emath_exec_ir::native_kernel::installed_record_layout(&name) else { continue; };
        let mut fields = Vec::with_capacity(layout.fields.len());
        for (field, ty) in layout.fields {
            let kind = ValueKind::from_signature(&ty);
            fields.push((escape_ident(&field), kind.rust_ty()?));
            collect_kind(kind, &mut records);
        }
        items.push(Item::Struct(StructDef { name: format!("EmathRecord_{}", escape_ident(&name)), generics: Vec::new(), fields, field_visibility: Visibility::Public, derives: vec!["Clone".into(), "Debug".into(), "PartialEq".into()], doc: Vec::new(), visibility: Visibility::Public }));
    }
    Ok(())
}
