//! Runtime-neutral Feature Capsule records.
//!
//! Capsules are language data. This module contains no feature-specific
//! operation enum or parser branch; `emath-schema` owns the restricted source
//! decoder and validates these records against the class table.

use std::collections::BTreeMap;
use std::fmt;

use emath_core::{ContentId, FeatureId, SemanticHash};

/// Stable, unversioned Feature Capsule schema name.
pub const FEATURE_CAPSULE_SCHEMA: &str = "emath.feature-capsule";

/// The twenty retained capsule classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureClass {
    Constitution,
    Syntax,
    Kind,
    Section,
    Surface,
    Symbol,
    Type,
    Binder,
    Capability,
    Theory,
    Instance,
    Goal,
    Method,
    World,
    Provider,
    Effect,
    Artifact,
    Diagnostic,
    Migration,
    FieldPack,
}

impl FeatureClass {
    pub const ALL: [Self; 20] = [
        Self::Constitution,
        Self::Syntax,
        Self::Kind,
        Self::Section,
        Self::Surface,
        Self::Symbol,
        Self::Type,
        Self::Binder,
        Self::Capability,
        Self::Theory,
        Self::Instance,
        Self::Goal,
        Self::Method,
        Self::World,
        Self::Provider,
        Self::Effect,
        Self::Artifact,
        Self::Diagnostic,
        Self::Migration,
        Self::FieldPack,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Constitution => "constitution",
            Self::Syntax => "syntax",
            Self::Kind => "kind",
            Self::Section => "section",
            Self::Surface => "surface",
            Self::Symbol => "symbol",
            Self::Type => "type",
            Self::Binder => "binder",
            Self::Capability => "capability",
            Self::Theory => "theory",
            Self::Instance => "instance",
            Self::Goal => "goal",
            Self::Method => "method",
            Self::World => "world",
            Self::Provider => "provider",
            Self::Effect => "effect",
            Self::Artifact => "artifact",
            Self::Diagnostic => "diagnostic",
            Self::Migration => "migration",
            Self::FieldPack => "field_pack",
        }
    }
}

impl fmt::Display for FeatureClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for FeatureClass {
    type Err = CapsuleRecordError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|class| class.as_str() == value)
            .ok_or_else(|| CapsuleRecordError::UnknownClass(value.to_string()))
    }
}

/// Coverage maturity. Catalog presence is deliberately not live authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Maturity {
    Cataloged,
    Proposed,
    Accepted,
    Stable,
    Deprecated,
    Retired,
}

impl Maturity {
    pub const ALL: [Self; 6] = [
        Self::Cataloged,
        Self::Proposed,
        Self::Accepted,
        Self::Stable,
        Self::Deprecated,
        Self::Retired,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cataloged => "cataloged",
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Stable => "stable",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }

    /// Legal direct transition. Reversal is restricted to deprecated → stable
    /// and retired → deprecated; all other movement is one step forward.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Cataloged, Self::Proposed)
                | (Self::Proposed, Self::Accepted)
                | (Self::Accepted, Self::Stable)
                | (Self::Stable, Self::Deprecated)
                | (Self::Deprecated, Self::Retired)
                | (Self::Deprecated, Self::Stable)
                | (Self::Retired, Self::Deprecated)
        )
    }
}

impl std::str::FromStr for Maturity {
    type Err = CapsuleRecordError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|maturity| maturity.as_str() == value)
            .ok_or_else(|| CapsuleRecordError::UnknownMaturity(value.to_string()))
    }
}

/// A capsule field is either concrete data, a typed non-applicability, or an
/// explicit Spec Hole. Missing work is never encoded as N/A.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapsuleSlot {
    Value(String),
    NotApplicable { rule: String, reason: String },
    Hole { gate: String, reason: String },
}

impl CapsuleSlot {
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Value(value) => format!("value({}:{value})", value.len()),
            Self::NotApplicable { rule, reason } => {
                format!("n/a({}:{rule},{}:{reason})", rule.len(), reason.len())
            }
            Self::Hole { gate, reason } => {
                format!("hole({}:{gate},{}:{reason})", gate.len(), reason.len())
            }
        }
    }

    #[must_use]
    pub const fn is_hole(&self) -> bool {
        matches!(self, Self::Hole { .. })
    }
}

/// Typed semantic/resource edge. The exact edge vocabulary is validated by the
/// Meaning Spine; this record prevents untyped string adjacency.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CapsuleEdge {
    pub kind: String,
    pub target: FeatureId,
}

/// Projection declaration retained through canonical round trips.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CapsuleProjection {
    pub name: String,
    pub disposition: ProjectionDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectionDisposition {
    Required,
    Provided,
    Generated,
    Provider(String),
    NotApplicable { rule: String, reason: String },
    Hole { gate: String, reason: String },
}

impl ProjectionDisposition {
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Required => "required".to_string(),
            Self::Provided => "provided".to_string(),
            Self::Generated => "generated".to_string(),
            Self::Provider(provider) => format!("provider({}:{provider})", provider.len()),
            Self::NotApplicable { rule, reason } => {
                format!("n/a({}:{rule},{}:{reason})", rule.len(), reason.len())
            }
            Self::Hole { gate, reason } => {
                format!("hole({}:{gate},{}:{reason})", gate.len(), reason.len())
            }
        }
    }

    #[must_use]
    pub const fn is_blocking_hole(&self) -> bool {
        matches!(self, Self::Hole { .. })
    }
}

/// One complete Feature Capsule. Section-like material remains neutral strings;
/// class schemas decide which slots require concrete values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureCapsule {
    pub schema: String,
    pub feature_id: FeatureId,
    pub semantic_hash: SemanticHash,
    pub class: FeatureClass,
    pub maturity: Maturity,
    pub summary: String,
    pub source: String,
    pub edges: Vec<CapsuleEdge>,
    pub slots: BTreeMap<String, CapsuleSlot>,
    pub projections: Vec<CapsuleProjection>,
}

impl FeatureCapsule {
    /// Byte-stable canonical representation. Ordering is independent of source
    /// order; exact lengths prevent separator ambiguity.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn field(out: &mut Vec<u8>, name: &str, value: &[u8]) {
            out.extend_from_slice(&(name.len() as u64).to_be_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(value.len() as u64).to_be_bytes());
            out.extend_from_slice(value);
        }

        let mut out = Vec::new();
        field(&mut out, "schema", self.schema.as_bytes());
        field(&mut out, "feature_id", self.feature_id.as_str().as_bytes());
        field(
            &mut out,
            "semantic_hash",
            self.semantic_hash.as_str().as_bytes(),
        );
        field(&mut out, "class", self.class.as_str().as_bytes());
        field(&mut out, "maturity", self.maturity.as_str().as_bytes());
        field(&mut out, "summary", self.summary.as_bytes());
        field(&mut out, "source", self.source.as_bytes());

        let mut edges = self.edges.clone();
        edges.sort();
        for edge in edges {
            field(&mut out, "edge_kind", edge.kind.as_bytes());
            field(&mut out, "edge_target", edge.target.as_str().as_bytes());
        }
        for (name, slot) in &self.slots {
            field(&mut out, name, slot.canonical().as_bytes());
        }
        let mut projections = self.projections.clone();
        projections.sort();
        for projection in projections {
            field(&mut out, "projection_name", projection.name.as_bytes());
            field(
                &mut out,
                "projection_disposition",
                projection.disposition.canonical().as_bytes(),
            );
        }
        out
    }

    #[must_use]
    pub fn has_blocking_hole(&self) -> bool {
        self.slots.values().any(CapsuleSlot::is_hole)
            || self
                .projections
                .iter()
                .any(|projection| projection.disposition.is_blocking_hole())
    }
}

/// Explicit bridge for a compatible legacy capability cell. The legacy bytes
/// and their provenance stay addressable; they are never silently reinterpreted
/// as a FeatureID or capsule hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyCellMapping {
    pub legacy_cell_id: ContentId,
    pub feature_id: FeatureId,
    pub authority_provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapsuleRecordError {
    UnknownClass(String),
    UnknownMaturity(String),
}

impl fmt::Display for CapsuleRecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownClass(class) => {
                write!(formatter, "unknown Feature Capsule class `{class}`")
            }
            Self::UnknownMaturity(maturity) => {
                write!(formatter, "unknown Feature Capsule maturity `{maturity}`")
            }
        }
    }
}

impl std::error::Error for CapsuleRecordError {}
