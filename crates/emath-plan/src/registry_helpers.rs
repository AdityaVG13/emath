//! Test/kit sample registry used by planner tests and CLI inspection.

use emath_ir::EvidenceLevel;
use emath_provider_api::{
    CapabilitySpec, CapabilityTable, ProviderIsolation, ProviderLock, ProviderRegistry,
    RegistryConfig, RepresentationSpec,
};

/// A static-only registry with four deterministic providers:
/// `exact-a`, `exact-b` (exact), `est-c` (estimate), `no-checker-d`
/// (exact but no checker binding, evidence ceiling E3).
#[must_use]
pub fn sample_registry() -> ProviderRegistry {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    for (id, exactness, max_evidence, checker) in [
        ("exact-a", &["exact"][..], EvidenceLevel::E2, true),
        ("exact-b", &["exact"][..], EvidenceLevel::E2, true),
        ("est-c", &["estimate"][..], EvidenceLevel::E1, false),
        ("no-checker-d", &["exact"][..], EvidenceLevel::E3, false),
    ] {
        let table = CapabilityTable {
            capabilities: vec![CapabilitySpec {
                name: format!("evaluate.{id}"),
                semantic_subset: "rust-library".into(),
                representations: vec![RepresentationSpec {
                    name: "f64".into(),
                    exact_relation: "bit-identical".into(),
                    encode_cost: 0,
                }],
                exactness: exactness.iter().map(|token| (*token).to_string()).collect(),
                failure_modes: vec![],
                checker_bindings: if checker {
                    vec!["sir-checker".into()]
                } else {
                    vec![]
                },
            }],
            isolation: ProviderIsolation::Static,
            lock: ProviderLock::Unlocked,
            maximum_evidence: max_evidence,
            deterministic: true,
        };
        registry
            .register(id, ProviderIsolation::Static, table)
            .expect("sample registration must succeed");
    }
    registry
}
