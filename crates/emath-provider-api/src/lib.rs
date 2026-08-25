//! Provider API: descriptors, capabilities, providers, adapters, checkers.
//!
//! Adapter law (Neutral IR Constitution §7): encode/decode carry a declared
//! relation R(E, E'); provider output is untrusted until a `ResultChecker`
//! admits it (§8). Phase 1 ships no concrete providers; this is the frozen
//! adapter seam for Phase 2+.

#![forbid(unsafe_code)]

pub mod constellation;
pub mod descriptor;
pub mod filter;
pub mod registry;

use emath_core::{ContentId, SchemaId};
use emath_ir::{EvidenceLevel, Goal, ResolutionPlan};
use emath_runtime::{Budget, Cancellation, Outcome};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub schema: SchemaId,
    pub id: String,
    pub version: String,
    pub implementation: ContentId,
    pub goal_kinds: Vec<String>,
    pub semantic_subsets: Vec<String>,
    pub targets: Vec<String>,
    pub maximum_evidence: EvidenceLevel,
    pub deterministic: bool,
    pub permissions: Vec<String>,
    pub checker_bindings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityReport {
    pub supported: bool,
    pub reasons: Vec<CapabilityReason>,
    pub estimated_cost: Option<CostEstimate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityReason {
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostEstimate {
    pub compile_work: u128,
    pub runtime_work: u128,
    pub memory_bytes: u64,
    pub confidence_basis: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub code: String,
    pub message: String,
}

/// Untrusted provider transport until admitted by a checker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderResult {
    pub schema: SchemaId,
    pub goal_identity: ContentId,
    pub payload: Vec<u8>,
    pub certificate: Option<Vec<u8>>,
    pub evidence_claims: Vec<String>,
}

pub trait Provider {
    fn descriptor(&self) -> &ProviderDescriptor;
    fn supports(&self, goal: &Goal) -> CapabilityReport;
    fn execute(
        &self,
        plan: &ResolutionPlan,
        budget: Budget,
        cancellation: &dyn Cancellation,
    ) -> Outcome<ProviderResult, ProviderError>;
}

/// Adapter between an emath semantic object `Source` and a provider
/// representation `Target`. The adapter declares its relation via
/// `relation()`; unsupported semantics must refuse before provider
/// execution (never silently approximate).
pub trait Adapter<Source, Target> {
    fn adapter_id(&self) -> &str;
    fn relation(&self) -> &'static str;
    fn supports(&self, source: &Source) -> CapabilityReport;
    fn encode(&self, source: &Source) -> Result<Target, ProviderError>;
    fn decode(&self, target: &Target) -> Result<Source, ProviderError>;
}

pub use constellation::{
    CompositionOutcome, ConstellationError, ConstellationLock, ConstellationProvider,
    MaturityLevel, MaturityRegistry, VersionEntry, compose_chain, default_constellation,
};
pub use descriptor::{
    CapabilitySpec, CapabilityTable, DescriptorProblem, ProviderIsolation, ProviderLock,
    RepresentationSpec, capability_token, lock_token,
};
pub use filter::{Compatibility, ExclusionReason, ProviderVerdict, filter_goal};
pub use registry::{ProviderRegistry, RegistryConfig, RegistryError};

/// Only `Admitted` results may satisfy a goal with authority.
pub trait ResultChecker {
    type Admitted;
    fn checker_id(&self) -> &str;
    fn check(&self, goal: &Goal, result: &ProviderResult) -> Result<Self::Admitted, ProviderError>;
}
