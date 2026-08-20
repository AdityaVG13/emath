//! Programmatic model builder: construct the same semantic representation
//! (SIR package + GIR goals) that `.emath` text admission produces, without
//! a source file. Hosts and the lab use this to compose models in Rust.
//!
//! Phase 1 supports the strict-f64 subset with one declaration. The
//! constructor surface admits overloads, factories,
//! delegation, defaults, derived fields, postconditions and typed
//! errors without bypassing schema or constructor admission
//!.

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
use std::collections::BTreeSet;

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

/// Convenience: builds an artifact from a programmatic model through the
/// exact `emath-build` artifact path.
pub fn build_from_model(
    model: BuilderModel,
    name: &str,
    target_dir: impl AsRef<std::path::Path>,
) -> Result<emath_build::BuildReport, BuilderError> {
    let mut package = model.build()?;
    package.seal();
    let diagnostics = emath_core::Diagnostics::new();
    emath_build::build_package(
        &package,
        name,
        &diagnostics,
        &[],
        target_dir.as_ref(),
        emath_build::BuildOptions::default(),
    )
    .map_err(|error| BuilderError(error.to_string()))
}

impl std::error::Error for BuilderError {}

/// A builder model: everything the trait collects before lowering.
#[derive(Clone, Debug, Default)]
pub struct BuilderModel {
    pub name: String,
    pub kind: Option<KindRef>,
    pub generic: Option<String>,
    /// Generic requirement predicate ( generic
    /// requirements), rendered into the kind schema.
    pub generic_requirement: Option<String>,
    pub inputs: Vec<(String, TypeKind)>,
    pub outputs: Vec<(String, TypeKind)>,
    pub state: Vec<(String, TypeKind)>,
    /// Constructor set: the first is the primary (`new`); further
    /// entries are overloads/factories.
    pub constructors: Vec<ConstructorModel>,
    /// Derived fields: computed after construction from state; lowered
    /// as definitions ( derived fields).
    pub derived: Vec<(String, Expression)>,
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

/// One constructor ( subset in the builder: overloads,
/// factories, delegation, defaults, derived fields, postconditions,
/// typed errors).
#[derive(Clone, Debug, Default)]
pub struct ConstructorModel {
    /// Constructor name; the primary must be `new`, overloads may
    /// declare further names.
    pub name: String,
    pub is_public: bool,
    pub parameters: Vec<(String, TypeKind)>,
    /// Default values for parameters that may be omitted at call sites.
    pub defaults: Vec<(String, Expression)>,
    pub preconditions: Vec<Expression>,
    /// State assignments.
    pub assignments: Vec<(String, Expression)>,
    /// Postconditions (`ensure` surface).
    pub postconditions: Vec<Expression>,
    /// Typed error (`Result<Self, T>` surface).
    pub error_type: String,
    /// Delegation: forward the body to this already-declared
    /// constructor (factory surface).
    pub delegate: Option<String>,
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
    fn generic_requirement(self, predicate: impl Into<String>) -> Self;
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
    fn derive(self, name: impl Into<String>, expression: Expression) -> Self;
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

    fn generic_requirement(mut self, predicate: impl Into<String>) -> Self {
        self.generic_requirement = Some(predicate.into());
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
        self.constructors.push(constructor);
        self
    }

    fn define(mut self, name: impl Into<String>, expression: Expression) -> Self {
        self.definitions.push((name.into(), expression));
        self
    }

    fn derive(mut self, name: impl Into<String>, expression: Expression) -> Self {
        self.derived.push((name.into(), expression));
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
        // Derived fields are computed after construction
        // from state; they must be outputs and lower as definitions.
        for (name, expression) in &self.derived {
            if !outputs.iter().any(|output| &output.name == name) {
                return Err(BuilderError(format!(
                    "derived field `{name}` is not an output (E-NAME-024)"
                )));
            }
            let (id, ty) = Self::lower_expr(&mut package, expression, &env, float64, boolean)?;
            env.push((name.clone(), ty));
            definitions.insert(name.clone(), id);
        }
        let compile_spec = match &self.compile {
            Some(compile) => {
                if compile.target != "rust" || compile.profile != "library" {
                    return Err(BuilderError(format!(
                        "compile spec `{}/{}` outside Phase 1 subset (E-CODEGEN-012)",
                        compile.target, compile.profile
                    )));
                }
                CompileSpec {
                    target: compile.target.clone(),
                    profile: compile.profile.clone(),
                    numeric: NumericProfile::StrictF64,
                    safety: SafetyProfile::ForbidUnsafe,
                    unresolved: None,
                }
            }
            None => CompileSpec {
                target: "rust".into(),
                profile: "library".into(),
                numeric: NumericProfile::StrictF64,
                safety: SafetyProfile::ForbidUnsafe,
                unresolved: None,
            },
        };

        // Constructor admission (the builder must not
        // bypass schema or constructor admission). Policies require a
        // public `new`; functions cannot carry constructors.
        let is_policy = self.kind == Some(KindRef::Policy);
        if is_policy && self.constructors.is_empty() {
            return Err(BuilderError(
                "policy declarations require a `constructors:` section with a public `new` \
                 (E-CTOR-031)"
                    .into(),
            ));
        }
        if !is_policy && !self.constructors.is_empty() {
            return Err(BuilderError(
                "function declarations cannot have constructors in this subphase (E-KIND-010)"
                    .into(),
            ));
        }
        let all_names: Vec<String> = self
            .constructors
            .iter()
            .map(|model| {
                if model.name.is_empty() {
                    "new".to_string()
                } else {
                    model.name.clone()
                }
            })
            .collect();
        if all_names.first().is_some_and(|first| first != "new") {
            return Err(BuilderError(
                "the primary constructor must be named `new` (E-CTOR-036)".into(),
            ));
        }
        if all_names.first().is_some_and(|first| first == "new")
            && all_names.iter().filter(|name| *name == "new").count() > 1
        {
            return Err(BuilderError(
                "multiple constructors named `new` (E-CTOR-034)".into(),
            ));
        }
        let mut constructors: Vec<emath_ir::Constructor> = Vec::new();
        for model in &self.constructors {
            constructors.push(Self::lower_constructor(
                model,
                &mut package,
                &state,
                &all_names,
                float64,
                boolean,
                OWNER,
            )?);
        }

        let tests: Vec<emath_ir::TestCase> = self
            .tests
            .iter()
            .map(|test| -> Result<_, BuilderError> {
                let mut given = std::collections::BTreeMap::new();
                let mut given_env: Vec<(String, TypeId)> = Vec::new();
                for (name, expression) in &test.given {
                    let (id, ty) =
                        Self::lower_expr(&mut package, expression, &given_env, float64, boolean)?;
                    given.insert(name.clone(), id);
                    given_env.push((name.clone(), ty));
                }
                let (expect, _) =
                    Self::lower_expr(&mut package, &test.expect, &given_env, float64, boolean)?;
                Ok(emath_ir::TestCase {
                    name: test.name.clone(),
                    given,
                    expect: Some(expect),
                    source: OWNER,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

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
                    payload: emath_ir::GoalPayload::default(),
                    source: OWNER,
                }
            })
            .collect();

        // Attach tests and goals to the package and the declaration.
        // Both attach by id (like the admit lane): a builder model's
        // tests must surface on `declaration.tests` so identity and the
        // generated `#[test]` functions see them.
        let goal_start = package.goals.len();
        let test_start = package.tests.len();
        package.tests.extend(tests);
        package.goals.extend(goals);
        let goal_ids: Vec<emath_ir::GoalId> = package
            .goals
            .iter()
            .skip(goal_start)
            .map(|goal| goal.id)
            .collect();
        // A TestId is the test's arena position (TestCase carries no id
        // field; the package index is the stable id).
        let test_ids: Vec<emath_ir::TestId> = package
            .tests
            .iter()
            .enumerate()
            .skip(test_start)
            .map(|(index, _)| emath_ir::TestId(u32::try_from(index).unwrap_or(u32::MAX)))
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
            constructors,
            definitions,
            invariants,
            goals: goal_ids,
            tests: test_ids,
            exports: Vec::new(),
            compile_spec,
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: OWNER,
        };
        package.declarations.push(declaration);
        package.seal();
        Ok(package)
    }
}

impl BuilderModel {
    /// Lower one constructor model, enforcing its admission contract:
    /// boolean requirements (E-CTOR-032), no state reads while
    /// constructing (E-CTOR-033), exact state coverage (E-CTOR-030 /
    /// E-CTOR-035), defaults for declared parameters only
    /// (E-CTOR-039), typed errors, postconditions, and delegation to
    /// declared constructors only (E-CTOR-037 / E-CTOR-038).
    fn lower_constructor(
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

/// Expansion of the `emath!` proc macro: the parsed source literal plus its
/// deterministic identity. Parsing lives here (a normal crate) so it is
/// unit-testable; the proc-macro crate is a thin shim over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroExpansion {
    /// Parsed `.emath` source text.
    pub source: String,
    /// FNV-1a64 identity of the source.
    pub identity: String,
}

impl MacroExpansion {
    /// Used by the `emath!` proc macro to reconstruct an expansion from
    /// emitted literals (compile-time constant path).
    #[must_use]
    pub fn from_literals(source: &'static str, identity: &'static str) -> Self {
        Self {
            source: source.to_string(),
            identity: identity.to_string(),
        }
    }
}

/// Macro expansion failure (input must be a single string literal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroError {
    /// Stable code (`E-CODEGEN-011`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Parses a proc-macro token stream into a source literal. Token text is
/// parsed (never concatenated), so arbitrary input cannot inject tokens.
pub fn macro_expand(input: &str) -> Result<MacroExpansion, MacroError> {
    let input = input.trim();
    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        let inner = &input[1..input.len() - 1];
        if inner.contains('"') {
            return Err(MacroError {
                code: "E-CODEGEN-011",
                message: "unescaped quotes are not supported in emath! literals".into(),
            });
        }
        let identity = emath_core::content_id_of_str(inner).0;
        Ok(MacroExpansion {
            source: inner.to_string(),
            identity,
        })
    } else {
        Err(MacroError {
            code: "E-CODEGEN-011",
            message: "`emath!` requires a single string literal of `.emath` source".into(),
        })
    }
}

/// Builds an artifact from in-memory `.emath` source (the runtime half of
/// the `emath!` macro expansion); the exact `build_text` compiler path.
pub fn build_from_source(
    name: &str,
    source: &str,
    target_dir: impl AsRef<std::path::Path>,
) -> Result<emath_build::BuildReport, emath_build::BuildError> {
    emath_build::build_text(
        name,
        source,
        target_dir,
        emath_build::BuildOptions::default(),
    )
}
