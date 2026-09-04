//! The ModelBuilder trait and its BuilderModel implementation.

use super::*;

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
            algebraic: Vec::new(),
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
