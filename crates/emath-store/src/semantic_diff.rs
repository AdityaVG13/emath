//! Semantic diff and early cutoff.
//!
//! The diff classifies a change between two [`SemanticSnapshot`]s as one
//! of the closed classes — presentation / meaning / evidence / provider
//! — and drives rebuilds:
//!
//! - **Presentation-only** (SourceID changed, MeaningID stable) is
//!   NEVER labeled semantic: `decide` returns a cutoff whose receipt
//!   names the stable meaning. A formatter pass or a comment edit never
//!   rebuilds semantics.
//! - **Meaning** changes rebuild dependents — never a silent cutoff
//!   (the fail-safe rule: an unclassified semantic change must not cut
//!   off, so a meaning change always wins the classification, even over
//!   a simultaneous source change).
//! - **Provider** changes (generator/toolchain identity) invalidate
//!   only the recipes dependent on that provider — the materialization
//!   bookkeeping of any other toolchain survives untouched.
//! - **Evidence** changes are additive: no rebuild, a cutoff receipt.
//!
//! Cutoff receipts are deterministic values that explain the skipped
//! work — an early cutoff is evidence, not prose that drifts.
//!
//! Determinism class: pure sequence. No wall-clock, no randomness; the
//! classification is a pure function of the two snapshots, and no math
//! equivalence is ever guessed (MeaningID equality is the only semantic
//! oracle consulted).

use std::collections::BTreeSet;

use emath_core::{MeaningId, SourceId};

use crate::materialization::MaterializationRecipe;

/// One side of a semantic diff: the identity inputs the meaning store
/// tracks. `evidence` carries the attached evidence ids (canonical id
/// strings) — additive receipts do not change meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSnapshot {
    source: SourceId,
    meaning: MeaningId,
    toolchain: String,
    evidence: BTreeSet<String>,
}

impl SemanticSnapshot {
    #[must_use]
    pub fn new(source: SourceId, meaning: MeaningId, toolchain: &str, evidence: &[&str]) -> Self {
        Self {
            source,
            meaning,
            toolchain: toolchain.to_string(),
            evidence: evidence.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    #[must_use]
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    #[must_use]
    pub fn meaning(&self) -> &MeaningId {
        &self.meaning
    }

    #[must_use]
    pub fn toolchain(&self) -> &str {
        &self.toolchain
    }

    #[must_use]
    pub fn evidence(&self) -> &BTreeSet<String> {
        &self.evidence
    }
}

/// The closed change-class set. Every snapshot comparison lands in
/// exactly one class — the diff never returns an unclassified state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeClass {
    /// Nothing changed: skip all work.
    Unchanged,
    /// Source spelling changed; MeaningID stable — not semantic.
    Presentation,
    /// MeaningID changed — dependents rebuild, never a silent cutoff.
    Meaning,
    /// Generator/toolchain identity changed — dependent recipes
    /// invalidated.
    Provider,
    /// Attached evidence changed additively; meaning and source stable.
    Evidence,
}

impl std::fmt::Display for ChangeClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unchanged => "unchanged",
            Self::Presentation => "presentation",
            Self::Meaning => "meaning",
            Self::Provider => "provider",
            Self::Evidence => "evidence",
        })
    }
}

/// A cutoff receipt: the deterministic explanation of skipped (or
/// forced) work. `class` is the classification the decision was made
/// from; `reason` names what was skipped and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutoffReceipt {
    pub class: ChangeClass,
    pub reason: String,
}

impl std::fmt::Display for CutoffReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.class, self.reason)
    }
}

/// The rebuild decision for a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffOutcome {
    /// Early cutoff: skip the semantic/executable rebuild; the receipt
    /// explains the skipped work.
    Cutoff(CutoffReceipt),
    /// Rebuild the dependents; the receipt carries the reason.
    Rebuild(CutoffReceipt),
    /// The provider changed: the semantic meaning did not, but the
    /// recipes recorded under the old provider are invalidated (the
    /// other recipes survive).
    ProviderInvalidation {
        receipt: CutoffReceipt,
        invalidated: Vec<emath_core::RecipeId>,
    },
}

/// Classify the change between two snapshots. Precedence is
/// fail-safe-first: a meaning change is classified as MEANING even when
/// source or evidence changed alongside it (an unclassified semantic
/// change must never silently cut off); presentation is only claimed
/// when the meaning is stable.
#[must_use]
pub fn classify(before: &SemanticSnapshot, after: &SemanticSnapshot) -> ChangeClass {
    if before.meaning != after.meaning {
        ChangeClass::Meaning
    } else if before.toolchain != after.toolchain {
        ChangeClass::Provider
    } else if before.source != after.source {
        ChangeClass::Presentation
    } else if before.evidence != after.evidence {
        ChangeClass::Evidence
    } else {
        ChangeClass::Unchanged
    }
}

/// Decide the rebuild outcome for a diff over the given recipes. A
/// provider change invalidates exactly the recipes whose toolchain is
/// the OLD provider (their identity is pinned to it); recipes under
/// other toolchains — including the new one — survive.
pub fn decide(
    before: &SemanticSnapshot,
    after: &SemanticSnapshot,
    recipes: &[MaterializationRecipe],
) -> DiffOutcome {
    match classify(before, after) {
        ChangeClass::Meaning => DiffOutcome::Rebuild(CutoffReceipt {
            class: ChangeClass::Meaning,
            reason: "meaning changed: dependents rebuild (never a silent cutoff)".to_string(),
        }),
        ChangeClass::Provider => {
            let invalidated = recipes
                .iter()
                .filter(|recipe| recipe.toolchain() == before.toolchain)
                .map(|recipe| recipe.identity())
                .collect();
            DiffOutcome::ProviderInvalidation {
                receipt: CutoffReceipt {
                    class: ChangeClass::Provider,
                    reason: format!(
                        "provider changed {} -> {}: only recipes recorded under the old \
                         provider are invalidated; the semantic rebuild is skipped because \
                         the meaning is stable",
                        before.toolchain, after.toolchain
                    ),
                },
                invalidated,
            }
        }
        ChangeClass::Presentation => DiffOutcome::Cutoff(CutoffReceipt {
            class: ChangeClass::Presentation,
            reason: "source changed with meaning stable: presentation-only; semantic and \
                     executable rebuilds skipped"
                .to_string(),
        }),
        ChangeClass::Evidence => DiffOutcome::Cutoff(CutoffReceipt {
            class: ChangeClass::Evidence,
            reason: "evidence changed additively with meaning and source stable: no rebuild"
                .to_string(),
        }),
        ChangeClass::Unchanged => DiffOutcome::Cutoff(CutoffReceipt {
            class: ChangeClass::Unchanged,
            reason: "nothing changed: all work skipped".to_string(),
        }),
    }
}
