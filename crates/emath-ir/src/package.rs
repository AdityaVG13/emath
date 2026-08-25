//! Semantic package: the SIR arena model.

pub use crate::constructor::Field;
use crate::constructor::{Constructor, TestCase};
use crate::evidence::EvidenceClaim;
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
    /// `algebraic:` unknowns of a causalized implicit-residual `emath model`
    /// (empty for every other declaration). Newton-solved at simulation
    /// and at `rust.library` codegen step time; kept off `inputs` because
    /// they are solved unknowns with initial guesses, not I/O contract.
    /// Admission requires each to be `Float64` or a fixed-length vector of
    /// `Float64`, so codegen can lay out the flattened solve vector at
    /// compile time.
    pub algebraic: Vec<Field>,
    pub constructors: Vec<Constructor>,
    pub definitions: BTreeMap<String, ExprId>,
    pub invariants: Vec<ExprId>,
    pub goals: Vec<GoalId>,
    pub tests: Vec<TestId>,
    pub exports: Vec<Export>,
    pub compile_spec: CompileSpec,
    /// `about:` prose retained from source (summary text).
    pub about: Option<String>,
    /// `evidence:` claims recorded with verdict `NotRun` until a checker discharges them.
    pub evidence: Vec<EvidenceClaim>,
    /// `host:` bindings retained structurally. Native rust.library codegen does not
    /// emit trait impls from these records (typed no-claim).
    pub host: Vec<HostBinding>,
    pub source: Span,
}

/// One causalized implicit residual of an `emath model`: an `equations:`
/// entry that is neither an explicit `der(x) = rhs` rate nor an algebraic
/// definition; the runner drives it to zero with Newton's method.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelResidual {
    /// The residual expression (`left - right`); definitions inlined,
    /// each `der(x)` rewritten to placeholder `__rate_<x>`.
    pub expr: ExprId,
    /// Dimension of the residual after lowering: 1 for scalar, `n` for a
    /// known-extent vector. Used at admission for the squareness check.
    pub components: u16,
    /// `algebraic:` variable names this declaration solves for; identical
    /// on every residual (one coupled system), initial guesses from `inputs`.
    pub algebraic: Vec<String>,
    /// State rate unknowns `der(x)` referenced by this residual with no
    /// explicit rate equation for `x`.
    pub rates: Vec<String>,
}

/// One `host:` language binding (`rust:` / `implement Trait for Type:`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostBinding {
    /// Host language (`rust`).
    pub language: String,
    /// Trait path (`cache_core::Policy`).
    pub trait_path: String,
    /// Implementing type (`AdaptiveCachePolicy`).
    pub target: String,
    /// Methods retained from the host block.
    pub methods: Vec<HostMethod>,
}

/// One host method inside an `implement` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMethod {
    /// Method name.
    pub name: String,
    /// `(name, type)` parameters in source order.
    pub params: Vec<(String, String)>,
    /// Return type display, when present.
    pub ret: Option<String>,
    /// Retained body commands (not executed by Phase 1 native codegen).
    pub body: Vec<String>,
}

/// One admitted import (`use` front-end).
#[derive(Clone, Debug, PartialEq)]
pub struct ImportEntry {
    /// Dotted library path (`std.units`).
    pub path: Vec<String>,
    /// Imported names / wildcard.
    pub selection: ImportSelection,
    /// Source span of the `use` item.
    pub source: Span,
}

/// Imported name selection.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportSelection {
    /// `use std.numeric.*`
    All,
    /// `use std.units.{Millisecond, ...}` or `use std.numeric.Real`
    Named(Vec<(String, Option<String>)>),
}

impl ImportSelection {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::All => "*".to_string(),
            Self::Named(names) => {
                let mut names = names.clone();
                names.sort_by(|a, b| a.0.cmp(&b.0));
                names
                    .iter()
                    .map(|(name, alias)| match alias {
                        Some(alias) => format!("{name} as {alias}"),
                        None => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            }
        }
    }
}

/// The neutral semantic package. IDs index the sibling arenas.
#[derive(Clone, Debug, Default)]
pub struct SemanticPackage {
    pub identity: Option<PackageIdentity>,
    /// `package <dotted>` identity declared by the front-end.
    pub package_path: Option<Vec<String>>,
    /// Admitted imports (front-end).
    pub imports: Vec<ImportEntry>,
    pub declarations: Vec<Declaration>,
    /// Causalized implicit residuals per model declaration; package-side
    /// so adding a section does not churn every `Declaration` literal.
    pub residuals: std::collections::BTreeMap<DeclarationId, Vec<ModelResidual>>,
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
            package_path: None,
            imports: Vec::new(),
            declarations: Vec::new(),
            residuals: std::collections::BTreeMap::new(),
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
            edition: "2024".to_string(),
            content,
        });
    }
}
