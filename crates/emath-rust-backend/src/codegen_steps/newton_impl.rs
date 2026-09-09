//! Newton-step code generation for causalized implicit-residual models.
//!
//! Generated `step_euler` and `step_rk4` mirror the interpreter by
//! rendering the SHARED static residual step program
//! (`emath_exec_ir::runner::residual_model_step_program`): the authored
//! residual-Newton cell performs the Jacobian, elimination, and
//! convergence math, the authored `model-explicit-step` capsule
//! performs the stage formulas, and this module unpacks the returned
//! storage into the model struct. No Newton or Gaussian math is emitted
//! here or in newton_fns.

use super::*;

impl super::super::BackendInput<'_> {
    /// Emit `step_euler` and `step_rk4` for a causalized
    /// implicit-residual model from the shared static step program.
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
        let _ = (items, newton_helpers_emitted, assumptions);
        let residuals: Vec<emath_ir::ModelResidual> = package
            .residuals
            .get(&declaration.id)
            .cloned()
            .unwrap_or_default();
        for (name, rk4) in [("step_euler", false), ("step_rk4", true)] {
            let program = emath_exec_ir::runner::residual_model_step_program(
                package,
                declaration,
                &residuals,
                rk4,
            )
            .map_err(BackendError::Lowering)?;
            let mut names = input_names.to_vec();
            names.push("dt".to_string());
            let mut states = state_names.to_vec();
            for field in &declaration.algebraic {
                states.push(field.name.clone());
            }
            let body = self.render_residual_step_body(
                package,
                declaration,
                owner,
                &program,
                &names,
                &states,
            )?;
            let doc = format!(
                "{} step from the authored residual-Newton solve and authored stage formulas",
                if rk4 { "Classic RK4" } else { "Forward Euler" }
            );
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
            params.push(Param {
                name: "dt".to_string(),
                ty: Ty::F64,
            });
            methods.push(FnDef {
                name: name.to_string(),
                generics: vec![],
                params,
                ret: Ty::Result {
                    ok: Box::new(Ty::SelfType),
                    error: Box::new(Ty::Named("String".to_string())),
                },
                body: Stmt::Block(Block {
                    statements: vec![body],
                }),
                doc: vec![doc],
                visibility: Visibility::Public,
                attrs: Vec::new(),
            });
        }
        Ok(())
    }
}

impl super::super::BackendInput<'_> {
    /// Render one residual step body: evaluate the shared step program
    /// and rebuild Self from the returned differential and algebraic
    /// storage (state fields first, then algebraic fields, both in
    /// declaration order).
    fn render_residual_step_body(
        &self,
        package: &SemanticPackage,
        declaration: &emath_ir::Declaration,
        owner: &str,
        program: &emath_exec_ir::EmirProgram,
        names: &[String],
        states: &[String],
    ) -> Result<Stmt, BackendError> {
        let mut kinds = field_value_kinds(package, declaration);
        kinds.insert("dt".to_string(), crate::codegen_render::ValueKind::F64);
        let value = crate::codegen_render::value_expr(program, names, states, &kinds)?;
        let mut fields = Vec::new();
        for field in declaration.state.iter().chain(declaration.algebraic.iter()) {
            use crate::codegen_render::ValueKind;
            let name = escape_ident(&field.name);
            let (length, value): (String, String) = match kinds.get(&field.name) {
                Some(ValueKind::F64) => (
                    "1usize".to_string(),
                    "__model_data[__model_offset]".to_string(),
                ),
                Some(ValueKind::I64) => (
                    "1usize".to_string(),
                    "__model_data[__model_offset] as i64".to_string(),
                ),
                Some(ValueKind::Vector(element)) if **element == ValueKind::F64 => (
                    format!("self.{name}.len()"),
                    "__model_data[__model_offset..__model_end].to_vec()".to_string(),
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
                        "model field `{}` must have Float64 storage",
                        field.name
                    )));
                }
            };
            fields.push((field.name.clone(), Expr::Raw(format!(
                "{{ let __model_end = __model_offset.checked_add({length}).expect(\"model state length exceeds usize\"); let value = {value}; __model_offset = __model_end; value }}"
            ))));
        }
        let _ = owner;
        Ok(Stmt::Block(Block {
            statements: vec![
                Stmt::Let {
                    pattern: "__model_data".to_string(),
                    value: Box::new(value),
                },
                Stmt::Let {
                    pattern: "mut __model_offset".to_string(),
                    value: Box::new(Expr::Raw("0usize".to_string())),
                },
                Stmt::Let {
                    pattern: "__model_next".to_string(),
                    value: Box::new(Expr::StructLiteral {
                        name: "Self".to_string(),
                        fields,
                    }),
                },
                Stmt::Expr(Expr::Raw(
                    "assert_eq!(__model_offset, __model_data.len(), \"model step storage does not match state\")"
                        .to_string()
                )),
                Stmt::Expr(Expr::Call {
                    path: vec!["Ok".to_string()],
                    args: vec![Expr::Var("__model_next".to_string())],
                }),
            ],
        }))
    }
}
impl super::super::BackendInput<'_> {
    /// Width of one model carrier: 1 for a scalar, the fixed extent
    /// for a vector. Admission enforces the scalar/fixed-vector rule,
    /// so anything else here is an internal inconsistency.
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
            TypeNode::Tensor { shape, .. } if shape.is_empty() => Ok(1),
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
}
