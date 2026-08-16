//!: compatibility filter.
//!
//! Every exclusion carries a stable reason code and provenance
//! (`E-PROV-512` goal kind/subset, `E-PROV-513` evidence/checker,
//! `E-PROV-514` target, `E-PROV-515` exactness, `E-PROV-516` determinism).
//! Results are deterministically ordered: compatible first, then excluded,
//! both by provider id.

use crate::descriptor::{CapabilitySpec, ProviderIsolation};
use crate::registry::ProviderRegistry;
use emath_ir::{EvidenceLevel, ExactnessPolicy, Goal};

/// One exclusion with reason and provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExclusionReason {
    /// Stable code (`E-PROV-512`..`E-PROV-516`).
    pub code: &'static str,
    /// Explanation.
    pub detail: String,
    /// Provenance: registration origin.
    pub provenance: String,
}

/// Filter verdict for one candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compatibility {
    /// Candidate is eligible.
    Compatible,
    /// Candidate excluded with all reasons collected.
    Excluded { reasons: Vec<ExclusionReason> },
}

impl Compatibility {
    /// Whether the candidate is compatible.
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible)
    }
}

/// Filter result for one provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderVerdict {
    /// Provider id.
    pub provider: String,
    /// Compatibility verdict.
    pub compatibility: Compatibility,
}

/// Filters registry candidates against a goal, explaining every exclusion.
#[must_use]
pub fn filter_goal(goal: &Goal, registry: &ProviderRegistry) -> Vec<ProviderVerdict> {
    let mut verdicts: Vec<ProviderVerdict> = registry
        .ids()
        .iter()
        .map(|id| {
            let table = registry.get(id).expect("registry id lookup must succeed");
            let isolation = registry
                .isolation_of(id)
                .unwrap_or(ProviderIsolation::Static);
            let provenance = format!("registry:{id}@{}", isolation.name());
            let mut reasons = Vec::new();
            // Goal kind support.
            let kind_served = table
                .capabilities
                .iter()
                .any(|capability| capability.serves_kind(goal));
            if !kind_served {
                reasons.push(ExclusionReason {
                    code: "E-PROV-512",
                    detail: format!(
                        "no capability serves goal kind `{}` / subset `{}`",
                        goal.kind.as_str(),
                        goal.requirements.produce
                    ),
                    provenance: provenance.clone(),
                });
            }
            // Exactness.
            match &goal.requirements.exactness {
                ExactnessPolicy::Exact if !offers_any(table, "exact") => {
                    reasons.push(ExclusionReason {
                        code: "E-PROV-515",
                        detail: "goal requires exact results but provider offers none".into(),
                        provenance: provenance.clone(),
                    });
                }
                ExactnessPolicy::Estimate
                    if !offers_any(table, "estimate") && !offers_any(table, "exact") =>
                {
                    reasons.push(ExclusionReason {
                        code: "E-PROV-515",
                        detail: "goal permits estimates but provider lacks estimate/exact".into(),
                        provenance: provenance.clone(),
                    });
                }
                _ => {}
            }
            // Evidence and checkers.
            if goal.requirements.evidence > highest_evidence(table) {
                reasons.push(ExclusionReason {
                    code: "E-PROV-513",
                    detail: format!(
                        "goal requires {} but provider ceiling is {}",
                        goal.requirements.evidence.as_str(),
                        highest_evidence(table).as_str()
                    ),
                    provenance: provenance.clone(),
                });
            }
            if goal.requirements.evidence >= EvidenceLevel::E3 && !has_checker(table) {
                reasons.push(ExclusionReason {
                    code: "E-PROV-513",
                    detail: "E3 or stronger goal requires a checker binding".into(),
                    provenance: provenance.clone(),
                });
            }
            // Target.
            let family = &goal.requirements.target.family;
            let target_served = table
                .capabilities
                .iter()
                .any(|capability| capability.targets_family(family));
            if !target_served {
                reasons.push(ExclusionReason {
                    code: "E-PROV-514",
                    detail: format!("no capability serves target family `{family}`"),
                    provenance: provenance.clone(),
                });
            }
            // Determinism.
            if goal.requirements.determinism == emath_ir::DeterminismPolicy::Required
                && !table.deterministic()
            {
                reasons.push(ExclusionReason {
                    code: "E-PROV-516",
                    detail: "goal requires determinism but provider is nondeterministic".into(),
                    provenance: provenance.clone(),
                });
            }
            let compatibility = if reasons.is_empty() {
                Compatibility::Compatible
            } else {
                Compatibility::Excluded { reasons }
            };
            ProviderVerdict {
                provider: id.clone(),
                compatibility,
            }
        })
        .collect();
    verdicts.sort_by(|left, right| {
        // Compatible first, then excluded; both by provider id.
        left.compatibility
            .is_compatible()
            .cmp(&right.compatibility.is_compatible())
            .reverse()
            .then_with(|| left.provider.cmp(&right.provider))
    });
    verdicts
}

/// Whether any capability offers an exactness token.
fn offers_any(table: &crate::descriptor::CapabilityTable, token: &str) -> bool {
    table
        .capabilities
        .iter()
        .any(|capability| capability.offers(token))
}

/// Highest evidence level served by the table.
fn highest_evidence(table: &crate::descriptor::CapabilityTable) -> EvidenceLevel {
    table.maximum_evidence
}

/// Whether any capability carries a checker binding.
fn has_checker(table: &crate::descriptor::CapabilityTable) -> bool {
    table
        .capabilities
        .iter()
        .any(|capability| !capability.checker_bindings.is_empty())
}

impl CapabilitySpec {
    /// Whether this capability serves the goal's kind and produce string.
    #[must_use]
    pub fn serves_kind(&self, goal: &Goal) -> bool {
        let kind = goal.kind.as_str();
        self.name == format!("{kind}.{}", goal.requirements.produce)
            || self.name.starts_with(&format!("{kind}."))
    }

    /// Whether this capability serves a target family.
    #[must_use]
    pub fn targets_family(&self, family: &str) -> bool {
        self.semantic_subset.contains(family) || self.semantic_subset == "*"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{CapabilitySpec, CapabilityTable, ProviderLock, RepresentationSpec};
    use crate::registry::{ProviderRegistry, RegistryConfig};
    use emath_core::Span;
    use emath_ir::{DeterminismPolicy, GoalId, GoalKind, GoalRequirements, TargetProfile};

    fn goal_with(exactness: ExactnessPolicy, evidence: EvidenceLevel, family: &str) -> Goal {
        Goal {
            id: GoalId(0),
            kind: GoalKind::Evaluate,
            target: "y".into(),
            expression: None,
            requirements: GoalRequirements {
                evidence,
                exactness,
                determinism: DeterminismPolicy::Required,
                target: TargetProfile {
                    family: family.into(),
                    triple: None,
                    features: vec![],
                },
                fallback: emath_ir::FallbackPolicy::NativeOnly,
                produce: "rust.library".into(),
            },
            source: Span::default(),
        }
    }

    fn table_with(
        name: &str,
        exactness: &[&str],
        max_evidence: EvidenceLevel,
        checkers: bool,
        deterministic: bool,
    ) -> CapabilityTable {
        CapabilityTable {
            capabilities: vec![CapabilitySpec {
                name: format!("evaluate.{name}"),
                semantic_subset: "rust-library".into(),
                representations: vec![RepresentationSpec {
                    name: "f64".into(),
                    exact_relation: "bit-identical".into(),
                    encode_cost: 0,
                }],
                exactness: exactness.iter().map(|token| (*token).to_string()).collect(),
                failure_modes: vec![],
                checker_bindings: if checkers {
                    vec!["sir-checker.v1".into()]
                } else {
                    vec![]
                },
            }],
            isolation: crate::descriptor::ProviderIsolation::Static,
            lock: ProviderLock::Unlocked,
            maximum_evidence: max_evidence,
            deterministic,
        }
    }

    fn registry_with(tables: &[(&str, CapabilityTable)]) -> ProviderRegistry {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        for (id, table) in tables {
            registry
                .register(
                    id,
                    crate::descriptor::ProviderIsolation::Static,
                    table.clone(),
                )
                .unwrap();
        }
        registry
    }

    #[test]
    fn exact_provider_selected_over_approximate() {
        let registry = registry_with(&[
            (
                "approx",
                table_with("approx", &["estimate"], EvidenceLevel::E1, false, true),
            ),
            (
                "exact",
                table_with("exact", &["exact"], EvidenceLevel::E2, true, true),
            ),
        ]);
        let verdicts = filter_goal(
            &goal_with(ExactnessPolicy::Exact, EvidenceLevel::E1, "rust-library"),
            &registry,
        );
        assert_eq!(verdicts[0].provider, "exact");
        assert!(verdicts[0].compatibility.is_compatible());
        assert!(!verdicts[1].compatibility.is_compatible());
    }

    #[test]
    fn approximate_selected_under_declared_tolerance() {
        let registry = registry_with(&[(
            "approx",
            table_with("approx", &["estimate"], EvidenceLevel::E1, false, true),
        )]);
        let verdicts = filter_goal(
            &goal_with(
                ExactnessPolicy::Bounded {
                    tolerance_literal: "1e-6".into(),
                },
                EvidenceLevel::E1,
                "rust-library",
            ),
            &registry,
        );
        assert!(verdicts[0].compatibility.is_compatible());
    }

    #[test]
    fn provider_lacking_checker_excluded_from_e3_goal() {
        let registry = registry_with(&[(
            "nochecker",
            table_with("nochecker", &["exact"], EvidenceLevel::E3, false, true),
        )]);
        let verdicts = filter_goal(
            &goal_with(ExactnessPolicy::Exact, EvidenceLevel::E3, "rust-library"),
            &registry,
        );
        let reasons = match &verdicts[0].compatibility {
            Compatibility::Excluded { reasons } => reasons,
            Compatibility::Compatible => panic!("expected exclusion"),
        };
        assert!(reasons.iter().any(|r| r.code == "E-PROV-513"));
        assert!(reasons[0].provenance.starts_with("registry:nochecker@"));
    }

    #[test]
    fn deterministic_tie_result() {
        let registry = registry_with(&[
            (
                "zeta",
                table_with("zeta", &["exact"], EvidenceLevel::E2, true, true),
            ),
            (
                "alpha",
                table_with("alpha", &["exact"], EvidenceLevel::E2, true, true),
            ),
        ]);
        let goal = goal_with(ExactnessPolicy::Exact, EvidenceLevel::E1, "rust-library");
        let first = filter_goal(&goal, &registry);
        let second = filter_goal(&goal, &registry);
        assert_eq!(first, second);
        assert_eq!(first[0].provider, "alpha");
        assert_eq!(first[1].provider, "zeta");
    }

    #[test]
    fn nondeterministic_provider_excluded_when_determinism_required() {
        let registry = registry_with(&[(
            "wavy",
            table_with("wavy", &["exact"], EvidenceLevel::E2, true, false),
        )]);
        let verdicts = filter_goal(
            &goal_with(ExactnessPolicy::Exact, EvidenceLevel::E1, "rust-library"),
            &registry,
        );
        match &verdicts[0].compatibility {
            Compatibility::Excluded { reasons } => {
                assert!(reasons.iter().any(|r| r.code == "E-PROV-516"));
            }
            Compatibility::Compatible => panic!("expected exclusion"),
        }
    }

    #[test]
    fn all_excluded_yields_no_candidates() {
        let registry = registry_with(&[(
            "wrong-family",
            table_with("wrong-family", &["exact"], EvidenceLevel::E2, true, true),
        )]);
        let verdicts = filter_goal(
            &goal_with(ExactnessPolicy::Exact, EvidenceLevel::E1, "python"),
            &registry,
        );
        assert!(!verdicts.iter().any(|v| v.compatibility.is_compatible()));
        match &verdicts[0].compatibility {
            Compatibility::Excluded { reasons } => {
                assert!(reasons.iter().any(|r| r.code == "E-PROV-514"));
            }
            Compatibility::Compatible => panic!("expected exclusion"),
        }
    }
}
