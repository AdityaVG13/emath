//! Semantic package: the SIR arena model.

pub use crate::constructor::Field;
use crate::constructor::{Constructor, TestCase};
use crate::expression::ExprNode;
use crate::goal::{CompileSpec, Export, Goal, ResolutionPlan};
use crate::ids::{DeclarationId, ExprId, GoalId, TestId, TypeId};
use crate::types::TypeNode;
use emath_core::{ContentId, QualifiedName, Span};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageIdentity {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub content: ContentId,
}

/// One admitted declaration (Phase 1 subset).
#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    pub id: DeclarationId,
    pub name: QualifiedName,
    pub kind: QualifiedName,
    pub kind_label: String,
    pub inputs: Vec<Field>,
    pub outputs: Vec<Field>,
    pub state: Vec<Field>,
    pub constructors: Vec<Constructor>,
    pub definitions: BTreeMap<String, ExprId>,
    pub invariants: Vec<ExprId>,
    pub goals: Vec<GoalId>,
    pub tests: Vec<TestId>,
    pub exports: Vec<Export>,
    pub compile_spec: CompileSpec,
    pub source: Span,
}

/// The neutral semantic package. IDs index the sibling arenas.
#[derive(Clone, Debug, Default)]
pub struct SemanticPackage {
    pub identity: Option<PackageIdentity>,
    pub declarations: Vec<Declaration>,
    pub types: Vec<TypeNode>,
    pub exprs: Vec<ExprNode>,
    pub expr_spans: Vec<Span>,
    pub goals: Vec<Goal>,
    pub tests: Vec<TestCase>,
    pub plans: Vec<ResolutionPlan>,
}

impl SemanticPackage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: None,
            declarations: Vec::new(),
            types: Vec::new(),
            exprs: Vec::new(),
            expr_spans: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            plans: Vec::new(),
        }
    }

    pub fn push_type(&mut self, ty: TypeNode) -> TypeId {
        self.types.push(ty);
        TypeId(u32::try_from(self.types.len() - 1).unwrap_or(u32::MAX))
    }

    pub fn push_expr(&mut self, expr: ExprNode, span: Span) -> ExprId {
        self.exprs.push(expr);
        self.expr_spans.push(span);
        ExprId(u32::try_from(self.exprs.len() - 1).unwrap_or(u32::MAX))
    }

    pub fn push_goal(&mut self, goal: Goal) -> GoalId {
        self.goals.push(goal);
        GoalId(u32::try_from(self.goals.len() - 1).unwrap_or(u32::MAX))
    }

    pub fn push_test(&mut self, test: TestCase) -> TestId {
        self.tests.push(test);
        TestId(u32::try_from(self.tests.len() - 1).unwrap_or(u32::MAX))
    }

    #[must_use]
    pub fn declaration(&self, id: DeclarationId) -> Option<&Declaration> {
        self.declarations.get(id.index())
    }

    #[must_use]
    pub fn expr(&self, id: ExprId) -> Option<&ExprNode> {
        self.exprs.get(id.index())
    }

    #[must_use]
    pub fn expr_span(&self, id: ExprId) -> Span {
        self.expr_spans.get(id.index()).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn ty(&self, id: TypeId) -> Option<&TypeNode> {
        self.types.get(id.index())
    }

    #[must_use]
    pub fn goal(&self, id: GoalId) -> Option<&Goal> {
        self.goals.get(id.index())
    }

    /// Content identity of the whole package (bootstrap fingerprint over
    /// canonical bytes). See `crate::canonical`.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        if let Some(identity) = &self.identity {
            return identity.content.clone();
        }
        crate::canonical::canonical_package(self)
    }

    /// Set the package identity from canonical bytes.
    pub fn seal(&mut self) {
        let content = crate::canonical::canonical_package(self);
        let name = self
            .declarations
            .first()
            .map_or_else(|| "package".to_string(), |d| d.name.leaf().to_string());
        self.identity = Some(PackageIdentity {
            name,
            version: "0.1.0".to_string(),
            edition: "2021".to_string(),
            content,
        });
    }
}
