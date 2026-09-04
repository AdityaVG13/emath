//! Newton-step code generation methods.

use super::*;

impl super::super::BackendInput<'_> {
    /// Emit `step_euler` / `step_rk4` for a causalized implicit-residual
    /// model. Each step embeds the same Newton solve the interpreter runs
    /// (`causal_newton` in `emath-exec-ir`): the residual lowerings are
    /// rewritten so their load-input slots read from a flat `__x` vector
    /// (`__x[k]` slot names), forward-difference Jacobian, partial-pivot
    /// Gaussian elimination, 30-iteration budget and the two convergence
    /// tests are mirrored with the interpreter's constants, and
    /// non-convergence returns a typed `Result` error instead of a silent
    /// approximation. Explicit rate definitions re-evaluate against the
    /// solved algebraic values (the interpreter's "solve then re-evaluate
    /// definitions" step).
    pub(super) fn emit_newton_step_methods(
        &self,
        package: &SemanticPackage,
        declaration: &emath_ir::Declaration,
        owner: &str,
        input_names: &[String],
        state_names: &[String],
        items: &mut Vec<Item>,
        methods: &mut Vec<FnDef>,
        assumptions: &mut Vec<String>,
        newton_helpers_emitted: &mut bool,
    ) -> Result<(), BackendError> {
        if !*newton_helpers_emitted {
            *newton_helpers_emitted = true;
            items.push(Item::Fn(newton_max_abs_fn()));
            items.push(Item::Fn(newton_gaussian_solve_fn()));
        }
        let i64_names = i64_field_names(package, declaration);

        let residuals: Vec<emath_ir::ModelResidual> = package
            .residuals
            .get(&declaration.id)
            .cloned()
            .unwrap_or_default();
        let algebraic = declaration.algebraic.clone();
        let mut rate_names: Vec<String> = Vec::new();
        for residual in &residuals {
            for rate in &residual.rates {
                if !rate_names.iter().any(|name| name == rate) {
                    rate_names.push(rate.clone());
                }
            }
        }

        // Static widths of the flattened solve vector: algebraic unknowns
        // first, then rate unknowns (admission requires fixed extents, so
        // every offset is a compile-time constant).
        let mut unknown_widths: Vec<usize> = Vec::new();
        for field in &algebraic {
            unknown_widths.push(self.solve_width(
                field.ty,
                owner,
                &format!("algebraic `{}`", field.name),
            )?);
        }
        for rate in &rate_names {
            let state_field = declaration
                .state
                .iter()
                .find(|field| &field.name == rate)
                .ok_or_else(|| {
                    BackendError::Lowering(format!("rate unknown `{rate}` has no state field"))
                })?;
            unknown_widths.push(self.solve_width(
                state_field.ty,
                owner,
                &format!("rate `der({rate})`"),
            )?);
        }
        let algebraic_width_total: usize = unknown_widths[..algebraic.len()].iter().sum();
        let state_widths: Vec<usize> = declaration
            .state
            .iter()
            .map(|field| self.solve_width(field.ty, owner, &format!("state `{}`", field.name)))
            .collect::<Result<_, _>>()?;

        // Lowering names must match the residual variables by name
        // (`I`, `__rate_<state>`). Render names rewrite the unknown slots:
        // residual sources sit inside a closure taking `x: &[f64]`, so
        // they index the closure parameter (`x[k]`), while definition
        // lets sit in the stage scope and index the outer solve vector
        // (`__x[k]`) — same slots, same order.
        let mut bind_lower: Vec<String> = input_names.to_vec();
        for field in &algebraic {
            bind_lower.push(field.name.clone());
        }
        for rate in &rate_names {
            bind_lower.push(format!("__rate_{rate}"));
        }
        let mut bind_render_residual: Vec<String> = input_names.to_vec();
        for slot in 0..unknown_widths.len() {
            bind_render_residual.push(format!("x[{slot}]"));
        }

        let mut residual_sources: Vec<(u16, String)> = Vec::new();
        for residual in &residuals {
            let program = lower_definition(package, residual.expr, &bind_lower, state_names)
                .map_err(BackendError::Lowering)?;
            add_obligations(&program, assumptions);
            let rendered = value_expr(&program, &bind_render_residual, state_names, &i64_names)?;
            let source = replace_state_receiver(&render_expr(&rendered), state_names);
            residual_sources.push((residual.components, source));
        }

        // Explicit-rate definitions, rendered once with the algebraic
        // slots bound to `__x[k]`; each stage block re-binds them as lets
        // so the solved algebraic values feed the rates.
        let order = definition_order(package, declaration);
        let mut def_sources: Vec<(String, String)> = Vec::new();
        let mut available: Vec<String> = input_names.to_vec();
        for field in &algebraic {
            available.push(field.name.clone());
        }
        for (def_name, def_expr) in order {
            let used = {
                let mut names = BTreeSet::new();
                collect_var_names(package, def_expr, &mut names);
                names
            };
            let lowering_inputs = expand_host_inputs(&available, &used);
            let render_inputs: Vec<String> = lowering_inputs
                .iter()
                .map(|name| {
                    if let Some(index) = algebraic.iter().position(|field| &field.name == name) {
                        format!("__x[{index}]")
                    } else {
                        name.clone()
                    }
                })
                .collect();
            let program = lower_definition(package, def_expr, &lowering_inputs, state_names)
                .map_err(BackendError::Lowering)?;
            add_obligations(&program, assumptions);
            let rendered = value_expr(&program, &render_inputs, state_names, &i64_names)?;
            let source = replace_state_receiver(&render_expr(&rendered), state_names);
            def_sources.push((def_name.to_string(), source));
            available.push(def_name.to_string());
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
        let stage = |receiver: &str| {
            newton_stage_text(
                receiver,
                state_names,
                &state_widths,
                &algebraic,
                &rate_names,
                &unknown_widths,
                algebraic_width_total,
                &residual_sources,
                &def_sources,
            )
        };
        let expected_rate_width: usize = state_widths.iter().sum();
        let alg_unpack = |prefix: &str| {
            newton_unpack_lets(
                prefix,
                "alg",
                &algebraic,
                &unknown_widths[..algebraic.len()],
            )
        };

        let euler_rates = newton_rate_args("k1", state_names, &state_widths);
        let mut euler_statements = vec![
            Stmt::Let {
                pattern: "(__k1_rates, __k1_alg)".to_string(),
                value: Box::new(Expr::Raw(format!("{{ {} }}", stage("self")))),
            },
            Stmt::Expr(Expr::Raw(format!(
                "if __k1_rates.len() != {expected_rate_width} {{ return Err(\"internal: Newton rate vector has the wrong width\".to_string()); }}"
            ))),
        ];
        euler_statements.extend(alg_unpack("k1"));
        euler_statements.push(Stmt::Let {
            pattern: "__advanced".to_string(),
            value: Box::new(self.newton_combine_expr(
                declaration,
                owner,
                &euler_rates,
                &newton_alg_field_exprs("k1", &algebraic),
            )?),
        });
        euler_statements.push(Stmt::Let {
            pattern: "(__proj_rates, __proj_alg)".to_string(),
            value: Box::new(Expr::Raw(format!("{{ {} }}", stage("__advanced")))),
        });
        euler_statements.extend(alg_unpack("proj"));
        euler_statements.push(Stmt::Expr(Expr::Call {
            path: vec!["Ok".to_string()],
            args: vec![self.newton_projected_expr(
                declaration,
                owner,
                "__advanced",
                &newton_alg_field_exprs("proj", &algebraic),
            )?],
        }));
        methods.push(FnDef {
            name: "step_euler".to_string(),
            generics: vec![],
            params: step_params.clone(),
            ret: Ty::Result {
                ok: Box::new(Ty::SelfType),
                error: Box::new(Ty::Named("String".to_string())),
            },
            body: Stmt::Block(Block {
                statements: euler_statements,
            }),
            doc: vec![
                "Forward Euler step from a causalized Newton solve; algebraic unknowns are re-solved at the new state so the residual is ~0.".to_string(),
            ],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });

        let half = Expr::Bin {
            op: BinOp::Div,
            left: Box::new(Expr::Var("dt".to_string())),
            right: Box::new(Expr::F64(2.0_f64.to_bits())),
        };
        let dt_var = Expr::Var("dt".to_string());

        // RK4: four Newton rate evaluations, each against the stage's
        // shifted state (k1 on `self`, k2/k3 on half-step states, k4 on
        // the full-step state), then the classic 1/6-2/6-2/6-1/6 weighted
        // combine. Exact parallel of the explicit-rate `step_rk4`:
        //   s2 = self + dt/2*k1;  s3 = self + dt/2*k2;  s4 = self + dt*k3
        //   next = self + dt*(k1/6 + k2/3 + k3/3 + k4/6)
        let mut rk4_statements = Vec::new();
        let mut previous: String = "self".to_string();
        for (index, prefix) in ["k1", "k2", "k3", "k4"].iter().enumerate() {
            let receiver = if index == 0 { "self" } else { &previous };
            rk4_statements.push(Stmt::Let {
                pattern: format!("(__{prefix}_rates, __{prefix}_alg)"),
                value: Box::new(Expr::Raw(format!("{{ {} }}", stage(receiver)))),
            });
            rk4_statements.push(Stmt::Expr(Expr::Raw(format!(
                "if __{prefix}_rates.len() != {expected_rate_width} {{ return Err(\"internal: Newton rate vector has the wrong width\".to_string()); }}"
            ))));
            rk4_statements.extend(alg_unpack(prefix));
            for (field_name, rate) in newton_rate_args(prefix, state_names, &state_widths) {
                let rate_let = format!("{prefix}_{field_name}");
                rk4_statements.push(Stmt::Let {
                    pattern: rate_let.clone(),
                    value: Box::new(Expr::Raw(rate)),
                });
            }
            let alg_fields = newton_alg_field_exprs(prefix, &algebraic);
            if index == 0 {
                let s2_shift = self.shifted_state_with_algebraic(
                    declaration,
                    owner,
                    "k1",
                    &half,
                    &alg_fields,
                )?;
                rk4_statements.push(Stmt::Let {
                    pattern: "s2".to_string(),
                    value: Box::new(s2_shift),
                });
                previous = "s2".to_string();
            } else if index == 1 {
                let s3_shift = self.shifted_state_with_algebraic(
                    declaration,
                    owner,
                    "k2",
                    &half,
                    &alg_fields,
                )?;
                rk4_statements.push(Stmt::Let {
                    pattern: "s3".to_string(),
                    value: Box::new(s3_shift),
                });
                previous = "s3".to_string();
            } else if index == 2 {
                let s4_shift = self.shifted_state_with_algebraic(
                    declaration,
                    owner,
                    "k3",
                    &dt_var,
                    &alg_fields,
                )?;
                rk4_statements.push(Stmt::Let {
                    pattern: "s4".to_string(),
                    value: Box::new(s4_shift),
                });
                previous = "s4".to_string();
            }
        }
        // Classic RK4 weighted combine per state field:
        // `self.<f> + dt*(1/6*k1_<f> + 1/3*k2_<f> + 1/3*k3_<f> + 1/6*k4_<f>)`.
        let mut rk4_fields = Vec::new();
        for field in &declaration.state {
            let node = self.package.ty(field.ty).ok_or_else(|| {
                BackendError::UnsupportedType(format!("unknown state type in `{owner}`"))
            })?;
            let mut combined = Expr::Field {
                receiver: Box::new(Expr::SelfValue),
                field: field.name.clone(),
            };
            for (stage_name, weight) in [
                ("k1", 1.0_f64 / 6.0),
                ("k2", 1.0_f64 / 3.0),
                ("k3", 1.0_f64 / 3.0),
                ("k4", 1.0_f64 / 6.0),
            ] {
                let scale = Expr::Bin {
                    op: BinOp::Mul,
                    left: Box::new(Expr::F64(weight.to_bits())),
                    right: Box::new(Expr::Var("dt".to_string())),
                };
                combined = add_scaled_expr(
                    combined,
                    Expr::Var(format!("{stage_name}_{}", field.name)),
                    scale,
                    node,
                );
            }
            rk4_fields.push((field.name.clone(), combined));
        }
        // Carry k4's algebraic guess into the combined state, then project
        // at the accepted point so the returned residual is ~0.
        rk4_fields.extend(newton_alg_field_exprs("k4", &algebraic));
        rk4_statements.push(Stmt::Let {
            pattern: "__advanced".to_string(),
            value: Box::new(Expr::StructLiteral {
                name: "Self".to_string(),
                fields: rk4_fields,
            }),
        });
        rk4_statements.push(Stmt::Let {
            pattern: "(__proj_rates, __proj_alg)".to_string(),
            value: Box::new(Expr::Raw(format!("{{ {} }}", stage("__advanced")))),
        });
        rk4_statements.extend(alg_unpack("proj"));
        rk4_statements.push(Stmt::Expr(Expr::Call {
            path: vec!["Ok".to_string()],
            args: vec![self.newton_projected_expr(
                declaration,
                owner,
                "__advanced",
                &newton_alg_field_exprs("proj", &algebraic),
            )?],
        }));
        methods.push(FnDef {
            name: "step_rk4".to_string(),
            generics: vec![],
            params: step_params,
            ret: Ty::Result {
                ok: Box::new(Ty::SelfType),
                error: Box::new(Ty::Named("String".to_string())),
            },
            body: Stmt::Block(Block {
                statements: rk4_statements,
            }),
            doc: vec![
                "Classic RK4 step from four causalized Newton rate evaluations; algebraic unknowns are re-solved at the accepted state so the residual is ~0.".to_string(),
            ],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });
        Ok(())
    }

    /// Width of one solve-vector unknown: 1 for a scalar, the fixed extent
    /// for a vector. Admission already enforces the scalar / fixed-vector
    /// restriction, so anything else here is an internal inconsistency.
    pub(crate) fn solve_width(
        &self,
        ty: emath_ir::TypeId,
        owner: &str,
        what: &str,
    ) -> Result<usize, BackendError> {
        let Some(node) = self.package.ty(ty) else {
            return Err(BackendError::UnsupportedType(format!(
                "unknown type id for {what} in `{owner}`"
            )));
        };
        self.solve_width_node(node, owner, what)
    }

    fn solve_width_node(
        &self,
        node: &TypeNode,
        owner: &str,
        what: &str,
    ) -> Result<usize, BackendError> {
        match node {
            TypeNode::Float64 | TypeNode::Nat | TypeNode::Int => Ok(1),
            TypeNode::Refinement { base, .. } => self.solve_width_node(base, owner, what),
            TypeNode::Vector {
                extent: Some(Extent::Fixed(n)),
                ..
            } => Ok(*n),
            other => Err(BackendError::UnsupportedType(format!(
                "{what} in `{owner}` must be a Float64 scalar or fixed-length vector, found {}",
                other.display_name()
            ))),
        }
    }

    /// The advanced state of a Newton Euler step: differential fields
    /// `self.<state> + dt * <rate>`, algebraic fields from the stage
    /// solve (used as the projection guess).
    fn newton_combine_expr(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        rates: &[(String, String)],
        algebraic_fields: &[(String, Expr)],
    ) -> Result<Expr, BackendError> {
        let mut fields = Vec::new();
        for (field_name, rate_var) in rates {
            let field = declaration
                .state
                .iter()
                .find(|field| &field.name == field_name)
                .ok_or_else(|| {
                    BackendError::Lowering(format!("rate `{field_name}` has no state field"))
                })?;
            let node = self.package.ty(field.ty).ok_or_else(|| {
                BackendError::UnsupportedType(format!("unknown state type in `{owner}`"))
            })?;
            fields.push((
                field_name.clone(),
                add_scaled_expr(
                    Expr::Field {
                        receiver: Box::new(Expr::SelfValue),
                        field: field_name.clone(),
                    },
                    Expr::Raw(rate_var.clone()),
                    Expr::Var("dt".to_string()),
                    node,
                ),
            ));
        }
        fields.extend(algebraic_fields.iter().cloned());
        Ok(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        })
    }

    /// Copy differential fields from the advanced state and overwrite
    /// algebraic fields with the projection solve.
    fn newton_projected_expr(
        &self,
        declaration: &emath_ir::Declaration,
        _owner: &str,
        advanced: &str,
        algebraic_fields: &[(String, Expr)],
    ) -> Result<Expr, BackendError> {
        let mut fields = Vec::new();
        for field in &declaration.state {
            let scalar = self.solve_width(field.ty, "proj", &field.name)? == 1;
            let from = Expr::Field {
                receiver: Box::new(Expr::Var(advanced.to_string())),
                field: field.name.clone(),
            };
            fields.push((
                field.name.clone(),
                if scalar {
                    from
                } else {
                    Expr::MethodCall {
                        receiver: Box::new(from),
                        method: "clone".to_string(),
                        args: Vec::new(),
                    }
                },
            ));
        }
        fields.extend(algebraic_fields.iter().cloned());
        Ok(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        })
    }
}
