#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FeatureId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureClass {
    Syntax, SyntaxPack, Kind, Section, Symbol, Binder, Type, Constructor,
    Capability, Theory, Instance, Family, Method, World, Provider, Artifact,
    Diagnostic, Migration, FieldPack, Lens,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Maturity {
    Cataloged, Proposed, Accepted, Stable, Deprecated, Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Projection {
    Identity, Surface, Parse, Lowering, StaticSemantics, Reference, Worlds,
    Execution, Artifact, Diagnostics, Documentation, Tooling, Conformance,
    Migration, AgentView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureCapsule {
    pub id: FeatureId,
    pub class: FeatureClass,
    pub maturity: Maturity,
    pub summary: String,
    pub dependencies: BTreeSet<FeatureId>,
    pub required_projections: BTreeSet<Projection>,
    pub metadata: BTreeMap<String, String>,
}

impl FeatureCapsule {
    #[must_use]
    pub fn missing_projections(
        &self,
        present: &BTreeSet<Projection>,
    ) -> BTreeSet<Projection> {
        self.required_projections.difference(present).cloned().collect()
    }
}
