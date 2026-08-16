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
    /// deterministically; the recompute/identity input. A manifest that
    /// does not parse is refused (`E-HOST-011`), never hashed as `null`
    /// (a truncated receipt must not seal as valid).
    pub fn canonical(&self) -> Result<String, LabError> {
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
        let manifest = json::parse(&self.manifest_json).map_err(|error| {
            LabError::new(
                "E-HOST-011",
                format!("receipt manifest does not parse: {}", error.message),
            )
        })?;
        Ok(format!(
            "receipt:v1:{}:{}:{}:[{}]:{}:{}:{}:{}:{}:{}:{}:{}:{}:[{}]",
            self.experiment_id.0,
            json::write(&manifest),
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
        ))
    }

    /// FNV-1a64 identity of the canonical receipt body.
    pub fn identify(&self) -> Result<ContentId, LabError> {
        Ok(ContentId(format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(self.canonical()?.as_bytes())
        )))
    }

    /// Seals the receipt: computes and stores its identity.
    pub fn seal(mut self) -> Result<Self, LabError> {
        self.receipt_id = self.identify()?;
        Ok(self)
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

    /// Deterministic canonical JSON for audit; a manifest that does not
    /// parse is refused instead of serialized as `null`.
    pub fn to_json(&self) -> Result<String, LabError> {
        let manifest = json::parse(&self.manifest_json).map_err(|error| {
            LabError::new(
                "E-HOST-011",
                format!("receipt manifest does not parse: {}", error.message),
            )
        })?;
        Ok(json::write(&JsonValue::Object(vec![
            (
                "receipt_id".into(),
                JsonValue::String(self.receipt_id.0.clone()),
            ),
            (
                "experiment_id".into(),
                JsonValue::String(self.experiment_id.0.clone()),
            ),
            ("manifest".into(), manifest),
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
        ])))
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
