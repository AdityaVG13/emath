//! contracts: semantic diff and early cutoff.

use emath_core::{MeaningId, SourceId};
use emath_store::materialization::MaterializationRecipe;
use emath_store::semantic_diff::{ChangeClass, SemanticSnapshot, classify, decide};
use emath_test_harness::Probe;

fn snapshot(seed: &[u8], toolchain: &str, evidence: &[&str]) -> SemanticSnapshot {
    SemanticSnapshot::new(SourceId::from_bytes(seed), MeaningId::from_bytes(seed), toolchain, evidence)
}

fn snap(source: &[u8], meaning: &[u8], toolchain: &str, evidence: &[&str]) -> SemanticSnapshot {
    SemanticSnapshot::new(SourceId::from_bytes(source), MeaningId::from_bytes(meaning), toolchain, evidence)
}

#[test]
fn semantic_diff() {
    let mut p = Probe::new("diff classifies presentation/meaning/evidence/provider and cuts off safely");
    p.case("presentation", |p| {
        let before = snapshot(b"meaning-one", "gen-a", &[]);
        let after = snap(b"meaning-one-formatted", b"meaning-one", "gen-a", &[]);
        p.ne("not-semantic", classify(&before, &after), ChangeClass::Meaning);
        p.eq("class", classify(&before, &after), ChangeClass::Presentation);
        match decide(&before, &after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Cutoff(r) => {
                p.eq("receipt", r.class.clone(), ChangeClass::Presentation);
                p.contains("reason", &r.reason, "meaning stable");
            }
            other => { p.fail("cutoff", format!("must cut off, got {other:?}")); },
        }
    });
    p.case("meaning", |p| {
        let before = snapshot(b"meaning-old", "gen-a", &[]);
        let after = snap(b"meaning-old-formatted", b"meaning-new", "gen-a", &[]);
        p.eq("class", classify(&before, &after), ChangeClass::Meaning);
        match decide(&before, &after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Rebuild(r) => {
                p.eq("receipt", r.class.clone(), ChangeClass::Meaning);
                p.contains("scope", &r.reason, "dependents");
            }
            other => { p.fail("rebuild", format!("must rebuild, got {other:?}")); },
        }
        p.eq("stable-source", classify(&snapshot(b"same-source", "gen-a", &[]), &snap(b"same-source", b"same-source-resolved-differently", "gen-a", &[])), ChangeClass::Meaning);
    });
    p.case("provider", |p| {
        let before = snapshot(b"meaning", "gen-a", &[]);
        let after = snap(b"meaning", b"meaning", "gen-b", &[]);
        p.eq("class", classify(&before, &after), ChangeClass::Provider);
        let dependent = MaterializationRecipe::new(MeaningId::from_bytes(b"meaning"), "gen-a", "host", b"spec");
        let independent = MaterializationRecipe::new(MeaningId::from_bytes(b"meaning"), "gen-c", "host", b"spec");
        let repinned = MaterializationRecipe::new(MeaningId::from_bytes(b"meaning"), "gen-b", "host", b"spec");
        match decide(&before, &after, &[dependent.clone(), independent.clone(), repinned.clone()]) {
            emath_store::semantic_diff::DiffOutcome::ProviderInvalidation { receipt, invalidated } => {
                p.eq("receipt", receipt.class.clone(), ChangeClass::Provider);
                p.eq("invalidated", invalidated, vec![dependent.identity()]);
            }
            other => { p.fail("invalidate", format!("must invalidate, got {other:?}")); },
        }
    });
    p.case("evidence-unchanged", |p| {
        let before = snapshot(b"meaning", "gen-a", &["ev-1"]);
        let after = snapshot(b"meaning", "gen-a", &["ev-1", "ev-2"]);
        p.eq("class", classify(&before, &after), ChangeClass::Evidence);
        match decide(&before, &after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Cutoff(r) => {
                p.eq("receipt", r.class.clone(), ChangeClass::Evidence);
                p.contains("reason", &r.reason, "evidence");
            }
            other => { p.fail("cutoff", format!("must cut off, got {other:?}")); },
        }
        let same = snapshot(b"meaning", "gen-a", &["ev-1"]);
        p.eq("unchanged", classify(&same, &same), ChangeClass::Unchanged);
        match decide(&same, &same, &[]) {
            emath_store::semantic_diff::DiffOutcome::Cutoff(r) => { p.eq("receipt", r.class.clone(), ChangeClass::Unchanged); },
            other => { p.fail("cutoff", format!("must cut off, got {other:?}")); },
        }
        p.eq("deterministic", decide(&before, &after, &[]), decide(&before, &after, &[]));
    });
    p.case("closed", |p| {
        let base = snapshot(b"meaning", "gen-a", &["ev-1"]);
        p.eq("unchanged", classify(&base, &base), ChangeClass::Unchanged);
        p.eq("presentation", classify(&base, &snap(b"meaning-z", b"meaning", "gen-a", &["ev-1"])), ChangeClass::Presentation);
        p.eq("meaning", classify(&base, &snap(b"meaning", b"meaning-z", "gen-a", &["ev-1"])), ChangeClass::Meaning);
        p.eq("provider", classify(&base, &snap(b"meaning", b"meaning", "gen-b", &["ev-1"])), ChangeClass::Provider);
        p.eq("evidence", classify(&base, &snap(b"meaning", b"meaning", "gen-a", &["ev-2"])), ChangeClass::Evidence);
    });
    p.finish();
}
