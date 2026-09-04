//! The backend codegen pipeline (`BackendInput::generate`).

use super::*;

impl BackendInput<'_> {
    /// Run the whole backend: structure + methods + tests + crate files.
    pub fn generate(&self) -> Result<BackendOutput, BackendError> {
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
                    derives: vec!["Clone".to_string(), "Debug".to_string()],
                    doc: Vec::new(),
                    visibility: Visibility::Public,
                }));
            }

            let mut methods: Vec<FnDef> = Vec::new();
            let mut evaluate_targets: Vec<String> = Vec::new();
            let mut result_eval_targets: BTreeSet<String> = BTreeSet::new();
            let i64_names = i64_field_names(package, declaration);

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
                        value: Box::new(value_expr(&program, &param_names, &[], &i64_names)?),
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
                            value_expr(&program, &param_names, &[], &i64_names)?,
                            program_kind(&program, &param_names, &[], &i64_names),
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
                        value: Box::new(value_expr(&program, &param_names, &[], &i64_names)?),
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
                let mut eval_i64 = i64_names.clone();
                let mut body_stmts = Vec::new();
                let mut index_fault = false;
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
                    let kind = program_kind(&program, &lowering_inputs, &state_names, &eval_i64);
                    let value = value_expr(&program, &lowering_inputs, &state_names, &eval_i64)?;
                    index_fault |= program_may_index_fault(&program);
                    if def_name == &target {
                        let expr = coerce_to_ty(value, kind, &inner_ret);
                        body_stmts.push(Stmt::Expr(if index_fault {
                            Expr::Call {
                                path: vec!["Ok".to_string()],
                                args: vec![expr],
                            }
                        } else {
                            expr
                        }));
                    } else {
                        if kind == ScalarKind::I64 {
                            eval_i64.insert(def_name.clone());
                        }
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
                if index_fault {
                    result_eval_targets.insert(target.clone());
                }
                let ret = if index_fault {
                    Ty::Result {
                        ok: Box::new(inner_ret),
                        error: Box::new(Ty::Named("String".to_string())),
                    }
                } else {
                    inner_ret
                };
                let doc = if matches!(ret, Ty::I64) {
                    format!("Evaluate `{target}` (exact i64).")
                } else if index_fault {
                    format!(
                        "Evaluate `{target}` (strict-f64, Phase 1). Index/slice out of bounds is `Err`."
                    )
                } else {
                    format!("Evaluate `{target}` (strict-f64, Phase 1).")
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
                    let kind = program_kind(&program, &seen, &[], &i64_names);
                    let value = value_expr(&program, &seen, &[], &i64_names)?;
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
                // `actual` holds the evaluate goal's target value and
                // nothing else. Binding every definition name to it made
                // `expect` expressions silently compare the target's value
                // under a different name. Bind only the target itself; a
                // test that needs another definition's value is outside the
                // Phase 1 surface (one evaluate goal per declaration) and
                // must say so instead of lying.
                let mut expect_names: Vec<String> = input_names.clone();
                if !expect_names.contains(target) {
                    expect_names.push(target.clone());
                }
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
                if let Some(expect) = test.expect {
                    // A `test.expect` observes exactly what the evaluate
                    // goal returns: inputs and the target. Name another
                    // definition here and the old generator silently bound
                    // the target's value to it; reject that with the fix
                    // the author actually needs.
                    {
                        let mut referenced = BTreeSet::new();
                        collect_var_names(package, expect, &mut referenced);
                        for name in referenced {
                            if !expect_names.contains(&name)
                                && declaration.definitions.contains_key(&name)
                            {
                                return Err(BackendError::NoEvaluateGoal(format!(
                                    "test `{test_name}` expects `{name}`, but Phase 1 tests observe only the evaluate target `{target}`; assert on `{target}` or promote `{name}` to the `goals:` target"
                                )));
                            }
                        }
                    }
                    let mut expect_i64 = i64_names.clone();
                    if declaration
                        .outputs
                        .iter()
                        .any(|field| &field.name == target && type_is_i64(package, field.ty))
                    {
                        for definition in declaration.definitions.keys() {
                            expect_i64.insert(definition.clone());
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
                            &expect_i64,
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
