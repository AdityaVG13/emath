//! Explicit-stepper code generation (Euler, RK4).

use super::*;

impl super::super::BackendInput<'_> {
    pub(crate) fn emit_model_step_methods(
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
        if declaration.state.is_empty() {
            return Ok(());
        }
        let has_residuals = package
            .residuals
            .get(&declaration.id)
            .is_some_and(|residuals| !residuals.is_empty());
        if has_residuals {
            return self.emit_newton_step_methods(
                package,
                declaration,
                owner,
                input_names,
                state_names,
                items,
                methods,
                assumptions,
                newton_helpers_emitted,
            );
        }
        let order = definition_order(package, declaration);
        let i64_names = i64_field_names(package, declaration);
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
                let value = value_expr_rate(&program, &lowering_inputs, state_names, &i64_names)?;
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
            let node = self.package.ty(field.ty).ok_or_else(|| {
                BackendError::UnsupportedType(format!("unknown state type in `{owner}`"))
            })?;
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
        statements.extend(rate_lets("k1", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s2".to_string(),
            value: Box::new(self.shifted_state(declaration, owner, "k1", &half)?),
        });
        statements.extend(rate_lets("k2", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s3".to_string(),
            value: Box::new(self.shifted_state(declaration, owner, "k2", &half)?),
        });
        statements.extend(rate_lets("k3", declaration, input_args));
        statements.push(Stmt::Let {
            pattern: "s4".to_string(),
            value: Box::new(self.shifted_state(
                declaration,
                owner,
                "k3",
                &Expr::Var("dt".to_string()),
            )?),
        });
        statements.extend(rate_lets("k4", declaration, input_args));
        let mut fields = Vec::new();
        for field in &declaration.state {
            let node = self.package.ty(field.ty).ok_or_else(|| {
                BackendError::UnsupportedType(format!("unknown state type in `{owner}`"))
            })?;
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

    pub(super) fn shifted_state(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        rate_prefix: &str,
        scale: &Expr,
    ) -> Result<Expr, BackendError> {
        self.shifted_state_with_algebraic(declaration, owner, rate_prefix, scale, &[])
    }

    pub(super) fn shifted_state_with_algebraic(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        rate_prefix: &str,
        scale: &Expr,
        algebraic_fields: &[(String, Expr)],
    ) -> Result<Expr, BackendError> {
        let mut fields = Vec::new();
        for field in &declaration.state {
            let node = self.package.ty(field.ty).ok_or_else(|| {
                BackendError::UnsupportedType(format!("unknown state type in `{owner}`"))
            })?;
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
        fields.extend(algebraic_fields.iter().cloned());
        Ok(Expr::StructLiteral {
            name: "Self".to_string(),
            fields,
        })
    }
}
