use emath_exec_ir::{definition_order, lower_definition};
use emath_ir::{Extent, SemanticPackage, TypeNode};
use emath_rust_ir::ast::{
    escape_ident, BinOp, Block, Expr, FnDef, Item, Param, Stmt, Ty, Visibility,
};
use emath_rust_ir::render::render_expr;
use std::collections::BTreeSet;

use crate::codegen_helpers::{
    add_obligations, add_scaled_expr, collect_var_names, expand_host_inputs, i64_field_names,
    rate_call, rate_lets,
};
use crate::codegen_render::{value_expr, value_expr_rate};
use crate::BackendError;

/// Interpreter parity constants for generated causalized-Newton steps
/// (`crates/emath-exec-ir/src/runner/simulate/newton.rs`). Changing any
/// of these here without changing the interpreter breaks the parity claim
/// behind the admission message "implicit residual system did not
/// converge".
const NEWTON_MAX_ITER: usize = 30;
const NEWTON_TOL: f64 = 1e-9;
const NEWTON_FINAL_TOL: f64 = 1e-6;

impl super::BackendInput<'_> {
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

    fn shifted_state(
        &self,
        declaration: &emath_ir::Declaration,
        owner: &str,
        rate_prefix: &str,
        scale: &Expr,
    ) -> Result<Expr, BackendError> {
        self.shifted_state_with_algebraic(declaration, owner, rate_prefix, scale, &[])
    }

    fn shifted_state_with_algebraic(
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
    fn emit_newton_step_methods(
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

/// Module-level helper: max-abs of a residual vector (interpreter parity
/// with `newton.rs`'s `max_abs`).
fn newton_max_abs_fn() -> FnDef {
    FnDef {
        name: "__emath_max_abs".to_string(),
        generics: vec![],
        params: vec![Param {
            name: "values".to_string(),
            ty: Ty::Ref(Box::new(Ty::Named("Vec<f64>".to_string()))),
        }],
        ret: Ty::F64,
        body: Stmt::Block(Block {
            statements: vec![Stmt::Expr(Expr::Raw(
                "values.iter().fold(0.0f64, |acc, value| acc.max(value.abs()))".to_string(),
            ))],
        }),
        doc: vec!["Generated Newton helper: max-abs of a residual vector.".to_string()],
        visibility: Visibility::Private,
        attrs: Vec::new(),
    }
}

/// Module-level helper: partial-pivot Gaussian elimination, ported
/// verbatim from the interpreter's `causal_newton` solver so generated
/// steps match `emath simulate` bit-for-bit on the same inputs.
fn newton_gaussian_solve_fn() -> FnDef {
    FnDef {
        name: "__emath_gaussian_solve".to_string(),
        generics: vec![],
        params: vec![
            Param {
                name: "matrix".to_string(),
                ty: Ty::Ref(Box::new(Ty::Named("Vec<Vec<f64>>".to_string()))),
            },
            Param {
                name: "rhs".to_string(),
                ty: Ty::Ref(Box::new(Ty::Named("Vec<f64>".to_string()))),
            },
        ],
        ret: Ty::Result {
            ok: Box::new(Ty::Named("Vec<f64>".to_string())),
            error: Box::new(Ty::Named("String".to_string())),
        },
        body: newton_raw_body(
            "let n = rhs.len();\n\
             if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {\n\
             \x20   return Err(\"Jacobian is not square\".to_string());\n\
             }\n\
             if n == 0 { return Ok(Vec::new()); }\n\
             let mut a: Vec<Vec<f64>> = matrix.clone();\n\
             let mut b: Vec<f64> = rhs.clone();\n\
             for col in 0..n {\n\
             \x20   let mut pivot = col;\n\
             \x20   let mut best = a[col][col].abs();\n\
             \x20   for row in (col + 1)..n {\n\
             \x20       let candidate = a[row][col].abs();\n\
             \x20       if candidate > best { best = candidate; pivot = row; }\n\
             \x20   }\n\
             \x20   if best < 1e-300 { return Err(format!(\"near-zero pivot in column {col}\")); }\n\
             \x20   a.swap(col, pivot);\n\
             \x20   b.swap(col, pivot);\n\
             \x20   for row in (col + 1)..n {\n\
             \x20       let factor = a[row][col] / a[col][col];\n\
             \x20       if factor == 0.0 { continue; }\n\
             \x20       for k in col..n { a[row][k] -= factor * a[col][k]; }\n\
             \x20       b[row] -= factor * b[col];\n\
             \x20   }\n\
             }\n\
             let mut x = vec![0.0f64; n];\n\
             for row in (0..n).rev() {\n\
             \x20   let mut acc = b[row];\n\
             \x20   for k in (row + 1)..n { acc -= a[row][k] * x[k]; }\n\
             \x20   x[row] = acc / a[row][row];\n\
             }\n\
             Ok(x)",
        ),
        doc: vec![
            "Generated Newton helper: Gaussian elimination with partial pivoting.".to_string(),
        ],
        visibility: Visibility::Private,
        attrs: Vec::new(),
    }
}

/// Wrap generated statements in a block expression (`{ ... }`) that can
/// serve as a function body or a `let` value with a trailing expression.
fn newton_raw_body(text: &str) -> Stmt {
    Stmt::Block(Block {
        statements: vec![Stmt::Expr(Expr::Raw(format!("{{ {text} }}")))],
    })
}

/// One stage block of a causalized Newton step: state locals, the flat
/// `__x` solve vector (unknowns in order: algebraic then rate), residual
/// closures, and the interpreter-mirroring Newton loop. The block value
/// is the flattened per-state rates vector in state order.
fn newton_stage_text(
    receiver: &str,
    state_names: &[String],
    state_widths: &[usize],
    algebraic: &[emath_ir::Field],
    rate_names: &[String],
    unknown_widths: &[usize],
    algebraic_width_total: usize,
    residual_sources: &[(u16, String)],
    def_sources: &[(String, String)],
) -> String {
    let mut out = String::new();
    // State locals: `st_<state>` reads with the stage receiver, so the
    // residual closures and definition lets are pure-of-`self` and can be
    // re-emitted for each shifted stage.
    for (index, name) in state_names.iter().enumerate() {
        let scalar = state_widths[index] == 1;
        out.push_str(&format!(
            "let st_{name} = ({receiver}).{name}{};\n",
            if scalar { "" } else { ".clone()" }
        ));
    }
    // Flat solve vector. Algebraic guesses come from the stage receiver
    // (extended DAE state), not from extra step parameters.
    out.push_str("let mut __x: Vec<f64> = Vec::new();\n");
    let mut slot = 0usize;
    for field in algebraic {
        let width = unknown_widths[slot];
        let name = escape_ident(&field.name);
        if width == 1 {
            out.push_str(&format!("__x.push(({receiver}).{name});\n"));
        } else {
            out.push_str(&format!("__x.extend(({receiver}).{name}.clone());\n"));
        }
        slot += 1;
    }
    for width in &unknown_widths[algebraic.len()..] {
        if *width == 1 {
            out.push_str("__x.push(0.0);\n");
        } else {
            out.push_str(&format!("__x.extend(vec![0.0; {width}]);\n"));
        }
    }
    // Residual closures.
    for (index, (components, source)) in residual_sources.iter().enumerate() {
        let ret = if *components == 1 { "f64" } else { "Vec<f64>" };
        out.push_str(&format!(
            "let __r{index} = |x: &[f64]| -> {ret} {{ {source} }};\n"
        ));
    }
    // F assembly (mirrors `eval_residuals`).
    out.push_str("let __eval = |x: &[f64]| -> Vec<f64> {\n");
    out.push_str("    let mut out = Vec::new();\n");
    for (index, (components, _)) in residual_sources.iter().enumerate() {
        if *components == 1 {
            out.push_str(&format!("    out.push(__r{index}(x));\n"));
        } else {
            out.push_str(&format!("    out.extend(__r{index}(x));\n"));
        }
    }
    out.push_str("    out\n};\n");
    // Bring the interpreter's constants into scope by literal, not by
    // reference: generated crates are self-contained.
    out.push_str(&format!(
        "let mut __f = __eval(&__x);\n\
         let mut __converged = __emath_max_abs(&__f) < {NEWTON_TOL};\n\
         for _ in 0..{NEWTON_MAX_ITER}u32 {{\n\
         \x20   if __converged {{ break; }}\n\
         \x20   let n = __x.len();\n\
         \x20   let mut __jac = vec![vec![0.0f64; n]; __f.len()];\n\
         \x20   for col in 0..n {{\n\
         \x20       let h = 1e-7 * (1.0 + __x[col].abs());\n\
         \x20       let saved = __x[col];\n\
         \x20       __x[col] += h;\n\
         \x20       let __plus = __eval(&__x);\n\
         \x20       for (row, value) in __plus.iter().enumerate() {{\n\
         \x20           __jac[row][col] = (value - __f[row]) / h;\n\
         \x20       }}\n\
         \x20       __x[col] = saved;\n\
         \x20   }}\n\
         \x20   let __delta = match __emath_gaussian_solve(&__jac, &__f) {{\n\
         \x20       Ok(delta) => delta,\n\
         \x20       Err(message) => {{\n\
         \x20           return Err(format!(\"implicit residual Jacobian is singular ({{message}}); check that the residual equations are independent\"));\n\
         \x20       }}\n\
         \x20   }};\n\
         \x20   for (index, step) in __delta.iter().enumerate() {{\n\
         \x20       __x[index] -= *step;\n\
         \x20   }}\n\
         \x20   __f = __eval(&__x);\n\
         \x20   let __scale = __x.iter().fold(1.0f64, |acc, value| acc.max(value.abs()));\n\
         \x20   __converged = __emath_max_abs(&__f) < {NEWTON_TOL} || __emath_max_abs(&__delta) < 1e-12 * (1.0 + __scale);\n\
         }}\n\
         if __emath_max_abs(&__f) > {NEWTON_FINAL_TOL} {{\n\
         \x20   return Err(format!(\"implicit residual system did not converge within {NEWTON_MAX_ITER} Newton iterations (max residual {{:.3e}}); check the model equations and `algebraic:` guesses\", __emath_max_abs(&__f)));\n\
         }}\n"
    ));
    // Definition lets (see solved algebraic values).
    for (name, source) in def_sources {
        out.push_str(&format!("let {} = {source};\n", escape_ident(name)));
    }
    // Flattened per-state rates in state order.
    out.push_str("let mut __rates: Vec<f64> = Vec::new();\n");
    let mut rate_start = algebraic_width_total;
    for name in state_names {
        if let Some(rate_index) = rate_names.iter().position(|rate| rate == name) {
            let width = unknown_widths[algebraic.len() + rate_index];
            if width == 1 {
                out.push_str(&format!("__rates.push(__x[{rate_start}]);\n"));
            } else {
                out.push_str(&format!(
                    "__rates.extend(__x[{rate_start}..{}+{width}].to_vec());\n",
                    rate_start
                ));
            }
            rate_start += width;
        } else {
            // Explicit rate definition: the chain above bound it as a let
            // (`der_<state>`), and the solved algebraic values are inside
            // `__x`, so the definition reads them exactly like the
            // interpreter's "solve then re-evaluate definitions" step.
            out.push_str(&format!(
                "__rates.push({});\n",
                escape_ident(&format!("der_{name}"))
            ));
        }
    }
    out.push_str("let mut __alg: Vec<f64> = Vec::new();\n");
    if algebraic_width_total > 0 {
        out.push_str(&format!(
            "__alg.extend(__x[..{algebraic_width_total}].iter().copied());\n"
        ));
    }
    out.push_str("(__rates, __alg)\n");
    out
}

/// Unpack flattened algebraic components from `__{prefix}_alg` into
/// named locals `{prefix}_{field}` (scalar or `Vec<f64>`).
fn newton_unpack_lets(
    prefix: &str,
    kind: &str,
    fields: &[emath_ir::Field],
    widths: &[usize],
) -> Vec<Stmt> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let width = widths[index];
        let src = if width == 1 {
            format!("__{prefix}_{kind}[{offset}]")
        } else {
            format!("__{prefix}_{kind}[{offset}..{offset}+{width}].to_vec()")
        };
        out.push(Stmt::Let {
            pattern: format!("{prefix}_{}", escape_ident(&field.name)),
            value: Box::new(Expr::Raw(src)),
        });
        offset += width;
    }
    out
}

fn newton_alg_field_exprs(prefix: &str, algebraic: &[emath_ir::Field]) -> Vec<(String, Expr)> {
    algebraic
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                Expr::Var(format!("{prefix}_{}", escape_ident(&field.name))),
            )
        })
        .collect()
}

/// `(state, rate read)` pairs for one RK stage, in state order:
/// `__k1_rates[<off>]` for scalar states and
/// `__k1_rates[<off>..<off>+<width>]` for vector states (widths are
/// static, per admission).
fn newton_rate_args(
    prefix: &str,
    state_names: &[String],
    state_widths: &[usize],
) -> Vec<(String, String)> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for (index, name) in state_names.iter().enumerate() {
        let width = state_widths[index];
        let read = if width == 1 {
            format!("__{prefix}_rates[{offset}]")
        } else {
            format!("__{prefix}_rates[{offset}..{}+{width}]", offset)
        };
        out.push((name.clone(), read));
        offset += width;
    }
    out
}

/// Rewrite `self.<state>` field accesses to `st_<state>` so a rendered
/// expression can be re-emitted inside a stage block that holds state
/// locals instead of `self`. Longest names first with an identifier
/// boundary guard, so `q` cannot corrupt `q2`.
fn replace_state_receiver(source: &str, state_names: &[String]) -> String {
    let mut ordered: Vec<&String> = state_names.iter().collect();
    ordered.sort_by_key(|name| std::cmp::Reverse(name.len()));
    let mut out = source.to_string();
    for name in ordered {
        let needle = format!("self.{name}");
        let mut rebuilt = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find(&needle) {
            rebuilt.push_str(&rest[..pos]);
            rebuilt.push_str(&format!("st_{name}"));
            let after = &rest[pos + needle.len()..];
            if let Some(next) = after.chars().next() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    // A longer state name owns this span; skip past the
                    // self. prefix and let the longer name match next.
                    rebuilt.push_str("self.");
                    rest = &rest[pos + "self.".len()..];
                    continue;
                }
            }
            rest = after;
        }
        rebuilt.push_str(rest);
        out = rebuilt;
    }
    out
}
