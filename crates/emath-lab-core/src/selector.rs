//! Runtime selector, drift receipts and the candidate loop (P10-2).
//!
//! Deterministic and std-only. A [`RuntimeSelector`] decides whether a
//! candidate implementation may *promote* over the baseline, must *fall
//! back* to the baseline, or stays *pending* verification, using the
//! numeric/performance gates of [`PromotionPolicy`]. Every decision is
//! recorded in a [`DriftReceipt`] whose canonical JSON and FNV-1a64
//! content id make the decision auditable; observations are sorted by
//! metric id before any comparison so decisions are order-independent.
//! The bounded [`candidate_loop`] folds candidates until promotion,
//! fallback, or the iteration budget, and always emits a receipt.

use emath_core::{content_id_of_str, ContentId};

use crate::{Observation, PromotionPolicy};

/// Schema of a drift receipt document.
pub const DRIFT_RECEIPT_SCHEMA: &str = "emath.drift-receipt.v1";
/// Bounded iteration cap for the candidate loop.
pub const CANDIDATE_LOOP_BUDGET: u64 = 64;

/// The disposition decided for a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionDecision {
    /// Candidate passed every gate: promote.
    Promote,
    /// Candidate failed a numeric gate: fall back to the baseline.
    FallbackBaseline,
    /// Numerics pass but performance gates are not met: keep baseline,
    /// defer the decision.
    PendingVerification,
}

impl SelectionDecision {
    /// Stable string form for receipts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::FallbackBaseline => "fallback-baseline",
            Self::PendingVerification => "pending-verification",
        }
    }
}

/// One decision plus its justification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeSelection {
    /// Candidate identity under decision.
    pub candidate: ContentId,
    /// Decision.
    pub decision: SelectionDecision,
    /// Stable reason string (gate name; empty on promote).
    pub reason: String,
}

/// Auditable record of a selection decision.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftReceipt {
    /// Schema id (`emath.drift-receipt.v1`).
    pub schema: String,
    /// Experiment identity the decision belongs to.
    pub experiment_id: ContentId,
    /// Baseline identity.
    pub baseline: ContentId,
    /// Candidate identity.
    pub candidate: ContentId,
    /// Decision.
    pub decision: SelectionDecision,
    /// Gate that failed, when not promoting.
    pub failure_reason: Option<String>,
    /// Observations the decision used (deterministically sorted).
    pub observations: Vec<Observation>,
    /// Iterations spent in the candidate loop before a terminal decision.
    pub candidate_loop_iterations: u64,
}

impl DriftReceipt {
    /// Renders the deterministic canonical JSON. Observations are sorted
    /// by metric id (`crate::sort_observations`) before rendering.
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut observations = self.observations.clone();
        crate::sort_observations(&mut observations);
        let mut out = String::from(r#"{"schema":"emath.drift-receipt.v1","experiment_id":"#);
        json_string(&self.experiment_id.0, &mut out);
        out.push_str(r#","baseline":"#);
        json_string(&self.baseline.0, &mut out);
        out.push_str(r#","candidate":"#);
        json_string(&self.candidate.0, &mut out);
        out.push_str(r#","decision":"#);
        json_string(self.decision.as_str(), &mut out);
        out.push_str(r#","failure_reason":"#);
        match &self.failure_reason {
            Some(reason) => json_string(reason, &mut out),
            None => out.push_str("null"),
        }
        out.push_str(r#","observations":["#);
        for observation in &observations {
            out.push('{');
            out.push_str(r#""metric_id":"#);
            json_string(&observation.metric.id, &mut out);
            out.push_str(r#","metric_kind":"#);
            json_string(observation.metric.kind.as_str(), &mut out);
            out.push_str(r#","max_absolute_error":"#);
            json_string(&float_hex(observation.max_absolute_error), &mut out);
            out.push_str(r#","max_relative_error":"#);
            json_string(&float_hex(observation.max_relative_error), &mut out);
            out.push_str(r#","median_ratio":"#);
            json_string(&float_hex(observation.median_ratio), &mut out);
            out.push_str(r#","p99_ratio":"#);
            json_string(&float_hex(observation.p99_ratio), &mut out);
            out.push('}');
            out.push(',');
        }
        if !observations.is_empty() {
            out.pop();
        }
        out.push_str(r#"],"candidate_loop_iterations":"#);
        out.push_str(&self.candidate_loop_iterations.to_string());
        out.push('}');
        out
    }

    /// FNV-1a64 content id of the canonical JSON.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        content_id_of_str(&self.canonical_json())
    }
}

/// Renders an f64 deterministically as its IEEE-754 bit pattern in hex.
fn float_hex(value: f64) -> String {
    format!("0x{:016x}", value.to_bits())
}

/// Normalizes the content id display used by the workspace (kept short in
/// receipts; the full id is the canonical string).
fn json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Decides the disposition of a candidate against the baseline gates.
#[derive(Debug, Clone)]
pub struct RuntimeSelector {
    /// Promotion policy whose numeric/performance gates are enforced.
    pub policy: PromotionPolicy,
}

impl RuntimeSelector {
    /// Creates a selector with the given policy.
    #[must_use]
    pub const fn new(policy: PromotionPolicy) -> Self {
        Self { policy }
    }

    /// Selects a disposition for `candidate` given `observations`.
    ///
    /// Order of checks (deterministic):
    /// 1. any observation failing the numeric gates → [`SelectionDecision::FallbackBaseline`];
    /// 2. any observation failing the performance gates → [`SelectionDecision::PendingVerification`];
    /// 3. otherwise → [`SelectionDecision::Promote`].
    #[must_use]
    pub fn select(&self, candidate: &ContentId, observations: &[Observation]) -> RuntimeSelection {
        let mut sorted = observations.to_vec();
        crate::sort_observations(&mut sorted);
        for observation in &sorted {
            if !self.policy.passes_numeric_gates(observation) {
                return RuntimeSelection {
                    candidate: candidate.clone(),
                    decision: SelectionDecision::FallbackBaseline,
                    reason: format!("numeric gate failed on metric `{}`", observation.metric.id),
                };
            }
        }
        for observation in &sorted {
            if !self.policy.passes_performance_gates(observation) {
                return RuntimeSelection {
                    candidate: candidate.clone(),
                    decision: SelectionDecision::PendingVerification,
                    reason: format!("performance gate failed on metric `{}`", observation.metric.id),
                };
            }
        }
        RuntimeSelection {
            candidate: candidate.clone(),
            decision: SelectionDecision::Promote,
            reason: String::new(),
        }
    }

    /// Records a drift receipt for a decision.
    #[must_use]
    pub fn receipt(
        &self,
        experiment_id: &ContentId,
        baseline: &ContentId,
        selection: &RuntimeSelection,
        observations: &[Observation],
        candidate_loop_iterations: u64,
    ) -> DriftReceipt {
        DriftReceipt {
            schema: DRIFT_RECEIPT_SCHEMA.into(),
            experiment_id: experiment_id.clone(),
            baseline: baseline.clone(),
            candidate: selection.candidate.clone(),
            decision: selection.decision,
            failure_reason: (!selection.reason.is_empty()).then_some(selection.reason.clone()),
            observations: observations.to_vec(),
            candidate_loop_iterations,
        }
    }
}

/// Outcome of a bounded candidate loop.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopOutcome {
    /// Terminal decision.
    pub selection: RuntimeSelection,
    /// Receipt for the terminal decision.
    pub receipt: DriftReceipt,
}

/// Runs the bounded candidate loop: starting from the baseline, evaluate
/// candidates until the selector reaches a terminal decision (promote or
/// fallback) or the iteration budget is exhausted (pending).
///
/// `evaluate(candidate)` returns the observations for that candidate. The
/// loop is deterministic for deterministic evaluations.
#[must_use]
pub fn candidate_loop(
    selector: &RuntimeSelector,
    experiment_id: &ContentId,
    baseline: &ContentId,
    mut evaluate: impl FnMut(&ContentId) -> Vec<Observation>,
) -> LoopOutcome {
    let mut candidate = baseline.clone();
    let mut iterations = 0u64;
    loop {
        iterations += 1;
        let observations = evaluate(&candidate);
        let selection = selector.select(&candidate, &observations);
        if selection.decision == SelectionDecision::Promote {
            let receipt =
                selector.receipt(experiment_id, baseline, &selection, &observations, iterations);
            return LoopOutcome { selection, receipt };
        }
        if selection.decision == SelectionDecision::FallbackBaseline {
            let receipt =
                selector.receipt(experiment_id, baseline, &selection, &observations, iterations);
            return LoopOutcome { selection, receipt };
        }
        if iterations >= CANDIDATE_LOOP_BUDGET {
            let receipt =
                selector.receipt(experiment_id, baseline, &selection, &observations, iterations);
            return LoopOutcome { selection, receipt };
        }
        candidate = ContentId(format!("candidate-{iterations}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetricDefinition, MetricKind, Observation};

    fn observation(id: &str, abs_error: f64, median_ratio: f64, p99: f64) -> Observation {
        Observation {
            metric: MetricDefinition {
                id: id.into(),
                kind: MetricKind::Numeric,
                label: id.into(),
            },
            samples: 100,
            max_absolute_error: abs_error,
            max_relative_error: 0.0,
            median_ratio,
            p99_ratio: p99,
            operations_candidate: 3,
            operations_baseline: 4,
            evidence: None,
        }
    }

    fn good() -> Observation {
        observation("score", 1e-12, 1.2, 1.01)
    }

    #[test]
    fn selector_promotes_only_gate_clean_candidates() {
        let selector = RuntimeSelector::new(PromotionPolicy::default());
        let candidate = ContentId("candidate-1".into());

        let promoted = selector.select(&candidate, &[good()]);
        assert_eq!(promoted.decision, SelectionDecision::Promote);

        let drifting = selector.select(&candidate, &[observation("score", 1e-3, 1.2, 1.01)]);
        assert_eq!(drifting.decision, SelectionDecision::FallbackBaseline);
        assert!(drifting.reason.contains("numeric"));

        let slow = selector.select(&candidate, &[observation("score", 1e-12, 0.9, 1.01)]);
        assert_eq!(slow.decision, SelectionDecision::PendingVerification);
        assert!(slow.reason.contains("performance"));
    }

    #[test]
    fn adversarial_observation_never_promotes() {
        // Negative control: a gate-violating observation must never yield
        // Promote, regardless of the other metrics.
        let selector = RuntimeSelector::new(PromotionPolicy::default());
        let candidate = ContentId("candidate-adversarial".into());
        let observations = vec![good(), observation("score", 9e-1, 1.5, 1.0)];
        let selection = selector.select(&candidate, &observations);
        assert_ne!(selection.decision, SelectionDecision::Promote);
        assert_eq!(selection.decision, SelectionDecision::FallbackBaseline);
    }

    #[test]
    fn receipt_is_deterministic_and_auditable() {
        let selector = RuntimeSelector::new(PromotionPolicy::default());
        let experiment = ContentId("experiment-1".into());
        let baseline = ContentId("baseline-1".into());
        let candidate = ContentId("candidate-1".into());
        let observations = vec![good(), observation("latency", 1e-12, 1.1, 1.02)];
        let selection = selector.select(&candidate, &observations);
        let first = selector.receipt(&experiment, &baseline, &selection, &observations, 3);
        let second = selector.receipt(&experiment, &baseline, &selection, &observations, 3);
        assert_eq!(first.canonical_json(), second.canonical_json());
        assert_eq!(first.content_id(), second.content_id());
        assert!(first.canonical_json().contains(DRIFT_RECEIPT_SCHEMA));
        assert!(first.canonical_json().contains("\"decision\":\"promote\""));
        // Reversed input order must not change the receipt.
        let reversed = vec![observation("latency", 1e-12, 1.1, 1.02), good()];
        let selection_reversed = selector.select(&candidate, &reversed);
        let third = selector.receipt(&experiment, &baseline, &selection_reversed, &reversed, 3);
        assert_eq!(first.canonical_json(), third.canonical_json());
    }

    #[test]
    fn candidate_loop_respects_budget_and_terminates() {
        let selector = RuntimeSelector::new(PromotionPolicy::default());
        let experiment = ContentId("experiment-2".into());
        let baseline = ContentId("baseline-2".into());
        // Evaluator: first candidate fails only the performance gate
        // (pending -> loop continues), second passes -> promote after 2.
        let mut call = 0u64;
        let outcome = candidate_loop(&selector, &experiment, &baseline, |candidate| {
            call += 1;
            if call <= 1 {
                vec![observation("score", 1e-12, 0.5, 2.0)]
            } else {
                let _ = candidate;
                vec![good()]
            }
        });
        assert_eq!(outcome.selection.decision, SelectionDecision::Promote);
        assert_eq!(outcome.receipt.candidate_loop_iterations, 2);
    }

    #[test]
    fn always_failing_candidates_end_in_fallback_not_promotion() {
        let selector = RuntimeSelector::new(PromotionPolicy::default());
        let experiment = ContentId("experiment-3".into());
        let baseline = ContentId("baseline-3".into());
        let outcome = candidate_loop(&selector, &experiment, &baseline, |_| {
            vec![observation("score", 5e-1, 0.4, 3.0)]
        });
        assert_eq!(outcome.selection.decision, SelectionDecision::FallbackBaseline);
        assert_eq!(outcome.receipt.candidate_loop_iterations, 1);
    }
}
