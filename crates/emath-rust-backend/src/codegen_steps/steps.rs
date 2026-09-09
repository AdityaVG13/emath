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
        let input_kinds = field_value_kinds(package, declaration);
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
                let value = value_expr_rate(&program, &lowering_inputs, state_names, &input_kinds)?;
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

        methods.push(FnDef {
            name: "step_euler".to_string(),
            generics: vec![],
            params: step_params.clone(),
            ret: Ty::SelfType,
            body: self.authored_step_body(package, declaration, input_names, state_names, false)?,
            doc: vec!["Forward Euler step from explicit `der_<state>` rates.".to_string()],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });
        methods.push(FnDef {
            name: "step_rk4".to_string(),
            generics: vec![],
            params: step_params,
            ret: Ty::SelfType,
            body: self.authored_step_body(package, declaration, input_names, state_names, true)?,
            doc: vec!["Classic RK4 step from explicit `der_<state>` rates.".to_string()],
            visibility: Visibility::Public,
            attrs: Vec::new(),
        });
        Ok(())
    }

    fn authored_step_body(
        &self,
        package: &SemanticPackage,
        declaration: &emath_ir::Declaration,
        input_names: &[String],
        state_names: &[String],
        rk4: bool,
    ) -> Result<Stmt, BackendError> {
        let program = emath_exec_ir::runner::explicit_step_program(package, declaration, rk4, true)
            .map_err(BackendError::Lowering)?;
        let mut names = input_names.to_vec();
        names.push("dt".into());
        let mut kinds = field_value_kinds(package, declaration);
        kinds.insert("dt".into(), crate::codegen_render::ValueKind::F64);
        let value = value_expr_rate(&program, &names, state_names, &kinds)?;
        let mut fields = Vec::new();
        for field in &declaration.state {
            use crate::codegen_render::ValueKind;
            let name = escape_ident(&field.name);
            let (length, value): (String, String) = match kinds.get(&field.name) {
                Some(ValueKind::F64) => ("1usize".into(), "__model_data[__model_offset]".into()),
                Some(ValueKind::Vector(element)) if **element == ValueKind::F64 => (
                    format!("self.{name}.len()"),
                    "__model_data[__model_offset..__model_end].to_vec()".into(),
                ),
                Some(ValueKind::Matrix(element)) if **element == ValueKind::F64 => (
                    format!("self.{name}.as_slice().len()"),
                    format!(
                        "emath_rt::Matrix::new(self.{name}.rows(), self.{name}.cols(), __model_data[__model_offset..__model_end].to_vec()).expect(\"model step storage must match its matrix shape\")"
                    ),
                ),
                Some(ValueKind::Tensor) => (
                    format!("self.{name}.data.len()"),
                    format!(
                        "emath_rt::Tensor {{ shape: self.{name}.shape.clone(), data: __model_data[__model_offset..__model_end].to_vec() }}"
                    ),
                ),
                _ => {
                    return Err(BackendError::UnsupportedType(format!(
                        "model state \x60{}\x60 must have Float64 storage",
                        field.name
                    )));
                }
            };
            fields.push((field.name.clone(), Expr::Raw(format!(
                "{{ let __model_end = __model_offset.checked_add({length}).expect(\"model state length exceeds usize\"); let value = {value}; __model_offset = __model_end; value }}"
            ))));
        }
        Ok(Stmt::Block(Block { statements: vec![
            Stmt::Let { pattern: "__model_data".into(), value: Box::new(value) },
            Stmt::Let { pattern: "mut __model_offset".into(), value: Box::new(Expr::Raw("0usize".into())) },
            Stmt::Let { pattern: "__model_next".into(), value: Box::new(Expr::StructLiteral { name: "Self".into(), fields }) },
            Stmt::Expr(Expr::Raw("assert_eq!(__model_offset, __model_data.len(), \"model step storage does not match state\")".into())),
            Stmt::Expr(Expr::Var("__model_next".into())),
        ] }))
    }
}
