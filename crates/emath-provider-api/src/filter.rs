//! Compatibility filter.
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
        // A capability serves a goal only when its produce matches exactly;
        // a bare `kind` capability spans every produce of the kind. The
        // prefix fallback made `evaluate.*` serve every evaluate goal.
        self.name == format!("{kind}.{}", goal.requirements.produce) || self.name == kind
    }

    /// Whether this capability serves a target family.
    #[must_use]
    pub fn targets_family(&self, family: &str) -> bool {
        self.semantic_subset.contains(family) || self.semantic_subset == "*"
    }
}
