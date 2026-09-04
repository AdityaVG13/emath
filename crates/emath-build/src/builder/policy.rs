//! Builder policies, boolean analysis, and helper queries.

use super::*;

impl BuilderModel {
    /// Lower one constructor model, enforcing its admission contract
    /// (E-CTOR-030/032/033/034/035/037/038/039) and delegation rules.
    pub(super) fn lower_constructor(
        model: &ConstructorModel,
        package: &mut SemanticPackage,
        state_fields: &[Field],
        all_names: &[String],
        float64: TypeId,
        boolean: TypeId,
        owner: Span,
    ) -> Result<emath_ir::Constructor, BuilderError> {
        let name = if model.name.is_empty() {
            "new".to_string()
        } else {
            model.name.clone()
        };
        let ground = |ty: TypeKind| match ty {
            TypeKind::Float64 => float64,
            TypeKind::Bool => boolean,
        };
        let mut parameters = Vec::new();
        let mut param_names = BTreeSet::new();
        for (param, ty) in &model.parameters {
            if !param_names.insert(param.clone()) {
                return Err(BuilderError(format!(
                    "duplicate constructor parameter `{param}` (E-CTOR-034)"
                )));
            }
            parameters.push(Field {
                name: param.clone(),
                ty: ground(*ty),
                visibility: Visibility::Public,
                source: owner,
            });
        }
        let params: Vec<(String, TypeId)> =
            parameters.iter().map(|f| (f.name.clone(), f.ty)).collect();

        let mut defaults = std::collections::BTreeMap::new();
        for (target, value) in &model.defaults {
            if !param_names.contains(target) {
                return Err(BuilderError(format!(
                    "default for undeclared parameter `{target}` (E-CTOR-039)"
                )));
            }
            if defaults.contains_key(target) {
                return Err(BuilderError(format!(
                    "duplicate default for parameter `{target}`"
                )));
            }
            if contains_state_reference(value) {
                return Err(BuilderError(format!(
                    "a default value cannot read `state.{target}` (E-CTOR-033)"
                )));
            }
            let (id, _) = Self::lower_expr(package, value, &[], float64, boolean)?;
            defaults.insert(target.clone(), id);
        }

        let mut preconditions = Vec::new();
        for expression in &model.preconditions {
            if !is_boolean(expression) {
                return Err(BuilderError(
                    "`require` must be a Boolean expression (E-CTOR-032)".into(),
                ));
            }
            let (id, _) = Self::lower_expr(package, expression, &params, float64, boolean)?;
            preconditions.push(id);
        }
        let mut postconditions = Vec::new();
        for expression in &model.postconditions {
            if !is_boolean(expression) {
                return Err(BuilderError(
                    "`ensure` must be a Boolean expression (E-CTOR-032)".into(),
                ));
            }
            let (id, _) = Self::lower_expr(package, expression, &params, float64, boolean)?;
            postconditions.push(id);
        }
        let error_type = if model.error_type.is_empty() {
            None
        } else {
            Some(package.push_type(TypeNode::Other(QualifiedName(model.error_type.clone()))))
        };
        let is_public = model.is_public || name == "new";

        // Delegation (factory surface): the target constructor performs
        // the body; local assignments are refused.
        if let Some(target) = &model.delegate {
            if !all_names.iter().any(|known| known == target) {
                return Err(BuilderError(format!(
                    "constructor `{name}` delegates to unknown `{target}` (E-CTOR-037)"
                )));
            }
            if !model.assignments.is_empty() {
                return Err(BuilderError(format!(
                    "delegating constructor `{name}` cannot assign state directly (E-CTOR-038)"
                )));
            }
            return Ok(emath_ir::Constructor {
                name,
                parameters,
                preconditions,
                assignments: std::collections::BTreeMap::new(),
                postconditions,
                defaults,
                error_type,
                is_public,
                source: owner,
            });
        }

        // Exact state coverage, one assignment per field.
        let mut assignments = std::collections::BTreeMap::new();
        for (target, expression) in &model.assignments {
            if !state_fields.iter().any(|field| &field.name == target) {
                return Err(BuilderError(format!(
                    "`{target}` is not a state field (E-CTOR-033)"
                )));
            }
            if assignments.contains_key(target) {
                return Err(BuilderError(format!(
                    "duplicate assignment for state field `{target}` (E-CTOR-035)"
                )));
            }
            if contains_state_reference(expression) {
                return Err(BuilderError(format!(
                    "constructor cannot read `state.{target}` while constructing (E-CTOR-033)"
                )));
            }
            let (id, _) = Self::lower_expr(package, expression, &params, float64, boolean)?;
            assignments.insert(target.clone(), id);
        }
        for field in state_fields {
            if !assignments.contains_key(&field.name) {
                return Err(BuilderError(format!(
                    "missing state assignment for `{}` (E-CTOR-030)",
                    field.name
                )));
            }
        }
        Ok(emath_ir::Constructor {
            name,
            parameters,
            preconditions,
            assignments,
            postconditions,
            defaults,
            error_type,
            is_public,
            source: owner,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expr(
        package: &mut SemanticPackage,
        expression: &Expression,
        env: &[(String, TypeId)],
        float64: TypeId,
        boolean: TypeId,
    ) -> Result<(emath_ir::ExprId, TypeId), BuilderError> {
        let owner = Span::default();
        let node = match expression {
            Expression::Float(value) => ExprNode::Literal(Literal::FloatBits(value.to_bits())),
            Expression::Int(value) => ExprNode::Literal(Literal::Integer(value.to_string())),
            Expression::Bool(value) => ExprNode::Literal(Literal::Bool(*value)),
            Expression::Symbol(name) => {
                let Some((_, ty)) = env.iter().find(|(env_name, _)| env_name == name) else {
                    return Err(BuilderError(format!("unknown symbol `{name}`")));
                };
                let _ = ty;
                ExprNode::Variable(QualifiedName(name.clone()))
            }
            Expression::Unary(op, inner) => {
                let (id, _) = Self::lower_expr(package, inner, env, float64, boolean)?;
                ExprNode::Unary {
                    operation: match op {
                        UnaryOp::Neg => emath_ir::UnaryOp::Negate,
                        UnaryOp::Sqrt => emath_ir::UnaryOp::Sqrt,
                        UnaryOp::Exp => emath_ir::UnaryOp::Exp,
                        UnaryOp::Log => emath_ir::UnaryOp::Log,
                        UnaryOp::Abs => emath_ir::UnaryOp::Abs,
                    },
                    value: id,
                }
            }
            Expression::Binary(op, left, right) => {
                let (l, _) = Self::lower_expr(package, left, env, float64, boolean)?;
                let (r, _) = Self::lower_expr(package, right, env, float64, boolean)?;
                ExprNode::Binary {
                    operation: match op {
                        BinaryOp::Add => emath_ir::BinaryOp::StrictFloatAdd,
                        BinaryOp::Sub => emath_ir::BinaryOp::StrictFloatSub,
                        BinaryOp::Mul => emath_ir::BinaryOp::StrictFloatMul,
                        BinaryOp::Div => emath_ir::BinaryOp::StrictFloatDiv,
                        BinaryOp::Pow => emath_ir::BinaryOp::StrictFloatPow,
                        BinaryOp::And => emath_ir::BinaryOp::And,
                        BinaryOp::Or => emath_ir::BinaryOp::Or,
                    },
                    left: l,
                    right: r,
                }
            }
            Expression::Call(name, args) => {
                let mut lowered = Vec::new();
                for arg in args {
                    let (id, _) = Self::lower_expr(package, arg, env, float64, boolean)?;
                    lowered.push(id);
                }
                ExprNode::Call {
                    function: QualifiedName(name.clone()),
                    arguments: lowered,
                }
            }
            Expression::Constraint(op, left, right) => {
                let (l, _) = Self::lower_expr(package, left, env, float64, boolean)?;
                let (r, _) = Self::lower_expr(package, right, env, float64, boolean)?;
                ExprNode::Binary {
                    operation: match op {
                        CmpOp::Eq => emath_ir::BinaryOp::Equal,
                        CmpOp::Ne => emath_ir::BinaryOp::NotEqual,
                        CmpOp::Lt => emath_ir::BinaryOp::Less,
                        CmpOp::Le => emath_ir::BinaryOp::LessEqual,
                        CmpOp::Gt => emath_ir::BinaryOp::Greater,
                        CmpOp::Ge => emath_ir::BinaryOp::GreaterEqual,
                    },
                    left: l,
                    right: r,
                }
            }
        };
        let id = package.push_expr(node, owner);
        let ty = match expression {
            Expression::Constraint(..)
            | Expression::Binary(BinaryOp::And | BinaryOp::Or, ..)
            | Expression::Bool(_) => boolean,
            _ => float64,
        };
        Ok((id, ty))
    }

    /// The kind schema this model builds against (the
    /// builder shares the same kind schema as the compiler; a generic
    /// requirement is rendered into the schema predicate).
    #[must_use]
    pub fn kind_schema(&self) -> emath_ir::KindSchema {
        let mut schema = match self.kind {
            Some(KindRef::Policy) => emath_ir::KindSchema::core_policy(),
            _ => emath_ir::KindSchema::core_function(),
        };
        if let Some(requirement) = &self.generic_requirement {
            schema.set_predicate(requirement.clone());
        }
        schema
    }
}

/// Whether a builder expression is Boolean (constraint or bool literal).
#[must_use]
pub fn is_boolean(expression: &Expression) -> bool {
    matches!(expression, Expression::Constraint(..) | Expression::Bool(_))
}

/// Whether a builder expression reads `state.<name>` (forbidden while
/// constructing: E-CTOR-033).
#[must_use]
pub fn contains_state_reference(expression: &Expression) -> bool {
    match expression {
        Expression::Symbol(name) => name.starts_with("state."),
        Expression::Float(_) | Expression::Int(_) | Expression::Bool(_) => false,
        Expression::Unary(_, inner) => contains_state_reference(inner),
        Expression::Binary(_, left, right) | Expression::Constraint(_, left, right) => {
            contains_state_reference(left) || contains_state_reference(right)
        }
        Expression::Call(_, args) => args.iter().any(contains_state_reference),
    }
}

/// Rust-side mirror of `CompilerPolicy` for laboratory use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuilderPolicy {
    pub verify_generated_crate: bool,
}

impl From<BuilderPolicy> for emath_sema::session::CompilerPolicy {
    fn from(policy: BuilderPolicy) -> Self {
        Self {
            verify_generated_crate: policy.verify_generated_crate,
        }
    }
}

// ---------------------------------------------------------------------------
// /09-008: macro expansion and artifact building.
// ---------------------------------------------------------------------------
