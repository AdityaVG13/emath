//! Host campaigns: promotion of candidate meanings under protected metrics.
//!
//! One campaign selects or rejects a candidate meaning (per the
//! tuning exit criterion). Promotion requires, in order:
//!
//! 1. semantic admission — the candidate passed the held-out challenge and
//!    does not touch preserved (declared-law) symbols;
//! 2. evidence threshold — enough evidence units;
//! 3. resource envelope — measured host metrics are within bounds;
//! 4. protected host metrics — objectives score the candidate against
//!    cache hit rate (maximize) and token cost / p95 latency (minimize);
//! 5. fallback availability — a strict baseline world exists to deopt to;
//! 6. deterministic receipt — the outcome carries a content identity over
//!    its canonical form.

use crate::JointCandidate;
use emath_term::SymbolId;
use emath_world_ir::{fnv1a64, WorldId};

/// A named host metric measurement (integer-valued, e.g. tokens, ms, permille).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostMetric {
    /// Metric name; campaign objectives and envelope match on this.
    pub name: String,
    /// Measured value.
    pub value: u64,
}

/// Protected bounds a candidate must stay within to be promoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEnvelope {
    /// Maximum `token_cost` (tokens).
    pub max_tokens: u64,
    /// Maximum `p95_latency` (milliseconds).
    pub max_p95_latency_ms: u64,
    /// Minimum `cache_hit_rate` (permille, 0..=1000).
    pub min_cache_hit_rate_permille: u64,
}

impl ResourceEnvelope {
    /// Fixed-point fraction of `bound` consumed by `value`, in permille.
    #[must_use]
    fn permille_of(value: u64, bound: u64) -> u64 {
        if bound == 0 {
            return if value == 0 { 0 } else { 1000 };
        }
        (value.saturating_mul(1000) / bound).min(1000)
    }

    /// Whether the measured metrics all stay within the envelope.
    #[must_use]
    pub fn admits(&self, metrics: &[HostMetric]) -> bool {
        let hit_ok = metrics
            .iter()
            .find(|metric| metric.name == "cache_hit_rate")
            .is_some_and(|metric| metric.value >= self.min_cache_hit_rate_permille);
        let tokens_ok = metrics
            .iter()
            .find(|metric| metric.name == "token_cost")
            .map_or(true, |metric| metric.value <= self.max_tokens);
        let latency_ok = metrics
            .iter()
            .find(|metric| metric.name == "p95_latency")
            .map_or(true, |metric| metric.value <= self.max_p95_latency_ms);
        hit_ok && tokens_ok && latency_ok
    }
}

/// Which named metrics the host optimizes and in which direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostObjectives {
    /// Metric names whose value is maximized.
    pub maximize: Vec<String>,
    /// Metric names whose value is minimized.
    pub minimize: Vec<String>,
}

/// Fixed-point protected-metric reward over a campaign's objectives.
/// Every contribution is oriented so that higher is better: maximize
/// metrics add their permille value (cache hit rate is already permille);
/// minimize metrics add the permille of budget left unused (cost against
/// `max_tokens`, latency against `max_p95_latency_ms`).
#[must_use]
fn protected_score(
    objectives: &HostObjectives,
    envelope: &ResourceEnvelope,
    metrics: &[HostMetric],
) -> u64 {
    let mut score = 0;
    for name in &objectives.maximize {
        let Some(metric) = metrics.iter().find(|metric| &metric.name == name) else {
            continue;
        };
        if name == "cache_hit_rate" {
            score += metric.value.min(1000);
        }
    }
    for name in &objectives.minimize {
        let Some(metric) = metrics.iter().find(|metric| &metric.name == name) else {
            continue;
        };
        let budget = match name.as_str() {
            "token_cost" => envelope.max_tokens,
            "p95_latency" => envelope.max_p95_latency_ms,
            _ => 0,
        };
        score += 1000_u64.saturating_sub(ResourceEnvelope::permille_of(metric.value, budget));
    }
    score
}

/// Measurements for one candidate, keyed by the candidate's content identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateMeasurement {
    /// `JointCandidate::identity`.
    pub candidate_identity: u64,
    /// Measured host metrics.
    pub metrics: Vec<HostMetric>,
}

/// Per-candidate promotion verdict (checklist in spec order).
///
/// Bools are the point of a checklist; the count is intentional.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionChecklist {
    /// Held-out challenge passed and no preserved symbol touched.
    pub semantic_admission: bool,
    /// Evidence units at or above the campaign threshold.
    pub evidence_threshold: bool,
    /// Measured metrics within the protected envelope.
    pub resource_envelope: bool,
    /// Metrics present for every objective (nothing unmeasurable).
    pub protected_metrics_ok: bool,
    /// A strict fallback world is available.
    pub fallback_availability: bool,
}

impl PromotionChecklist {
    /// Whether every promotion condition holds.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.semantic_admission
            && self.evidence_threshold
            && self.resource_envelope
            && self.protected_metrics_ok
            && self.fallback_availability
    }
}

/// One candidate's campaign decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateDecision {
    /// The candidate's content identity.
    pub candidate_identity: u64,
    /// The promotion checklist.
    pub checklist: PromotionChecklist,
    /// Protected-score permille, only meaningful when promoted.
    pub score_permille: u64,
    /// Whether the candidate was promoted.
    pub promoted: bool,
    /// Machine-readable rejection reason when not promoted.
    pub reason: String,
}

impl CandidateDecision {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "decision:{}:promoted={}:score={}:admitted={}:evidence={}:envelope={}:metrics={}:fallback={}",
            self.candidate_identity,
            self.promoted,
            self.score_permille,
            self.checklist.semantic_admission,
            self.checklist.evidence_threshold,
            self.checklist.resource_envelope,
            self.checklist.protected_metrics_ok,
            self.checklist.fallback_availability,
        )
    }
}

/// Deterministic receipt of a campaign outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignReceipt {
    /// Campaign label.
    pub campaign_label: String,
    /// Identity of the selected candidate; `None` when the campaign
    /// rejected every candidate.
    pub selected_identity: Option<u64>,
    /// Machine-readable outcome.
    pub reason: String,
    /// Per-candidate decisions, sorted by candidate identity.
    pub decisions: Vec<CandidateDecision>,
    /// FNV-1a64 content identity over the canonical receipt form.
    pub identity: u64,
}

impl CampaignReceipt {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let decided = self
            .decisions
            .iter()
            .map(CandidateDecision::canonical)
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "receipt:{}:selected={}:{}:{}",
            self.campaign_label,
            self.selected_identity
                .map_or_else(String::new, |id| id.to_string()),
            self.reason,
            decided
        )
    }
}

/// A host campaign that selects or rejects candidate meanings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCampaign {
    /// Campaign label.
    pub label: String,
    /// Symbols whose meaning must not vary (declared laws and examples).
    pub preserved_symbols: Vec<SymbolId>,
    /// Minimum evidence units for promotion.
    pub evidence_threshold: u32,
    /// Protected resource envelope.
    pub envelope: ResourceEnvelope,
    /// Protected-metric objectives.
    pub objectives: HostObjectives,
    /// Strict baseline world to deopt to; promotion requires it.
    pub fallback_world: Option<WorldId>,
}

impl HostCampaign {
    /// Runs the campaign over candidate meanings and their measurements.
    ///
    /// Selection is deterministic: among promoted candidates the highest
    /// protected score wins, ties break toward the lower identity. When no
    /// candidate promotes, the campaign rejects (selects the fallback) and
    /// the receipt records that.
    #[must_use]
    pub fn run(
        &self,
        candidates: &[JointCandidate],
        measurements: &[CandidateMeasurement],
    ) -> CampaignReceipt {
        let mut decisions = candidates
            .iter()
            .map(|candidate| self.decide(candidate, measurements))
            .collect::<Vec<_>>();
        decisions.sort_by_key(|decision| decision.candidate_identity);

        let selected = decisions
            .iter()
            .filter(|decision| decision.promoted)
            .max_by(|left, right| {
                left.score_permille
                    .cmp(&right.score_permille)
                    .then_with(|| right.candidate_identity.cmp(&left.candidate_identity))
            })
            .map(|decision| decision.candidate_identity);

        let reason = match selected {
            Some(_) => "selected".to_string(),
            None => "rejected:no-candidate-passed-promotion".to_string(),
        };

        let receipt = CampaignReceipt {
            campaign_label: self.label.clone(),
            selected_identity: selected,
            reason,
            decisions,
            identity: 0,
        };
        let identity = fnv1a64(receipt.canonical().as_bytes());
        CampaignReceipt {
            identity,
            ..receipt
        }
    }

    fn decide(
        &self,
        candidate: &JointCandidate,
        measurements: &[CandidateMeasurement],
    ) -> CandidateDecision {
        let metrics = measurements
            .iter()
            .find(|measurement| measurement.candidate_identity == candidate.identity)
            .map_or_else(Vec::new, |measurement| measurement.metrics.clone());

        let touches_preserved = candidate.world.changes.iter().any(|change| {
            change
                .symbol
                .as_ref()
                .is_some_and(|symbol| self.preserved_symbols.contains(symbol))
        });
        let semantic_admission = candidate.held_out_verified && !touches_preserved;
        let evidence_threshold = candidate.evidence_units >= self.evidence_threshold;
        let resource_envelope = self.envelope.admits(&metrics);
        let covered = |names: &[String]| {
            names
                .iter()
                .all(|name| metrics.iter().any(|metric| metric.name == *name))
        };
        let protected_metrics_ok =
            covered(&self.objectives.maximize) && covered(&self.objectives.minimize);
        let fallback_availability = self.fallback_world.is_some();

        let checklist = PromotionChecklist {
            semantic_admission,
            evidence_threshold,
            resource_envelope,
            protected_metrics_ok,
            fallback_availability,
        };
        let promoted = checklist.all_passed();
        let reason = rejection_reason(&checklist, candidate, self.evidence_threshold);
        let score_permille = if promoted {
            protected_score(&self.objectives, &self.envelope, &metrics)
        } else {
            0
        };

        CandidateDecision {
            candidate_identity: candidate.identity,
            checklist,
            score_permille,
            promoted,
            reason,
        }
    }
}

/// Machine-readable rejection reason, in checklist order.
#[must_use]
fn rejection_reason(
    checklist: &PromotionChecklist,
    candidate: &JointCandidate,
    threshold: u32,
) -> String {
    if checklist.all_passed() {
        return "promoted".to_string();
    }
    let mut reasons = Vec::new();
    if !checklist.semantic_admission {
        reasons.push(if candidate.held_out_verified {
            "semantic-admission:preserved-symbol-touched".to_string()
        } else {
            "semantic-admission:held-out-failed".to_string()
        });
    }
    if !checklist.evidence_threshold {
        reasons.push(format!(
            "evidence:{}<{}",
            candidate.evidence_units, threshold
        ));
    }
    if !checklist.resource_envelope {
        reasons.push("envelope:out-of-bounds".to_string());
    }
    if !checklist.protected_metrics_ok {
        reasons.push("metrics:unmeasurable".to_string());
    }
    if !checklist.fallback_availability {
        reasons.push("fallback:unavailable".to_string());
    }
    reasons.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecutionDelta, SemanticChange, SemanticVariableKind, WorldDelta};

    fn symbol(name: &str) -> SymbolId {
        SymbolId(name.to_string())
    }

    fn candidate(
        label: &str,
        held_out: bool,
        evidence: u32,
        changed_symbol: Option<SymbolId>,
    ) -> JointCandidate {
        let change = SemanticChange {
            kind: SemanticVariableKind::Operator,
            symbol: changed_symbol,
            description: "xor over Bool".to_string(),
            provenance: "synthesized".to_string(),
        };
        JointCandidate::new(
            label,
            WorldDelta::new(WorldId(7), vec![change]),
            ExecutionDelta {
                lowering: "algebraic".to_string(),
                precision: "f64".to_string(),
                provider: "native".to_string(),
                target: "cpu".to_string(),
                schedule: "direct".to_string(),
            },
            held_out,
            evidence,
        )
    }

    fn campaign() -> HostCampaign {
        HostCampaign {
            label: "host-1".to_string(),
            preserved_symbols: vec![symbol("+")],
            evidence_threshold: 3,
            envelope: ResourceEnvelope {
                max_tokens: 1000,
                max_p95_latency_ms: 100,
                min_cache_hit_rate_permille: 900,
            },
            objectives: HostObjectives {
                maximize: vec!["cache_hit_rate".to_string()],
                minimize: vec!["token_cost".to_string(), "p95_latency".to_string()],
            },
            fallback_world: Some(WorldId(0)),
        }
    }

    fn measurements(
        candidate: &JointCandidate,
        tokens: u64,
        hit: u64,
        latency_ms: u64,
    ) -> CandidateMeasurement {
        CandidateMeasurement {
            candidate_identity: candidate.identity,
            metrics: vec![
                HostMetric {
                    name: "token_cost".to_string(),
                    value: tokens,
                },
                HostMetric {
                    name: "cache_hit_rate".to_string(),
                    value: hit,
                },
                HostMetric {
                    name: "p95_latency".to_string(),
                    value: latency_ms,
                },
            ],
        }
    }

    #[test]
    fn general_selected_memorizer_rejected() {
        let memorizer = candidate("memorizer", false, 10, Some(symbol("⋈")));
        let general = candidate("general", true, 10, Some(symbol("⋈")));
        let campaign = campaign();
        let outcome = campaign.run(
            &[memorizer.clone(), general.clone()],
            &[
                measurements(&memorizer, 100, 920, 30),
                measurements(&general, 200, 950, 40),
            ],
        );
        assert_eq!(outcome.selected_identity, Some(general.identity));
        assert_eq!(outcome.reason, "selected");
        let decision = outcome
            .decisions
            .iter()
            .find(|d| d.candidate_identity == memorizer.identity)
            .expect("memorizer decision present");
        assert!(!decision.promoted);
        assert_eq!(decision.reason, "semantic-admission:held-out-failed");
    }

    #[test]
    fn envelope_rejects_hot_candidate() {
        let general = candidate("general", true, 10, Some(symbol("⋈")));
        let campaign = campaign();
        let outcome = campaign.run(
            std::slice::from_ref(&general),
            &[measurements(&general, 5_000, 990, 200)],
        );
        assert_eq!(outcome.selected_identity, None);
        assert_eq!(outcome.reason, "rejected:no-candidate-passed-promotion");
        let decision = &outcome.decisions[0];
        assert!(!decision.promoted);
        assert_eq!(decision.reason, "envelope:out-of-bounds");
    }

    #[test]
    fn preserved_symbol_touch_rejects() {
        let violator = candidate("violator", true, 10, Some(symbol("+")));
        let campaign = campaign();
        let outcome = campaign.run(
            std::slice::from_ref(&violator),
            &[measurements(&violator, 100, 950, 30)],
        );
        assert_eq!(outcome.selected_identity, None);
        assert_eq!(outcome.reason, "rejected:no-candidate-passed-promotion");
        let decision = &outcome.decisions[0];
        assert_eq!(
            decision.reason,
            "semantic-admission:preserved-symbol-touched"
        );
    }

    #[test]
    fn fallback_required_for_promotion() {
        let general = candidate("general", true, 10, Some(symbol("⋈")));
        let mut campaign = campaign();
        campaign.fallback_world = None;
        let outcome = campaign.run(
            std::slice::from_ref(&general),
            &[measurements(&general, 100, 950, 30)],
        );
        assert_eq!(outcome.selected_identity, None);
        let decision = &outcome.decisions[0];
        assert!(!decision.promoted);
        assert_eq!(decision.reason, "fallback:unavailable");
    }

    #[test]
    fn evidence_threshold_blocks_weak_candidate() {
        let weak = candidate("weak", true, 1, Some(symbol("⋈")));
        let campaign = campaign();
        let outcome = campaign.run(
            std::slice::from_ref(&weak),
            &[measurements(&weak, 100, 950, 30)],
        );
        assert_eq!(outcome.selected_identity, None);
        let decision = &outcome.decisions[0];
        assert!(!decision.promoted);
        assert!(decision.reason.starts_with("evidence:"));
    }

    #[test]
    fn best_protected_score_selected() {
        let slow = candidate("slow", true, 10, Some(symbol("⋈")));
        let fast = candidate("fast", true, 10, Some(symbol("⋈")));
        let campaign = campaign();
        let outcome = campaign.run(
            &[slow.clone(), fast.clone()],
            &[
                measurements(&slow, 300, 930, 90),
                measurements(&fast, 120, 970, 25),
            ],
        );
        assert_eq!(outcome.selected_identity, Some(fast.identity));
        let fast_decision = outcome
            .decisions
            .iter()
            .find(|d| d.candidate_identity == fast.identity)
            .expect("fast decision present");
        let slow_decision = outcome
            .decisions
            .iter()
            .find(|d| d.candidate_identity == slow.identity)
            .expect("slow decision present");
        assert!(fast_decision.score_permille > slow_decision.score_permille);
    }

    #[test]
    fn unmet_metrics_block_promotion() {
        let general = candidate("general", true, 10, Some(symbol("⋈")));
        let campaign = campaign();
        let outcome = campaign.run(
            std::slice::from_ref(&general),
            &[CandidateMeasurement {
                candidate_identity: general.identity,
                metrics: vec![HostMetric {
                    name: "token_cost".to_string(),
                    value: 100,
                }],
            }],
        );
        assert_eq!(outcome.selected_identity, None);
        let decision = &outcome.decisions[0];
        assert!(!decision.promoted);
        assert_eq!(
            decision.reason,
            "envelope:out-of-bounds,metrics:unmeasurable"
        );
    }

    #[test]
    fn receipt_identity_is_deterministic() {
        let general = candidate("general", true, 10, Some(symbol("⋈")));
        let campaign = campaign();
        let first = campaign.run(
            std::slice::from_ref(&general),
            &[measurements(&general, 200, 950, 40)],
        );
        let second = campaign.run(
            std::slice::from_ref(&general),
            &[measurements(&general, 200, 950, 40)],
        );
        assert_eq!(first.identity, second.identity);
        assert_eq!(first, second);
    }
}
