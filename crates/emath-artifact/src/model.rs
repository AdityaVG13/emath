//! Artifact manifests, source maps, plans, and evidence records.

use super::*;

pub const ARTIFACT_MANIFEST_SCHEMA: &str = "emath.artifact";
/// Artifact manifest document version (manifest v1). Bump on any change
/// to the manifest layout or to the identity preimage in
/// [`manifest_identity`]; consumers refuse versions they do not know.
pub const ARTIFACT_MANIFEST_VERSION: u32 = 1;
/// Durable artifact source map (byte-range + `source_package` shape; see
/// [`write_source_map`]). Distinct from the world-codegen provenance map
/// ([`GENERATED_CRATE_SOURCE_MAP_SCHEMA`]); the two never share an id.
pub const SOURCE_MAP_SCHEMA: &str = "emath.source-map";
/// World-codegen provenance map written next to a generated world crate
/// (see [`write_generated_crate_source_map`]). Distinct from
/// [`SOURCE_MAP_SCHEMA`]; the two documents must never share an id.
pub const GENERATED_CRATE_SOURCE_MAP_SCHEMA: &str = "emath.generated-crate-source-map";
/// JSON `$schema` id of the durable resolution-plan document
/// ([`write_resolution_plan`]). The plan identity preimage is
/// `plan_identity` over a `plan:` payload, not this document id.
pub const RESOLUTION_PLAN_SCHEMA: &str = "emath.resolution-plan";
pub const EVIDENCE_BUNDLE_SCHEMA: &str = "emath.evidence-bundle";

/// The seven total-artifact-protocol classes. Compilation is total over
/// this set: every accepted intent resolves to an artifact of some class,
/// and resolution monotonicity requires that adding providers or budgets
/// never destroys a class that was previously reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactClass {
    Native,
    Portfolio,
    Hybrid,
    Parametric,
    Exploration,
    Continuation,
    Diagnostic,
}

impl ArtifactClass {
    /// All seven classes in stable protocol order.
    pub const ALL: [Self; 7] = [
        Self::Native,
        Self::Portfolio,
        Self::Hybrid,
        Self::Parametric,
        Self::Exploration,
        Self::Continuation,
        Self::Diagnostic,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Portfolio => "portfolio",
            Self::Hybrid => "hybrid",
            Self::Parametric => "parametric",
            Self::Exploration => "exploration",
            Self::Continuation => "continuation",
            Self::Diagnostic => "diagnostic",
        }
    }
}

impl std::str::FromStr for ArtifactClass {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native" => Ok(Self::Native),
            "portfolio" => Ok(Self::Portfolio),
            "hybrid" => Ok(Self::Hybrid),
            "parametric" => Ok(Self::Parametric),
            "exploration" => Ok(Self::Exploration),
            "continuation" => Ok(Self::Continuation),
            "diagnostic" => Ok(Self::Diagnostic),
            _ => Err(()),
        }
    }
}

/// `emath.artifact`
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactManifest {
    pub schema: SchemaId,
    pub artifact_id: ContentId,
    pub class: ArtifactClass,
    pub source_package: ContentId,
    pub compiler: ContentId,
    pub target: TargetProfile,
    pub numeric_profile: String,
    pub providers: Vec<emath_ir::ProviderRef>,
    pub evidence_level: EvidenceLevel,
    pub public_exports: Vec<String>,
    pub assumptions: Vec<String>,
    pub files: BTreeMap<String, ContentId>,
    pub source_map: ContentId,
    pub resolution_plan: ContentId,
    pub evidence_bundle: ContentId,
}

/// One `emath.source-map` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// Source file id (index into the session source store).
    pub file: u32,
    pub source_file: String,
    pub source_start: u64,
    pub source_end: u64,
    pub semantic_node: String,
    pub plan_node: Option<String>,
    pub generated_file: String,
    pub generated_start: u64,
    pub generated_end: u64,
    pub generated_symbol: Option<String>,
}

/// `emath.source-map`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub schema: SchemaId,
    pub source_package: ContentId,
    pub entries: Vec<SourceMapEntry>,
}

/// `emath.resolution-plan` (provider-free Phase 1 mirror of GIR plan).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRecord {
    pub schema: SchemaId,
    pub plan_id: ContentId,
    pub goal: u32,
    pub policy: String,
    pub artifact_class: String,
    pub operations: Vec<OperationRecord>,
    pub excluded_candidates: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
    pub node: u32,
    pub operation: String,
    pub dependencies: Vec<u32>,
    pub fallback: Option<u32>,
}

/// `emath.evidence-bundle`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceBundleRecord {
    pub schema: SchemaId,
    pub bundle_id: ContentId,
    pub source_package: ContentId,
    pub resolution_plan: ContentId,
    pub claims: Vec<EvidenceClaim>,
    pub artifact_paths: Vec<String>,
    pub reproduction: Vec<String>,
}
