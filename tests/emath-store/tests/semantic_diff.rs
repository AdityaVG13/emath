//! emath-epic-emlib-nz1n.8 contracts: semantic diff and early cutoff.
//!
//! The diff classifies a change as presentation / meaning / evidence /
//! provider and drives rebuilds:
//! - Presentation-only (SourceID changed, MeaningID stable) is NEVER
//!   labeled semantic: the semantic/executable rebuild is skipped with a
//!   cutoff receipt that says so.
//! - A meaning change rebuilds dependents — it is never silently cut
//!   off (the fail-safe rule), even when source changed too.
//! - A provider change invalidates only the recipes dependent on that
//!   provider (the other recipes survive untouched).
//! - Cutoff receipts are deterministic and explain the skipped work.

use std::collections::BTreeSet;

use emath_core::{MeaningId, SourceId};
use emath_store::materialization::MaterializationRecipe;
use emath_store::semantic_diff::{classify, decide, ChangeClass, SemanticSnapshot};

fn snapshot(seed: &[u8], toolchain: &str, evidence: &[&str]) -> SemanticSnapshot {
    SemanticSnapshot::new(
        SourceId::from_bytes(seed),
        MeaningId::from_bytes(seed),
        toolchain,
        evidence,
    )
}

/// Presentation-only (source changed, meaning stable) is NOT semantic:
/// the class is Presentation, the outcome is a cutoff, and the receipt
/// names the stable meaning. The bead's negative control: presentation
/// labeled semantic fails.
#[test]
fn presentation_only_is_not_labeled_semantic() {
    let before = snapshot(b"meaning-one", "gen-a", &[]);
    let after = SemanticSnapshot::new(
        SourceId::from_bytes(b"meaning-one-formatted"),
        MeaningId::from_bytes(b"meaning-one"),
        "gen-a",
        &[],
    );
    let class = classify(&before, &after);
    assert_ne!(
        class,
        ChangeClass::Meaning,
        "presentation-only must never be labeled semantic"
    );
    assert_eq!(class, ChangeClass::Presentation);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Cutoff(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Presentation);
            assert!(
                receipt.reason.contains("meaning stable"),
                "the receipt must name the stable meaning, got: {}",
                receipt.reason
            );
        }
        other => panic!("presentation-only must cut off, got {other:?}"),
    }
}

/// A meaning change rebuilds dependents — never a silent cutoff — even
/// when the source changed as well (meaning change wins).
#[test]
fn meaning_change_never_cuts_off() {
    let before = snapshot(b"meaning-old", "gen-a", &[]);
    let after = SemanticSnapshot::new(
        SourceId::from_bytes(b"meaning-old-formatted"),
        MeaningId::from_bytes(b"meaning-new"),
        "gen-a",
        &[],
    );
    assert_eq!(classify(&before, &after), ChangeClass::Meaning);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Rebuild(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Meaning);
            assert!(
                receipt.reason.contains("dependents"),
                "the receipt must name the rebuild scope, got: {}",
                receipt.reason
            );
        }
        other => panic!("a meaning change must rebuild, got {other:?}"),
    }
}

/// A pure meaning change (source stable — semantic-policy drift) is
/// still classified Meaning: no spelling of stability hides it.
#[test]
fn meaning_change_with_stable_source_is_still_meaning() {
    let before = snapshot(b"same-source", "gen-a", &[]);
    let after = SemanticSnapshot::new(
        SourceId::from_bytes(b"same-source"),
        MeaningId::from_bytes(b"same-source-resolved-differently"),
        "gen-a",
        &[],
    );
    assert_eq!(classify(&before, &after), ChangeClass::Meaning);
}

/// A provider change invalidates ONLY the recipes dependent on that
/// provider: the recipe recorded under the old toolchain is invalidated,
/// recipes under other toolchains survive.
#[test]
fn provider_change_invalidates_only_dependent_recipes() {
    let before = snapshot(b"meaning", "gen-a", &[]);
    let after = SemanticSnapshot::new(
        SourceId::from_bytes(b"meaning"),
        MeaningId::from_bytes(b"meaning"),
        "gen-b",
        &[],
    );
    assert_eq!(classify(&before, &after), ChangeClass::Provider);

    let dependent = MaterializationRecipe::new(
        MeaningId::from_bytes(b"meaning"),
        "gen-a",
        "host",
        b"spec",
    );
    let independent = MaterializationRecipe::new(
        MeaningId::from_bytes(b"meaning"),
        "gen-c",
        "host",
        b"spec",
    );
    let re_pinned = MaterializationRecipe::new(
        MeaningId::from_bytes(b"meaning"),
        "gen-b",
        "host",
        b"spec",
    );
    match decide(&before, &after, &[dependent.clone(), independent.clone(), re_pinned.clone()]) {
        emath_store::semantic_diff::DiffOutcome::ProviderInvalidation {
            receipt,
            invalidated,
        } => {
            assert_eq!(receipt.class, ChangeClass::Provider);
            assert_eq!(
                invalidated,
                vec![dependent.identity()],
                "only the old-provider recipe is invalidated"
            );
        }
        other => panic!("provider change must invalidate recipes, got {other:?}"),
    }
}

/// Evidence changes are additive: no rebuild, a cutoff receipt explains.
#[test]
fn evidence_change_is_additive_only() {
    let before = snapshot(b"meaning", "gen-a", &["ev-1"]);
    let after = snapshot(b"meaning", "gen-a", &["ev-1", "ev-2"]);
    assert_eq!(classify(&before, &after), ChangeClass::Evidence);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Cutoff(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Evidence);
            assert!(
                receipt.reason.contains("evidence"),
                "the receipt must name the evidence work, got: {}",
                receipt.reason
            );
        }
        other => panic!("evidence-only change must cut off, got {other:?}"),
    }
}

/// Identical snapshots classify Unchanged and cut off with a receipt.
#[test]
fn unchanged_inputs_cut_off() {
    let before = snapshot(b"meaning", "gen-a", &["ev-1"]);
    let after = snapshot(b"meaning", "gen-a", &["ev-1"]);
    assert_eq!(classify(&before, &after), ChangeClass::Unchanged);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Cutoff(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Unchanged);
        }
        other => panic!("unchanged inputs must cut off, got {other:?}"),
    }
}

/// Cutoff receipts are deterministic: the same inputs decide to the same
/// receipt (an early-cutoff receipt is evidence, not prose that drifts).
#[test]
fn cutoff_receipts_are_deterministic() {
    let before = snapshot(b"meaning", "gen-a", &[]);
    let after = SemanticSnapshot::new(
        SourceId::from_bytes(b"meaning-formatted"),
        MeaningId::from_bytes(b"meaning"),
        "gen-a",
        &[],
    );
    let first = decide(&before, &after, &[]);
    let second = decide(&before, &after, &[]);
    assert_eq!(first, second, "the same diff must decide one receipt");
}

/// The fail-safe rule as a type-level fact: the diff never returns an
/// unclassified state — every comparison lands in the closed class set.
#[test]
fn class_set_is_closed() {
    // Exhaustively cover the closed class set through the classifier.
    let base = snapshot(b"meaning", "gen-a", &["ev-1"]);
    assert_eq!(classify(&base, &base), ChangeClass::Unchanged);
    assert_eq!(
        classify(
            &base,
            &SemanticSnapshot::new(
                SourceId::from_bytes(b"meaning-z"),
                MeaningId::from_bytes(b"meaning"),
                "gen-a",
                &["ev-1"]
            )
        ),
        ChangeClass::Presentation
    );
    assert_eq!(
        classify(
            &base,
            &SemanticSnapshot::new(
                SourceId::from_bytes(b"meaning"),
                MeaningId::from_bytes(b"meaning-z"),
                "gen-a",
                &["ev-1"]
            )
        ),
        ChangeClass::Meaning
    );
    assert_eq!(
        classify(
            &base,
            &SemanticSnapshot::new(
                SourceId::from_bytes(b"meaning"),
                MeaningId::from_bytes(b"meaning"),
                "gen-b",
                &["ev-1"]
            )
        ),
        ChangeClass::Provider
    );
    assert_eq!(
        classify(
            &base,
            &SemanticSnapshot::new(
                SourceId::from_bytes(b"meaning"),
                MeaningId::from_bytes(b"meaning"),
                "gen-a",
                &["ev-2"]
            )
        ),
        ChangeClass::Evidence
    );
}
