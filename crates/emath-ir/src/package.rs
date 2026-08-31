//! Semantic package: the SIR arena model.

pub use crate::constructor::Field;
use crate::constructor::{Constructor, TestCase};
use crate::evidence::EvidenceClaim;
use crate::expression::ExprNode;
use crate::goal::{CompileSpec, Export, Goal, ResolutionPlan};
use crate::ids::{CapabilityId, DeclarationId, ExprId, GoalId, TestId, TypeId};
use crate::provenance::{BindingSite, Provenance};
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

/// One hybrid event rule (r3-dynamical-03lh ch7, event-execution slice):
/// a declared event with a `.emath` Boolean condition over the model's
/// inputs, state, and algebraic unknowns (definitions inlined at
/// admission) plus exactly one deterministic action. The runner fires
/// the event once per rising edge of the condition and persists the
/// action into the live input/state map for all later steps.
#[derive(Clone, Debug, PartialEq)]
pub struct EventDecl {
    /// Event name from the `events:` section (`event Name(field: Type)`).
    pub name: String,
    /// Parameter names from the `event Name(field: Type)` head, in
    /// declaration order. These are RUNTIME-CAPTURED payloads: when the
    /// event fires, the runner binds each parameter to the live value of
    /// the SAME-NAMED input/state/algebraic variable at the crossing
    /// sample. Admission requires every parameter name to match a
    /// declared model variable (else E-TRANS-006) so binding is always
    /// defined: a parameter is a named capture of that variable.
    pub params: Vec<String>,
    /// Boolean condition expression; definitions inlined at admission.
    pub condition: ExprId,
    /// The single deterministic action applied exactly once at the
    /// crossing sample.
    pub action: EventAction,
}

/// One event action: an assignment to a declared `inputs:` or `state:`
/// slot. Algebraic unknowns are refused as targets at admission — the
/// Newton projection owns them.
#[derive(Clone, Debug, PartialEq)]
pub struct EventAction {
    /// Declared `inputs:` or `state:` field name the action writes.
    pub target: String,
    /// Right-hand side expression; definitions inlined at admission and
    /// slot-typed against the target's declared type.
    pub expr: ExprId,
}

/// One transition rule (r3-dynamical-03lh ch7, transitions slice): an
/// `on <Event>:` rule attaches deterministic re-assignments to a
/// DECLARED event by name. The runner applies the actions when the
/// named event fires; the action values may reference the event's
/// runtime-captured parameters (bound from same-named model variables
/// at the firing sample).
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionDecl {
    /// Declared event name the rule triggers on (from `events:`).
    pub trigger: String,
    /// Deterministic re-assignments applied at firing, in source order.
    /// Each action's `expr` may reference the trigger's captured event
    /// parameters; definitions are inlined at admission.
    pub actions: Vec<TransitionAction>,
}

/// One transition action: a re-assignment of a declared `inputs:` or
/// `state:` slot. `is_state` distinguishes a `state.<name>` target from
/// a bare declared input/state name.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionAction {
    /// Declared `inputs:` / `state:` field name the action writes.
    pub target: String,
    /// `true` when the action wrote `state.<target>`; `false` for a bare
    /// declared input/state name.
    pub is_state: bool,
    /// Right-hand side expression; definitions inlined at admission.
    /// A referenced event parameter remains a plain `Variable` node whose
    /// name is a runtime capture bound by the runner.
    pub expr: ExprId,
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

/// Metadata carried by an executable `emath law` declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LawMetadata {
    /// Preconditions under which the law is claimed.
    pub assumptions: Vec<String>,
    /// Mathematical or physical domain in which the law applies.
    pub domain: String,
    /// Source or derivation records.
    pub provenance: Vec<String>,
    /// Human-readable references.
    pub citations: Vec<String>,
}

/// One admitted `emath field_pack` declaration (v9-06-2rdq.16): the
/// pack's exports as artifact data. Packs compile to a semantic image /
/// `.emlib` consumed by layout/install tooling — admission never lowers
/// a pack into runnable meaning, and the exports carry no parser
/// surface (the section table is closed at admission).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldPackEntry {
    /// Pack declaration name; public identity is package path + name.
    pub name: String,
    /// Exported items in source order: `(export kind, name)` where the
    /// kind is one of the closed export vocabulary (`cell`, `theory`,
    /// `method`, `world`).
    pub exports: Vec<(String, String)>,
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
    /// Admitted capability cells. Domain operations are arena data: adding
    /// a cell appends here and never adds an `ExprNode` variant.
    pub capabilities: Vec<crate::capability::Capability>,
    /// Admitted `emath field_pack` declarations (v9-06-2rdq.16). Pack
    /// data appends here; adding a pack never adds a core variant.
    pub field_packs: Vec<FieldPackEntry>,
    /// Law-only metadata keyed by declaration id. Keeping this package-side
    /// leaves ordinary function/model declarations unchanged.
    pub law_metadata: BTreeMap<DeclarationId, LawMetadata>,
    /// Scientific provenance keyed by declaration-local binding site.
    /// This is semantic artifact data and participates in content identity.
    pub binding_provenance: BTreeMap<BindingSite, Provenance>,
    /// Causalized implicit residuals per model declaration; package-side
    /// so adding a section does not churn every `Declaration` literal.
    pub residuals: std::collections::BTreeMap<DeclarationId, Vec<ModelResidual>>,
    /// Hybrid event rules per model declaration (r3-dynamical-03lh ch7,
    /// event-execution slice): each declared event carries a `.emath`
    /// Boolean condition and one deterministic action. The runner fires
    /// an event at most once per rising edge of its condition; bare
    /// `event Name` / `event Name(f: T)` declarations (no payload
    /// suite) are surface-only and never scheduled. Package-side so
    /// adding the section does not churn every `Declaration` literal.
    pub events: std::collections::BTreeMap<DeclarationId, Vec<EventDecl>>,
    /// Transition rules per model declaration (r3-dynamical-03lh ch7,
    /// transitions slice): each `on <Event>:` rule attaches deterministic
    /// re-assignments to a declared event by name. The runner applies
    /// them when the event fires. Package-side so adding the section does
    /// not churn every `Declaration` literal.
    pub transitions: std::collections::BTreeMap<DeclarationId, Vec<TransitionDecl>>,
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
            capabilities: Vec::new(),
            field_packs: Vec::new(),
            law_metadata: BTreeMap::new(),
            binding_provenance: BTreeMap::new(),
            residuals: std::collections::BTreeMap::new(),
            events: std::collections::BTreeMap::new(),
            transitions: std::collections::BTreeMap::new(),
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

    /// Intern one capability cell. Adding a cell is arena growth: core IR
    /// enums stay fixed.
    pub fn push_capability(&mut self, capability: crate::capability::Capability) -> CapabilityId {
        self.capabilities.push(capability);
        CapabilityId(u32::try_from(self.capabilities.len() - 1).unwrap_or(u32::MAX))
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
    pub fn capability(&self, id: CapabilityId) -> Option<&crate::capability::Capability> {
        self.capabilities.get(id.index())
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

    /// Identity of admitted mathematics, independent of presentation,
    /// local names, evidence attachments and tests.
    pub fn meaning_id(
        &self,
        dependencies: &[emath_core::MeaningId],
    ) -> Result<emath_core::MeaningId, crate::meaning::MeaningError> {
        crate::meaning::meaning_id(self, dependencies)
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
