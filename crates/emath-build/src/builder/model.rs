//! Builder data model: expressions, goals, compile/test models.

use super::*;

/// Synthetic span for programmatically-built nodes (no source file).
pub(super) const OWNER: Span = Span {
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
) -> Result<crate::BuildReport, BuilderError> {
    let mut package = model.build()?;
    package.seal();
    let diagnostics = emath_core::Diagnostics::new();
    crate::build_package(
        &package,
        name,
        &diagnostics,
        &[],
        target_dir.as_ref(),
        crate::BuildOptions::default(),
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
