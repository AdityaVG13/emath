//! MIG: the mathematical intent graph (schema `emath.mig`, version 1)
//! between HIR admission and SIR semantics: one node per intent-bearing
//! element, typed edges from each declaration to the intents it owns; six
//! semantic planes (definition/construction/goal/evidence/execution/evolution)
//! represented losslessly. Span-free by construction (identity excludes
//! presentation-only data).

use crate::canonical::canonical_expr;
use crate::package::{Declaration, SemanticPackage};
use emath_core::{ContentId, FeatureId, SchemaId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::str::FromStr;

/// Versioned MIG schema id.
pub const MIG_SCHEMA: &str = "emath.mig";
/// MIG schema version.
pub const MIG_SCHEMA_VERSION: u32 = 1;

/// MIG node id (index into `Mig::nodes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MigNodeId(pub usize);

/// The intent plane a node belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigNodeKind {
    /// The declaration itself (owner of every other node).
    Declaration,
    /// Definition plane: an input field.
    Input,
    /// Definition plane: an output field.
    Output,
    /// Definition plane: a state field.
    State,
    /// Definition plane: a named definition body.
    Definition,
    /// Construction plane: a constructor.
    Constructor,
    /// Construction plane: a constructor obligation (require/ensure).
    Obligation,
    /// Construction plane: a `Self:` field assignment.
    Assignment,
    /// Evidence plane: a declaration invariant.
    Invariant,
    /// Goal plane: a resolution goal.
    Goal,
    /// Evidence plane: an example test.
    Test,
    /// Execution plane: the compile specification.
    CompileSpec,
    /// Evolution plane: an export.
    Export,
}

impl MigNodeKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::Input => "input",
            Self::Output => "output",
            Self::State => "state",
            Self::Definition => "definition",
            Self::Constructor => "constructor",
            Self::Obligation => "obligation",
            Self::Assignment => "assignment",
            Self::Invariant => "invariant",
            Self::Goal => "goal",
            Self::Test => "test",
            Self::CompileSpec => "compile-spec",
            Self::Export => "export",
        }
    }
}

/// One intent node: kind, semantic label and optional expression content
/// (canonical, span-free).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigNode {
    /// Node id.
    pub id: MigNodeId,
    /// Intent plane.
    pub kind: MigNodeKind,
    /// Semantic label (name, target or specification token). Never a span.
    pub label: String,
    /// Canonical content id of the carried expression, if any.
    pub content: Option<ContentId>,
}

/// Typed intent edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigEdgeKind {
    /// Declaration owns a field / constructor / compile spec / export.
    Owns,
    /// Declaration defines a named body.
    Defines,
    /// Constructor requires an obligation before field init.
    Requires,
    /// Constructor (or declaration) ensures an obligation after init.
    Ensures,
    /// Constructor assigns a state field.
    Assigns,
    /// Declaration targets a goal.
    Targets,
    /// Declaration is checked by an example test.
    Tests,
}

impl MigEdgeKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Owns => "owns",
            Self::Defines => "defines",
            Self::Requires => "requires",
            Self::Ensures => "ensures",
            Self::Assigns => "assigns",
            Self::Targets => "targets",
            Self::Tests => "tests",
        }
    }
}

/// One edge in the intent graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigEdge {
    /// Source node.
    pub from: MigNodeId,
    /// Edge kind.
    pub kind: MigEdgeKind,
    /// Destination node.
    pub to: MigNodeId,
}

/// The mathematical intent graph of a semantic package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mig {
    /// Versioned schema id (`emath.mig.v1`).
    pub schema: SchemaId,
    /// Nodes in deterministic derivation order.
    pub nodes: Vec<MigNode>,
    /// Edges in deterministic derivation order.
    pub edges: Vec<MigEdge>,
}

impl Mig {
    /// Derives the intent graph from an admitted package. Derivation is
    /// deterministic: declaration order, field order, sorted definition /
    /// assignment maps, goal/test id order.
    #[must_use]
    pub fn from_package(package: &SemanticPackage) -> Self {
        let mut graph = Self {
            schema: SchemaId(format!("{MIG_SCHEMA}.v{MIG_SCHEMA_VERSION}")),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        for declaration in &package.declarations {
            graph.add_declaration(package, declaration);
        }
        graph
    }

    fn push_node(
        &mut self,
        kind: MigNodeKind,
        label: impl Into<String>,
        content: Option<ContentId>,
    ) -> MigNodeId {
        let id = MigNodeId(self.nodes.len());
        self.nodes.push(MigNode {
            id,
            kind,
            label: label.into(),
            content,
        });
        id
    }

    fn push_edge(&mut self, from: MigNodeId, kind: MigEdgeKind, to: MigNodeId) {
        self.edges.push(MigEdge { from, kind, to });
    }

    fn add_declaration(&mut self, package: &SemanticPackage, declaration: &Declaration) {
        let owner = self.push_node(MigNodeKind::Declaration, declaration.name.0.clone(), None);
        for (fields, kind) in [
            (&declaration.inputs, MigNodeKind::Input),
            (&declaration.outputs, MigNodeKind::Output),
            (&declaration.state, MigNodeKind::State),
        ] {
            for field in fields {
                let node = self.push_node(kind, field.name.clone(), None);
                self.push_edge(owner, MigEdgeKind::Owns, node);
            }
        }
        for (name, expr) in &declaration.definitions {
            let content = Some(canonical_expr(package, *expr));
            let node = self.push_node(MigNodeKind::Definition, name.clone(), content);
            self.push_edge(owner, MigEdgeKind::Defines, node);
        }
        for constructor in &declaration.constructors {
            let node = self.push_node(MigNodeKind::Constructor, constructor.name.clone(), None);
            self.push_edge(owner, MigEdgeKind::Owns, node);
            for precondition in &constructor.preconditions {
                let content = Some(canonical_expr(package, *precondition));
                let obligation = self.push_node(MigNodeKind::Obligation, "require", content);
                self.push_edge(node, MigEdgeKind::Requires, obligation);
            }
            for (field, expr) in &constructor.assignments {
                let content = Some(canonical_expr(package, *expr));
                let assignment = self.push_node(MigNodeKind::Assignment, field.clone(), content);
                self.push_edge(node, MigEdgeKind::Assigns, assignment);
            }
            for postcondition in &constructor.postconditions {
                let content = Some(canonical_expr(package, *postcondition));
                let obligation = self.push_node(MigNodeKind::Obligation, "ensure", content);
                self.push_edge(node, MigEdgeKind::Ensures, obligation);
            }
        }
        for invariant in &declaration.invariants {
            let content = Some(canonical_expr(package, *invariant));
            let node = self.push_node(MigNodeKind::Invariant, "invariant", content);
            self.push_edge(owner, MigEdgeKind::Ensures, node);
        }
        for claim in &declaration.evidence {
            let node = self.push_node(MigNodeKind::Invariant, format!("claim:{}", claim.id), None);
            self.push_edge(owner, MigEdgeKind::Ensures, node);
        }
        for goal_id in &declaration.goals {
            if let Some(goal) = package.goals.get(goal_id.index()) {
                let label = format!(
                    "{}:{}:{}",
                    goal.kind.as_str(),
                    goal.target,
                    goal.requirements.produce
                );
                let content = goal
                    .expression
                    .map(|expression| canonical_expr(package, expression));
                let node = self.push_node(MigNodeKind::Goal, label, content);
                self.push_edge(owner, MigEdgeKind::Targets, node);
            }
        }
        for test_id in &declaration.tests {
            if let Some(test) = package.tests.get(test_id.index()) {
                let content = test.expect.map(|expect| canonical_expr(package, expect));
                let node = self.push_node(MigNodeKind::Test, test.name.clone(), content);
                self.push_edge(owner, MigEdgeKind::Tests, node);
            }
        }
        let spec = &declaration.compile_spec;
        let compile_label = format!(
            "{}:{}:{}:{}",
            spec.target,
            spec.profile,
            spec.numeric.as_str(),
            spec.safety.as_str()
        );
        let compile = self.push_node(MigNodeKind::CompileSpec, compile_label, None);
        self.push_edge(owner, MigEdgeKind::Owns, compile);
        for export in &declaration.exports {
            let node = self.push_node(
                MigNodeKind::Export,
                format!("{}:{}", export.kind, export.name),
                None,
            );
            self.push_edge(owner, MigEdgeKind::Owns, node);
        }
    }

    /// Deterministic canonical encoding: schema header, one row per node
    /// (kind, label, content), one row per edge. Span-free by construction.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.schema.0);
        out.push('\n');
        for node in &self.nodes {
            out.push_str("node ");
            out.push_str(&node.id.0.to_string());
            out.push(' ');
            out.push_str(node.kind.name());
            out.push(' ');
            out.push_str(&node.label);
            out.push(' ');
            out.push_str(node.content.as_ref().map_or("-", |content| &content.0));
            out.push('\n');
        }
        for edge in &self.edges {
            out.push_str("edge ");
            out.push_str(&edge.from.0.to_string());
            out.push(' ');
            out.push_str(edge.kind.name());
            out.push(' ');
            out.push_str(&edge.to.0.to_string());
            out.push('\n');
        }
        out
    }

    /// Semantic identity of the intent graph (excludes presentation-only
    /// changes: spans and formatting never enter the derivation).
    #[must_use]
    pub fn identity(&self) -> ContentId {
        emath_core::hash::bootstrap_content_id(self.canonical().as_bytes())
    }
}

/// Typed endpoint in the feature/resource portion of the mathematical intent
/// graph. External resources are restricted to three unambiguous schemes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeaningResource {
    Feature(FeatureId),
    Ir(String),
    Test(String),
    Doc(String),
}

impl MeaningResource {
    pub fn parse(value: &str) -> Result<Self, MeaningSpineError> {
        for (scheme, constructor) in [
            ("ir://", Self::Ir as fn(String) -> Self),
            ("test://", Self::Test as fn(String) -> Self),
            ("doc://", Self::Doc as fn(String) -> Self),
        ] {
            if let Some(path) = value.strip_prefix(scheme) {
                if valid_resource_path(path) {
                    return Ok(constructor(path.to_string()));
                }
                return Err(MeaningSpineError::AmbiguousResource(value.to_string()));
            }
        }
        if value.contains("://") {
            return Err(MeaningSpineError::AmbiguousResource(value.to_string()));
        }
        FeatureId::from_str(value)
            .map(Self::Feature)
            .map_err(|_| MeaningSpineError::AmbiguousResource(value.to_string()))
    }

    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Feature(id) => id.to_string(),
            Self::Ir(path) => format!("ir://{path}"),
            Self::Test(path) => format!("test://{path}"),
            Self::Doc(path) => format!("doc://{path}"),
        }
    }
}

fn valid_resource_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains("//")
        && path.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'/' | b'_' | b'-' | b'.')
        })
}

/// Exact twelve-kind Meaning Spine vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeaningEdgeKind {
    DependsOn,
    Implements,
    Defines,
    Uses,
    RequiresWorld,
    ProvidedBy,
    Emits,
    Documents,
    ConformsTo,
    MigratesFrom,
    Replaces,
    ProjectsTo,
}

impl MeaningEdgeKind {
    pub const ALL: [Self; 12] = [
        Self::DependsOn,
        Self::Implements,
        Self::Defines,
        Self::Uses,
        Self::RequiresWorld,
        Self::ProvidedBy,
        Self::Emits,
        Self::Documents,
        Self::ConformsTo,
        Self::MigratesFrom,
        Self::Replaces,
        Self::ProjectsTo,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DependsOn => "depends_on",
            Self::Implements => "implements",
            Self::Defines => "defines",
            Self::Uses => "uses",
            Self::RequiresWorld => "requires_world",
            Self::ProvidedBy => "provided_by",
            Self::Emits => "emits",
            Self::Documents => "documents",
            Self::ConformsTo => "conforms_to",
            Self::MigratesFrom => "migrates_from",
            Self::Replaces => "replaces",
            Self::ProjectsTo => "projects_to",
        }
    }

    #[must_use]
    pub const fn contributes_to_build(self) -> bool {
        matches!(
            self,
            Self::DependsOn
                | Self::Implements
                | Self::Defines
                | Self::Uses
                | Self::RequiresWorld
                | Self::ProvidedBy
        )
    }

    #[must_use]
    pub const fn forbids_cycles(self) -> bool {
        self.contributes_to_build() || matches!(self, Self::MigratesFrom | Self::Replaces)
    }
}

impl FromStr for MeaningEdgeKind {
    type Err = MeaningSpineError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| MeaningSpineError::UnknownEdgeKind(value.to_string()))
    }
}

/// One canonical typed edge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeaningEdge {
    pub source: MeaningResource,
    pub kind: MeaningEdgeKind,
    pub target: MeaningResource,
}

/// Feature/resource projection of MIG. It is deliberately not a generalized
/// graph API: its endpoints, edges, closures, and cycle policies are closed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeaningSpine {
    resources: BTreeMap<MeaningResource, Option<crate::FeatureClass>>,
    edges: BTreeSet<MeaningEdge>,
}

impl MeaningSpine {
    pub fn register_feature(&mut self, id: FeatureId, class: crate::FeatureClass) {
        self.resources
            .insert(MeaningResource::Feature(id), Some(class));
    }

    pub fn register_external(
        &mut self,
        resource: MeaningResource,
    ) -> Result<(), MeaningSpineError> {
        if matches!(resource, MeaningResource::Feature(_)) {
            return Err(MeaningSpineError::EndpointMismatch {
                kind: MeaningEdgeKind::DependsOn,
                source: resource.clone(),
                target: resource,
            });
        }
        self.resources.insert(resource, None);
        Ok(())
    }

    pub fn insert(&mut self, edge: MeaningEdge) -> Result<(), MeaningSpineError> {
        if !self.resources.contains_key(&edge.source) {
            return Err(MeaningSpineError::Unresolved(edge.source.clone()));
        }
        if !self.resources.contains_key(&edge.target) {
            return Err(MeaningSpineError::Unresolved(edge.target.clone()));
        }
        if !self.endpoint_is_legal(&edge) {
            return Err(MeaningSpineError::EndpointMismatch {
                kind: edge.kind,
                source: edge.source,
                target: edge.target,
            });
        }
        if self.edges.contains(&edge) {
            return Err(MeaningSpineError::Duplicate(edge));
        }
        if edge.kind.forbids_cycles() && self.would_cycle(&edge) {
            return Err(MeaningSpineError::Cycle {
                kind: edge.kind,
                witness: vec![edge.target.clone(), edge.source.clone(), edge.target],
            });
        }
        self.edges.insert(edge);
        Ok(())
    }

    fn endpoint_is_legal(&self, edge: &MeaningEdge) -> bool {
        use MeaningEdgeKind as K;
        use MeaningResource as R;
        let source_feature = matches!(edge.source, R::Feature(_));
        match edge.kind {
            K::DependsOn | K::Defines | K::MigratesFrom | K::Replaces => {
                source_feature && matches!(edge.target, R::Feature(_))
            }
            K::Implements => source_feature && matches!(edge.target, R::Ir(_)),
            K::Uses => source_feature && matches!(edge.target, R::Feature(_) | R::Ir(_)),
            K::RequiresWorld => {
                source_feature
                    && self.resources.get(&edge.target) == Some(&Some(crate::FeatureClass::World))
            }
            K::ProvidedBy => {
                source_feature
                    && self.resources.get(&edge.target)
                        == Some(&Some(crate::FeatureClass::Provider))
            }
            K::Emits => {
                source_feature
                    && (matches!(edge.target, R::Ir(_))
                        || self.resources.get(&edge.target)
                            == Some(&Some(crate::FeatureClass::Artifact)))
            }
            K::Documents => source_feature && matches!(edge.target, R::Doc(_)),
            K::ConformsTo => source_feature && matches!(edge.target, R::Test(_)),
            K::ProjectsTo => source_feature && matches!(edge.target, R::Ir(_) | R::Doc(_)),
        }
    }

    fn would_cycle(&self, candidate: &MeaningEdge) -> bool {
        let mut queue = VecDeque::from([candidate.target.clone()]);
        let mut seen = BTreeSet::new();
        while let Some(current) = queue.pop_front() {
            if current == candidate.source {
                return true;
            }
            if !seen.insert(current.clone()) {
                continue;
            }
            for edge in &self.edges {
                let same_cycle_family = if candidate.kind.contributes_to_build() {
                    edge.kind.contributes_to_build()
                } else {
                    edge.kind == candidate.kind
                };
                if same_cycle_family && edge.source == current {
                    queue.push_back(edge.target.clone());
                }
            }
        }
        false
    }

    #[must_use]
    pub fn canonical_edges(&self) -> Vec<MeaningEdge> {
        self.edges.iter().cloned().collect()
    }

    #[must_use]
    pub fn canonical(&self) -> String {
        let mut output = String::new();
        for resource in self.resources.keys() {
            output.push_str("resource ");
            output.push_str(&resource.canonical());
            output.push('\n');
        }
        for edge in &self.edges {
            output.push_str("edge ");
            output.push_str(&edge.source.canonical());
            output.push(' ');
            output.push_str(edge.kind.as_str());
            output.push(' ');
            output.push_str(&edge.target.canonical());
            output.push('\n');
        }
        output
    }

    #[must_use]
    pub fn direct_dependencies(&self, feature: &FeatureId) -> Vec<MeaningResource> {
        let source = MeaningResource::Feature(feature.clone());
        self.edges
            .iter()
            .filter(|edge| edge.source == source && edge.kind.contributes_to_build())
            .map(|edge| edge.target.clone())
            .collect()
    }

    #[must_use]
    pub fn transitive_build_dependencies(&self, feature: &FeatureId) -> Vec<MeaningResource> {
        self.forward_closure(
            [MeaningResource::Feature(feature.clone())],
            MeaningEdgeKind::contributes_to_build,
        )
    }

    #[must_use]
    pub fn reverse_impact(&self, changed: &MeaningResource) -> Vec<MeaningResource> {
        let mut queue = VecDeque::from([changed.clone()]);
        let mut seen = BTreeSet::new();
        while let Some(current) = queue.pop_front() {
            for edge in &self.edges {
                if edge.target == current && seen.insert(edge.source.clone()) {
                    queue.push_back(edge.source.clone());
                }
                if edge.source == current
                    && matches!(
                        edge.kind,
                        MeaningEdgeKind::Documents
                            | MeaningEdgeKind::ConformsTo
                            | MeaningEdgeKind::ProjectsTo
                            | MeaningEdgeKind::Emits
                    )
                    && seen.insert(edge.target.clone())
                {
                    queue.push_back(edge.target.clone());
                }
            }
        }
        seen.remove(changed);
        seen.into_iter().collect()
    }

    #[must_use]
    pub fn migration_reachability(&self, old: &FeatureId) -> Vec<MeaningResource> {
        let old = MeaningResource::Feature(old.clone());
        let mut result = BTreeSet::new();
        let mut queue = VecDeque::from([old.clone()]);
        while let Some(current) = queue.pop_front() {
            for edge in &self.edges {
                if matches!(
                    edge.kind,
                    MeaningEdgeKind::MigratesFrom | MeaningEdgeKind::Replaces
                ) && edge.target == current
                    && result.insert(edge.source.clone())
                {
                    queue.push_back(edge.source.clone());
                }
            }
        }
        result.into_iter().collect()
    }

    #[must_use]
    pub fn conformance_requirements(&self, feature: &FeatureId) -> Vec<MeaningResource> {
        let source = MeaningResource::Feature(feature.clone());
        self.edges
            .iter()
            .filter(|edge| edge.source == source && edge.kind == MeaningEdgeKind::ConformsTo)
            .map(|edge| edge.target.clone())
            .collect()
    }

    fn forward_closure(
        &self,
        roots: impl IntoIterator<Item = MeaningResource>,
        include: impl Fn(MeaningEdgeKind) -> bool,
    ) -> Vec<MeaningResource> {
        let roots = roots.into_iter().collect::<BTreeSet<_>>();
        let mut queue = roots.iter().cloned().collect::<VecDeque<_>>();
        let mut seen = BTreeSet::new();
        while let Some(current) = queue.pop_front() {
            for edge in &self.edges {
                if edge.source == current && include(edge.kind) && seen.insert(edge.target.clone())
                {
                    queue.push_back(edge.target.clone());
                }
            }
        }
        for root in roots {
            seen.remove(&root);
        }
        seen.into_iter().collect()
    }

    #[must_use]
    pub fn minimum_agent_context(&self, capsule: &crate::FeatureCapsule) -> AgentContext {
        let agent = capsule
            .slots
            .get("agent")
            .and_then(|slot| match slot {
                crate::CapsuleSlot::Value(value) => Some(value.as_str()),
                _ => None,
            })
            .unwrap_or("");
        let fields = agent
            .split(';')
            .filter_map(|entry| entry.split_once('='))
            .map(|(key, value)| (key.trim(), value.trim()))
            .collect::<BTreeMap<_, _>>();
        AgentContext {
            feature: capsule.feature_id.clone(),
            direct_dependencies: self.direct_dependencies(&capsule.feature_id),
            owner_contract: fields.get("owners").copied().unwrap_or("").to_string(),
            hazards: fields.get("hazards").copied().unwrap_or("").to_string(),
            conformance: self.conformance_requirements(&capsule.feature_id),
            migrations: self.migration_reachability(&capsule.feature_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentContext {
    pub feature: FeatureId,
    pub direct_dependencies: Vec<MeaningResource>,
    pub owner_contract: String,
    pub hazards: String,
    pub conformance: Vec<MeaningResource>,
    pub migrations: Vec<MeaningResource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeaningSpineError {
    AmbiguousResource(String),
    UnknownEdgeKind(String),
    Unresolved(MeaningResource),
    EndpointMismatch {
        kind: MeaningEdgeKind,
        source: MeaningResource,
        target: MeaningResource,
    },
    Duplicate(MeaningEdge),
    Cycle {
        kind: MeaningEdgeKind,
        witness: Vec<MeaningResource>,
    },
}

impl std::fmt::Display for MeaningSpineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AmbiguousResource(resource) => {
                write!(formatter, "ambiguous meaning resource `{resource}`")
            }
            Self::UnknownEdgeKind(kind) => {
                write!(formatter, "unknown Meaning Spine edge kind `{kind}`")
            }
            Self::Unresolved(resource) => write!(
                formatter,
                "unresolved meaning resource `{}`",
                resource.canonical()
            ),
            Self::EndpointMismatch {
                kind,
                source,
                target,
            } => write!(
                formatter,
                "illegal endpoints for `{}`: {} -> {}",
                kind.as_str(),
                source.canonical(),
                target.canonical()
            ),
            Self::Duplicate(edge) => write!(
                formatter,
                "duplicate semantic edge {} {} {}",
                edge.source.canonical(),
                edge.kind.as_str(),
                edge.target.canonical()
            ),
            Self::Cycle { kind, witness } => write!(
                formatter,
                "forbidden `{}` cycle: {}",
                kind.as_str(),
                witness
                    .iter()
                    .map(MeaningResource::canonical)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ),
        }
    }
}

impl std::error::Error for MeaningSpineError {}

// MIG intent-graph tests moved to `tests/emath-ir/tests/mig.rs`.
