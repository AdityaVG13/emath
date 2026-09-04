//! Frontier engine: aggressive candidate generation, strict admission.
//!
//! Generation is deliberately asymmetric with admission: candidates start
//! unverified with zero evidence, and only the held-out challenge plus the
//! campaign checklist can promote them. Pipeline order is fixed:
//! generate → verify → benchmark → campaign.

use crate::tuning::{
    ExecutionDelta, JointCandidate, SemanticChange, SemanticVariableKind, WorldDelta,
};
use emath_term::SymbolId;
use emath_world_ir::WorldId;

/// Frontier document schema id.
pub const FRONTIER_SCHEMA: &str = "emath.frontier";
/// Frontier document version.
pub const FRONTIER_VERSION: u32 = 1;

/// Provenance recorded on changes proposed by the algebraic generator.
pub const ALGEBRAIC_PROVENANCE: &str = "algebraic-rewrite";

/// One algebraic rewrite hypothesis: replace the meaning of `symbol` with
/// a claimed-equivalent implementation. The claim is *not* trusted; the
/// held-out challenge decides whether it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteRule {
    /// Stable rule label (becomes the candidate label).
    pub label: String,
    /// Operator whose meaning the rewrite replaces.
    pub symbol: SymbolId,
    /// Canonical description of the replacement meaning.
    pub replacement: String,
}

impl RewriteRule {
    /// Deterministic canonical form (sort and dedup key).
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "rewrite:{}:{}:{}",
            self.label, self.symbol.0, self.replacement
        )
    }
}

/// Generates one unverified [`JointCandidate`] per rewrite rule; sorted,
/// deduplicated, budget-capped, always unverified and evidence-free.
#[must_use]
pub fn generate_algebraic_candidates(
    base_world: WorldId,
    rules: &[RewriteRule],
    execution: &ExecutionDelta,
    budget: usize,
) -> Vec<JointCandidate> {
    let mut ordered: Vec<&RewriteRule> = rules.iter().collect();
    ordered.sort_by_key(|rule| rule.canonical());
    ordered.dedup_by_key(|rule| rule.canonical());
    ordered.truncate(budget);
    ordered
        .into_iter()
        .map(|rule| {
            let change = SemanticChange {
                kind: SemanticVariableKind::Operator,
                symbol: Some(rule.symbol.clone()),
                description: rule.replacement.clone(),
                provenance: ALGEBRAIC_PROVENANCE.to_string(),
            };
            JointCandidate::new(
                rule.label.clone(),
                WorldDelta::new(base_world, vec![change]),
                execution.clone(),
                false,
                0,
            )
        })
        .collect()
}

/// Runs the held-out challenge: passes only when the oracle accepts every
/// change; the rebuilt candidate records the verdict in its identity.
#[must_use]
pub fn verify_held_out(
    candidate: &JointCandidate,
    oracle: impl Fn(&SemanticChange) -> bool,
    evidence_units: u32,
) -> JointCandidate {
    let passed = candidate.world.changes.iter().all(oracle);
    JointCandidate::new(
        candidate.label.clone(),
        candidate.world.clone(),
        candidate.execution.clone(),
        passed,
        if passed { evidence_units } else { 0 },
    )
}
