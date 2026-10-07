//! Meaning spine: typed edges between features and the IR, test and
//! external resources they depend on, with impact, migration-reachability
//! and minimum-agent-context queries over that graph.

use emath_core::FeatureId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::str::FromStr;

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
