//! Provider skeleton: a Phase 2+ adapter seam, wired today as a typed
//! refusal. Phase 1 has no providers; this example demonstrates the
//! capability contract without faking support (Constitution §6: never
//! silently accept what you do not implement).

use emath_core::{ContentId, SchemaId, bootstrap_content_id};
use emath_ir::Goal;
use emath_provider_api::{
    CapabilityReason, CapabilityReport, CostEstimate, Provider, ProviderDescriptor, ProviderError,
    ProviderResult,
};
use emath_runtime::{Budget, Cancellation, NeverCancel, Outcome};

const PROVIDER_ID: &str = "skeleton.native-placeholder";
const PROVIDER_VERSION: &str = "0.0.0";

struct NativePlaceholderSkeleton {
    descriptor: ProviderDescriptor,
}

impl NativePlaceholderSkeleton {
    fn new() -> Self {
        Self {
            descriptor: ProviderDescriptor {
                schema: SchemaId("emath.provider".to_string()),
                id: PROVIDER_ID.to_string(),
                version: PROVIDER_VERSION.to_string(),
                implementation: bootstrap_content_id(b"provider-skeleton/native-placeholder"),
                goal_kinds: vec!["evaluate".to_string()],
                semantic_subsets: vec!["strict-f64".to_string()],
                targets: vec!["rust".to_string()],
                maximum_evidence: emath_ir::EvidenceLevel::E0,
                deterministic: true,
                permissions: vec![],
                checker_bindings: vec![],
            },
        }
    }
}

impl Provider for NativePlaceholderSkeleton {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn supports(&self, _goal: &Goal) -> CapabilityReport {
        CapabilityReport {
            // The skeleton advertises the *capability shape* but refuses
            // execution: Phase 1 substitutes no provider work.
            supported: false,
            reasons: vec![CapabilityReason {
                code: "PHASE1-NO-PROVIDERS".to_string(),
                detail: format!(
                    "Phase 1 ships the native path only; `{PROVIDER_ID}` is a typed-refusal skeleton, not a provider"
                ),
            }],
            estimated_cost: Some(CostEstimate {
                compile_work: 0,
                runtime_work: 0,
                memory_bytes: 0,
                confidence_basis: "skeleton (no execution)".to_string(),
            }),
        }
    }

    fn execute(
        &self,
        _plan: &emath_ir::ResolutionPlan,
        _budget: Budget,
        _cancellation: &dyn Cancellation,
    ) -> Outcome<ProviderResult, ProviderError> {
        Outcome::Unresolved {
            reason: emath_runtime::UnresolvedReason::UnsupportedSemanticSubset,
            partial: None,
            continuation: None,
            evidence: emath_runtime::EvidenceHandle {
                schema: self.descriptor.schema.clone(),
                identity: self.descriptor.implementation.clone(),
            },
        }
    }
}

fn main() {
    let provider = NativePlaceholderSkeleton::new();
    let report = provider.supports(&Goal {
        id: emath_ir::GoalId(0),
        kind: emath_ir::GoalKind::Evaluate,
        target: "score".to_string(),
        expression: None,
        requirements: emath_ir::GoalRequirements {
            evidence: emath_ir::EvidenceLevel::E1,
            exactness: emath_ir::ExactnessPolicy::Exact,
            determinism: emath_ir::DeterminismPolicy::Required,
            target: emath_ir::TargetProfile {
                family: "rust".to_string(),
                triple: None,
                features: vec![],
            },
            fallback: emath_ir::FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: emath_ir::GoalPayload::default(),
        source: emath_core::Span::default(),
    });
    assert!(
        !report.supported,
        "skeleton must refuse, never fake support"
    );
    let outcome = provider.execute(
        &emath_ir::ResolutionPlan {
            schema: SchemaId("emath.resolution-plan".to_string()),
            plan_id: ContentId(String::new()),
            goal: emath_ir::GoalId(0),
            policy: "native".to_string(),
            artifact_class: "native".to_string(),
            nodes: std::collections::BTreeMap::new(),
            root: emath_ir::PlanNodeId(0),
            excluded_candidates: vec![],
        },
        Budget::default(),
        &NeverCancel,
    );
    assert!(matches!(outcome, Outcome::Unresolved { .. }));
    println!("{PROVIDER_ID} refuses with a typed capability report (Phase 2 seam)");
}
