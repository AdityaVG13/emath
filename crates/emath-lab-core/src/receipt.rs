//!: reproducible decision receipts.
//!
//! A receipt freezes the manifest, gate checks, protocol, raw samples,
//! commands, environment and artifact hashes alongside the decision, so
//! an auditor can recompute the decision from the stored evidence alone.
//! A receipt whose decision does not recompute is refused with
//! `E-HOST-011`.

use crate::error::LabError;
use crate::gate::{GateCheck, QualityGate};
use crate::json::{self, JsonValue};
use crate::promotion::{decide, EnginePolicy, PromotionDecision, PromotionReason};
use crate::stats::{PairedResult, StatisticalProtocol};
use emath_core::{fnv1a64_bytes, ContentId};

/// Full decision receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionReceipt {
    /// Identity over the canonical receipt body (self-referential fields
    /// excluded).
    pub receipt_id: ContentId,
    /// Experiment identity.
    pub experiment_id: ContentId,
    /// Frozen manifest, canonical JSON.
    pub manifest_json: String,
    /// Gate checks exactly as evaluated.
    pub gate_checks: Vec<GateCheck>,
    /// Frozen statistical protocol.
    pub protocol: StatisticalProtocol,
    /// Whether raw samples were retained as declared.
    pub raw_retained: bool,
    /// Paired comparison evidence.
    pub paired: Option<PairedResult>,
    /// Peak-memory ratio evidence.
    pub memory_ratio: Option<f64>,
    /// (joules used, joules budget) when bounded.
    pub energy: Option<(f64, f64)>,
    /// Whether the candidate was promoted at decision time (needed to
    /// reproduce demote-vs-retain).
    pub was_promoted: bool,
    /// The recorded decision.
    pub decision: PromotionDecision,
    /// Exact command used to produce the evidence.
    pub command: String,
    /// Environment token.
    pub environment_token: String,
    /// Artifact hashes `(label, content id)`, sorted by label.
    pub artifact_hashes: Vec<(String, ContentId)>,
}

impl DecisionReceipt {
    /// Canonical receipt body (everything except `receipt_id`), sorted
    /// deterministically; the recompute/identity input.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut hashes = self.artifact_hashes.clone();
        hashes.sort_by(|left, right| left.0.cmp(&right.0));
        let hash_token: Vec<String> = hashes
            .iter()
            .map(|(label, id)| format!("{label}={}", id.0))
            .collect();
        let paired_token = match &self.paired {
            Some(result) => format!(
                "samples={}:median_ratio={}:p99_ratio={}",
                result.samples_used, result.median_ratio, result.p99_ratio
            ),
            None => "-".to_string(),
        };
        format!(
            "receipt:v1:{}:{}:{}:[{}]:{}:{}:{}:{}:{}:{}:{}:{}:{}:[{}]",
            self.experiment_id.0,
            json::write(&json::parse(&self.manifest_json).unwrap_or(JsonValue::Null)),
            self.raw_retained,
            self.gate_checks
                .iter()
                .map(|check| format!(
                    "{}={}({})",
                    check.label,
                    if check.passes { "pass" } else { "fail" },
                    check.code.unwrap_or("-")
                ))
                .collect::<Vec<String>>()
                .join(";"),
            self.protocol.seed,
            paired_token,
            self.memory_ratio
                .map_or_else(|| "-".to_string(), |ratio| ratio.to_string()),
            self.energy.map_or_else(
                || "-".to_string(),
                |(joules, budget)| format!("{joules}/{budget}")
            ),
            self.was_promoted,
            self.decision.outcome.as_str(),
            self.decision.reason.code().unwrap_or("-"),
            self.command,
            self.environment_token,
            hash_token.join(";"),
        )
    }

    /// FNV-1a64 identity of the canonical receipt body.
    #[must_use]
    pub fn identify(&self) -> ContentId {
        ContentId(format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(self.canonical().as_bytes())
        ))
    }

    /// Seals the receipt: computes and stores its identity.
    #[must_use]
    pub fn seal(mut self) -> Self {
        self.receipt_id = self.identify();
        self
    }

    /// Independently recomputes the decision from the stored evidence and
    /// verifies it matches the recorded one (`E-HOST-011` on mismatch).
    pub fn recompute(&self, policy: &EnginePolicy) -> Result<PromotionDecision, LabError> {
        let verdict = QualityGate::evaluate(self.gate_checks.clone());
        let recomputed = decide(
            policy,
            &verdict,
            self.paired.as_ref(),
            self.memory_ratio,
            self.energy,
            self.was_promoted,
        );
        if recomputed == self.decision {
            Ok(recomputed)
        } else {
            Err(LabError::new(
                "E-HOST-011",
                format!(
                    "receipt {} does not recompute: stored {}, recomputed {}",
                    self.receipt_id.0,
                    self.decision.outcome.as_str(),
                    recomputed.outcome.as_str()
                ),
            ))
        }
    }

    /// Deterministic canonical JSON for audit.
    #[must_use]
    pub fn to_json(&self) -> String {
        json::write(&JsonValue::Object(vec![
            (
                "receipt_id".into(),
                JsonValue::String(self.receipt_id.0.clone()),
            ),
            (
                "experiment_id".into(),
                JsonValue::String(self.experiment_id.0.clone()),
            ),
            (
                "manifest".into(),
                json::parse(&self.manifest_json).unwrap_or(JsonValue::Null),
            ),
            (
                "outcome".into(),
                JsonValue::String(self.decision.outcome.as_str().into()),
            ),
            (
                "reason".into(),
                JsonValue::String(self.decision.reason.describe()),
            ),
            ("command".into(), JsonValue::String(self.command.clone())),
            (
                "environment".into(),
                JsonValue::String(self.environment_token.clone()),
            ),
        ]))
    }
}

impl PromotionReason {
    /// Receipts use the reason's `describe()` text; this ensures a stable
    /// audit line exists for every reason.
    #[must_use]
    pub fn audit_line(&self) -> String {
        self.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{GateCheck, GateCheckKind};
    use crate::promotion::{PromotionOutcome, PromotionReason};
    use crate::stats::PairedResult;

    fn receipt() -> DecisionReceipt {
        let checks = vec![
            GateCheck::pass("correctness", GateCheckKind::Correctness),
            GateCheck::pass("evidence", GateCheckKind::Evidence),
        ];
        let paired = PairedResult {
            samples_used: 8,
            outliers_removed: 0,
            median_baseline_ns: 100.0,
            median_candidate_ns: 90.0,
            median_ratio: 0.9,
            p99_ratio: 0.95,
            wins: 6,
            losses: 2,
            ties: 0,
            raw_retained: true,
            paired: true,
            seed: 7,
        };
        let manifest = crate::manifest::LabManifest {
            schema: "lab:v1".into(),
            experiment_id: ContentId("exp-01".into()),
            baseline: crate::manifest::ArtifactRef {
                package: "score".into(),
                content_id: ContentId("base".into()),
                profile: "release".into(),
            },
            candidate: crate::manifest::ArtifactRef {
                package: "score".into(),
                content_id: ContentId("cand".into()),
                profile: "release".into(),
            },
            partitions: vec![crate::manifest::CorpusPartition {
                name: "ops".into(),
                kind: crate::manifest::PartitionKind::Holdout,
                operations: 1000,
                fingerprint: ContentId("fp".into()),
            }],
            metrics: vec![crate::manifest::MetricSpec {
                id: "latency".into(),
                kind: "latency".into(),
                unit: "ns".into(),
                direction: crate::manifest::MetricDirection::LowerIsBetter,
                weight: 1.0,
            }],
            thresholds: crate::manifest::Thresholds::default(),
            kill_rules: vec![],
            fallback: crate::manifest::FallbackPlan {
                on_gate_failure: crate::manifest::FallbackAction::RetainBaseline,
                on_regression: crate::manifest::FallbackAction::ShadowCandidate,
                on_measurement_failure: crate::manifest::FallbackAction::QuarantineCandidate,
            },
            environment: crate::manifest::EnvironmentPin {
                toolchain: "1.74.0".into(),
                target_triple: "aarch64-apple-darwin".into(),
                features: vec![],
                host: "macos-arm64".into(),
            },
            frozen: true,
        };
        let verdict = QualityGate::evaluate(checks.clone());
        let decision = decide(
            &EnginePolicy::default(),
            &verdict,
            Some(&paired),
            None,
            None,
            false,
        );
        DecisionReceipt {
            receipt_id: ContentId("pending".into()),
            experiment_id: ContentId("exp-01".into()),
            manifest_json: manifest.to_json(),
            gate_checks: checks,
            protocol: StatisticalProtocol {
                warmup_repetitions: 2,
                repetitions: 10,
                min_repetitions: 4,
                paired: true,
                seed: 7,
                outlier: crate::stats::OutlierPolicy::KeepAll,
                retain_raw: true,
                randomize_order: false,
            },
            raw_retained: true,
            paired: Some(paired),
            memory_ratio: None,
            energy: None,
            was_promoted: false,
            decision,
            command: "pilot --serve".into(),
            environment_token: "macos-arm64".into(),
            artifact_hashes: vec![
                ("baseline".into(), ContentId("base".into())),
                ("candidate".into(), ContentId("cand".into())),
            ],
        }
        .seal()
    }

    #[test]
    fn receipt_seals_with_stable_identity() {
        let first = receipt();
        let second = receipt();
        assert_eq!(first.receipt_id, second.receipt_id);
        assert_ne!(first.receipt_id, ContentId("pending".into()));
    }

    #[test]
    fn receipt_recomputes_the_same_decision() {
        let receipt = receipt();
        let recomputed = receipt.recompute(&EnginePolicy::default()).unwrap();
        assert_eq!(recomputed.outcome, PromotionOutcome::Promote);
        assert!(receipt.to_json().contains("promote"));
    }

    #[test]
    fn tampered_receipt_is_refused_with_e_host_011() {
        let mut receipt = receipt();
        receipt.paired = Some(PairedResult {
            median_ratio: 2.0,
            p99_ratio: 2.5,
            ..receipt.paired.clone().expect("paired present")
        });
        let error = receipt.recompute(&EnginePolicy::default()).unwrap_err();
        assert_eq!(error.code, "E-HOST-011");
        assert!(error.message.contains("does not recompute"));
    }

    #[test]
    fn canonical_excludes_receipt_id_and_is_sensitive() {
        let first = receipt();
        let mut perturbed = receipt();
        assert_eq!(first.identify(), first.receipt_id);
        perturbed.command = "pilot --serve --debug".into();
        assert_ne!(first.identify(), perturbed.identify());
        // Reason lines are stable audit text.
        assert_eq!(
            receipt().decision.reason.audit_line(),
            PromotionReason::MeetsTarget { median_ratio: 0.9 }.describe()
        );
    }
}
