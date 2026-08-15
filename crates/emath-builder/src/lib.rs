//! Programmatic model builder: construct the same semantic representation
//! (SIR package + GIR goals) that `.emath` text admission produces, without
//! a source file. Hosts and the lab use this to compose models in Rust.
//!
//! Phase 1 supports the strict-f64 subset with one declaration.

#![forbid(unsafe_code)]

use emath_core::{QualifiedName, Span};
use emath_ir::constructor::Visibility;
use emath_ir::ids::DeclarationId;
use emath_ir::package::Field;
use emath_ir::{
    CompileSpec, Declaration, DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExprNode,
    FallbackPolicy, Goal, GoalKind, GoalRequirements, Literal, NumericProfile, SafetyProfile,
    SemanticPackage, TargetProfile, TypeId, TypeNode,
};

/// Synthetic span for programmatically-built nodes (no source file).
const OWNER: Span = Span {
    file: emath_core::FileId(0),
    start: 0,
    end: 0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindRef {
    Function,
    Policy,
}

impl KindRef {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Policy => "policy",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuilderError(pub String);

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "model builder: {}", self.0)
    }
}

impl std::error::Error for BuilderError {}

/// A builder model: everything the trait collects before lowering.
#[derive(Clone, Debug, Default)]
pub struct BuilderModel {
    pub name: String,
    pub kind: Option<KindRef>,
    pub generic: Option<String>,
    pub inputs: Vec<(String, TypeKind)>,
    pub outputs: Vec<(String, TypeKind)>,
    pub state: Vec<(String, TypeKind)>,
    pub constructor: Option<ConstructorModel>,
    pub definitions: Vec<(String, Expression)>,
    pub goals: Vec<GoalModel>,
    pub tests: Vec<TestModel>,
    pub compile: Option<CompileModel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Float64,
    Bool,
}

#[derive(Clone, Debug, Default)]
pub struct ConstructorModel {
    pub parameters: Vec<(String, TypeKind)>,
    pub preconditions: Vec<Expression>,
    pub assignments: Vec<(String, Expression)>,
    pub error_type: String,
}

#[derive(Clone, Debug)]
pub enum Expression {
    Float(f64),
    Int(i64),
    Bool(bool),
    Symbol(String), // input, state.<name> or previously defined name
    Unary(UnaryOp, Box<Expression>),
    Binary(BinaryOp, Box<Expression>, Box<Expression>),
    Call(String, Vec<Expression>),
    Constraint(CmpOp, Box<Expression>, Box<Expression>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Sqrt,
    Exp,
    Log,
    Abs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub struct GoalModel {
    pub kind: String, // Phase 1: "evaluate"
    pub target: String,
    pub produce: String,
}

#[derive(Clone, Debug)]
pub struct TestModel {
    pub name: String,
    pub given: Vec<(String, Expression)>,
    pub expect: Expression,
}

#[derive(Clone, Debug)]
pub struct CompileModel {
    pub target: String,
    pub profile: String,
}

/// The builder trait (`PUBLIC_API_INVENTORY.md` laboratory surface).
pub trait ModelBuilder: Sized {
    #[must_use]
    fn custom(name: impl Into<String>) -> Self;
    #[must_use]
    fn kind(self, kind: KindRef) -> Self;
    #[must_use]
    fn generic(self, parameter: impl Into<String>) -> Self;
    #[must_use]
    fn input(self, name: impl Into<String>, ty: TypeKind) -> Self;
    #[must_use]
    fn output(self, name: impl Into<String>, ty: TypeKind) -> Self;
    #[must_use]
    fn state(self, name: impl Into<String>, ty: TypeKind) -> Self;
    #[must_use]
    fn constructor(self, constructor: ConstructorModel) -> Self;
    #[must_use]
    fn define(self, name: impl Into<String>, expression: Expression) -> Self;
    #[must_use]
    fn goal(self, goal: GoalModel) -> Self;
    #[must_use]
    fn test(self, test: TestModel) -> Self;
    #[must_use]
    fn compile(self, compile: CompileModel) -> Self;
    fn build(self) -> Result<SemanticPackage, BuilderError>;
}

impl ModelBuilder for BuilderModel {
    fn custom(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    fn kind(mut self, kind: KindRef) -> Self {
        self.kind = Some(kind);
        self
    }

    fn generic(mut self, parameter: impl Into<String>) -> Self {
        self.generic = Some(parameter.into());
        self
    }

    fn input(mut self, name: impl Into<String>, ty: TypeKind) -> Self {
        self.inputs.push((name.into(), ty));
        self
    }

    fn output(mut self, name: impl Into<String>, ty: TypeKind) -> Self {
        self.outputs.push((name.into(), ty));
        self
    }

    fn state(mut self, name: impl Into<String>, ty: TypeKind) -> Self {
        self.state.push((name.into(), ty));
        self
    }

    fn constructor(mut self, constructor: ConstructorModel) -> Self {
        self.constructor = Some(constructor);
        self
    }

    fn define(mut self, name: impl Into<String>, expression: Expression) -> Self {
        self.definitions.push((name.into(), expression));
        self
    }

    fn goal(mut self, goal: GoalModel) -> Self {
        self.goals.push(goal);
        self
    }

    fn test(mut self, test: TestModel) -> Self {
        self.tests.push(test);
        self
    }

    fn compile(mut self, compile: CompileModel) -> Self {
        self.compile = Some(compile);
        self
    }

    /// Lower to the same SIR package produced by text admission.
    fn build(self) -> Result<SemanticPackage, BuilderError> {
        if self.name.is_empty() {
            return Err(BuilderError("declaration name cannot be empty".into()));
        }
        let mut package = SemanticPackage::new();
        let float64: TypeId = package.push_type(TypeNode::Float64);
        let boolean: TypeId = package.push_type(TypeNode::Bool);

        let ground = |ty: TypeKind| match ty {
            TypeKind::Float64 => float64,
            TypeKind::Bool => boolean,
        };
        let make_field = |name: String, ty: TypeKind| Field {
            name,
            ty: ground(ty),
            visibility: Visibility::Public,
            source: OWNER,
        };
        let field = &make_field;

        let inputs: Vec<Field> = self
            .inputs
            .iter()
            .map(|(n, t)| field(n.clone(), *t))
            .collect();
        let outputs: Vec<Field> = self
            .outputs
            .iter()
            .map(|(n, t)| field(n.clone(), *t))
            .collect();
        let state: Vec<Field> = self
            .state
            .iter()
            .map(|(n, t)| field(n.clone(), *t))
            .collect();

        // Lower expressions against the model environment: inputs, state
        // fields (`state.<name>`), then definitions in order.
        let mut env: Vec<(String, TypeId)> =
            inputs.iter().map(|f| (f.name.clone(), f.ty)).collect();
        for f in &state {
            env.push((format!("state.{}", f.name), f.ty));
        }
        let mut definitions = std::collections::BTreeMap::new();
        let invariants = Vec::new();
        for (name, expression) in &self.definitions {
            let (id, ty) = Self::lower_expr(&mut package, expression, &env, float64, boolean)?;
            env.push((name.clone(), ty));
            definitions.insert(name.clone(), id);
        }
        let compile_spec = match &self.compile {
            Some(compile) => CompileSpec {
                target: compile.target.clone(),
                profile: compile.profile.clone(),
                numeric: NumericProfile::StrictF64,
                safety: SafetyProfile::ForbidUnsafe,
                unresolved: None,
            },
            None => CompileSpec {
                target: "rust".into(),
                profile: "library".into(),
                numeric: NumericProfile::StrictF64,
                safety: SafetyProfile::ForbidUnsafe,
                unresolved: None,
            },
        };

        let constructor = match &self.constructor {
            Some(model) => Some(Self::lower_constructor(
                model,
                &mut package,
                float64,
                boolean,
                OWNER,
            )?),
            None => None,
        };

        let tests: Vec<emath_ir::TestCase> = self
            .tests
            .iter()
            .map(|test| {
                let mut given = std::collections::BTreeMap::new();
                let mut given_env: Vec<(String, TypeId)> = Vec::new();
                for (name, expression) in &test.given {
                    let (id, ty) =
                        Self::lower_expr(&mut package, expression, &given_env, float64, boolean)
                            .unwrap();
                    given.insert(name.clone(), id);
                    given_env.push((name.clone(), ty));
                }
                let (expect, _) =
                    Self::lower_expr(&mut package, &test.expect, &given_env, float64, boolean)
                        .unwrap();
                emath_ir::TestCase {
                    name: test.name.clone(),
                    given,
                    expect,
                    source: OWNER,
                }
            })
            .collect();

        let goals: Vec<Goal> = self
            .goals
            .iter()
            .map(|goal| {
                let id = emath_ir::GoalId(u32::try_from(package.goals.len()).unwrap_or(u32::MAX));
                Goal {
                    id,
                    kind: GoalKind::Evaluate,
                    target: goal.target.clone(),
                    expression: definitions.get(&goal.target).copied(),
                    requirements: GoalRequirements {
                        evidence: EvidenceLevel::E1,
                        exactness: ExactnessPolicy::Exact,
                        determinism: DeterminismPolicy::Required,
                        target: TargetProfile {
                            family: "rust-library".into(),
                            triple: None,
                            features: vec![],
                        },
                        fallback: FallbackPolicy::NativeOnly,
                        produce: goal.produce.clone(),
                    },
                    source: OWNER,
                }
            })
            .collect();

        // Attach tests and goals to the package and the declaration.
        let goal_start = package.goals.len();
        package.tests.extend(tests);
        package.goals.extend(goals);
        let goal_ids: Vec<emath_ir::GoalId> = package
            .goals
            .iter()
            .skip(goal_start)
            .map(|goal| goal.id)
            .collect();

        let declaration = Declaration {
            id: DeclarationId(0),
            name: QualifiedName(self.name.clone()),
            kind: QualifiedName(
                self.kind
                    .map_or_else(|| "function".to_string(), |k| k.label().to_string()),
            ),
            kind_label: self
                .kind
                .map_or_else(|| "function".to_string(), |k| k.label().to_string()),
            inputs,
            outputs,
            state,
            constructors: constructor.into_iter().collect(),
            definitions,
            invariants,
            goals: goal_ids,
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec,
            source: OWNER,
        };
        package.declarations.push(declaration);
        package.seal();
        Ok(package)
    }
}

impl BuilderModel {
    fn lower_constructor(
        model: &ConstructorModel,
        package: &mut SemanticPackage,
        float64: TypeId,
        boolean: TypeId,
        owner: Span,
    ) -> Result<emath_ir::Constructor, BuilderError> {
        let ground = |ty: TypeKind| match ty {
            TypeKind::Float64 => float64,
            TypeKind::Bool => boolean,
        };
        let parameters: Vec<Field> = model
            .parameters
            .iter()
            .map(|(name, ty)| Field {
                name: name.clone(),
                ty: ground(*ty),
                visibility: Visibility::Public,
                source: owner,
            })
            .collect();
        let params: Vec<(String, TypeId)> =
            parameters.iter().map(|f| (f.name.clone(), f.ty)).collect();
        let mut preconditions = Vec::new();
        for expression in &model.preconditions {
            let (id, _) = Self::lower_expr(package, expression, &params, float64, boolean)?;
            preconditions.push(id);
        }
        let mut assignments = std::collections::BTreeMap::new();
        for (target, expression) in &model.assignments {
            let (id, _) = Self::lower_expr(package, expression, &params, float64, boolean)?;
            assignments.insert(target.clone(), id);
        }
        Ok(emath_ir::Constructor {
            name: "new".into(),
            parameters,
            preconditions,
            assignments,
            postconditions: Vec::new(),
            error_type: Some(
                package.push_type(TypeNode::Other(QualifiedName(model.error_type.clone()))),
            ),
            is_public: true,
            source: owner,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_expr(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn square_model() -> BuilderModel {
        BuilderModel::custom("Square")
            .kind(KindRef::Function)
            .input("x", TypeKind::Float64)
            .output("y", TypeKind::Float64)
            .define(
                "y",
                Expression::Binary(
                    BinaryOp::Mul,
                    Box::new(Expression::Symbol("x".into())),
                    Box::new(Expression::Symbol("x".into())),
                ),
            )
            .goal(GoalModel {
                kind: "evaluate".into(),
                target: "y".into(),
                produce: "rust.library".into(),
            })
    }

    #[test]
    fn builder_lowers_to_package() {
        let package = square_model().build().unwrap();
        assert_eq!(package.declarations.len(), 1);
        assert_eq!(package.declarations[0].name.leaf(), "Square");
        assert_eq!(package.goals.len(), 1);
        assert!(package.goals[0].expression.is_some());
        // Identity is sealed: content id is stable.
        assert!(!package.content_id().0.is_empty());
    }

    #[test]
    fn builder_rejects_unknown_symbols() {
        let model = square_model().define("z", Expression::Symbol("missing".into()));
        let error = model.build().unwrap_err();
        assert!(error.0.contains("missing"));
    }

    #[test]
    fn builder_constructs_policy() {
        let model = BuilderModel::custom("AffinePolicy")
            .kind(KindRef::Policy)
            .input("x", TypeKind::Float64)
            .output("score", TypeKind::Float64)
            .state("scale", TypeKind::Float64)
            .state("bias", TypeKind::Float64)
            .constructor(ConstructorModel {
                parameters: vec![
                    ("scale".into(), TypeKind::Float64),
                    ("bias".into(), TypeKind::Float64),
                ],
                preconditions: vec![Expression::Constraint(
                    CmpOp::Ge,
                    Box::new(Expression::Symbol("scale".into())),
                    Box::new(Expression::Float(0.0)),
                )],
                assignments: vec![
                    ("scale".into(), Expression::Symbol("scale".into())),
                    ("bias".into(), Expression::Symbol("bias".into())),
                ],
                error_type: "ConfigError".into(),
            })
            .define(
                "score",
                Expression::Binary(
                    BinaryOp::Add,
                    Box::new(Expression::Binary(
                        BinaryOp::Mul,
                        Box::new(Expression::Symbol("state.scale".into())),
                        Box::new(Expression::Symbol("x".into())),
                    )),
                    Box::new(Expression::Symbol("state.bias".into())),
                ),
            )
            .goal(GoalModel {
                kind: "evaluate".into(),
                target: "score".into(),
                produce: "rust.library".into(),
            })
            .test(TestModel {
                name: "shift_up".into(),
                given: vec![
                    ("scale".into(), Expression::Float(2.0)),
                    ("bias".into(), Expression::Float(1.0)),
                    ("x".into(), Expression::Float(3.0)),
                ],
                expect: Expression::Float(7.0),
            });
        let package = model.build().unwrap();
        assert_eq!(package.tests.len(), 1);
        assert_eq!(package.declarations[0].constructors.len(), 1);
    }

    #[test]
    fn builder_errors_on_empty_name() {
        let error = BuilderModel::custom("").build().unwrap_err();
        assert!(error.0.contains("empty"));
    }
}
