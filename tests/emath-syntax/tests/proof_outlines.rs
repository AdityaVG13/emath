//! Proof outlines as sections (B13 + 05 §7.2) — THIN design+slice: obligation kinds as DATA
//! (assumption / lemma / check / qed), refuse incomplete outlines, NO
//! full ELP series (no ProofChecker execution, no by_cases, no typed
//! holes, no evidence levels — those are the named follow-ups).
//!
//! Contracts:
//! - `proofs:` is an admitted section inside existing kinds (03 rule 3:
//!   expansiveness via sections, not new kinds); outlines are nested
//!   `outline <Name>:` sections; steps are obligation kinds as data.
//! - COMPLETENESS: an outline must contain at least one step and end
//!   with `qed <target>`; `qed`/`check` must name a step declared
//!   earlier in the same outline; unknown step kinds refuse naming the
//!   four kinds. A complete outline ADMITS as data.
//! - Proofs are ADDITIVE AUTHORITY, never admission tickets: the same
//!   declaration without a `proofs:` section admits identically, and
//!   nothing in a proofs section gates artifact production. `check`
//!   steps are DATA obligations — no checker runs in this slice (no
//!   fake verification).
//! - `proofs:` stays structurally separate from `definitions:`
//!   (justification vs meaning) — outline claims are never lowered as
//!   definitions or constraints.
//!
//! Failure-first evidence: live probes before the slice — both the
//! complete and the incomplete outline refused E-SEC-101 at the
//! section whitelist (recorded in the pack).

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const COMPLETE_OUTLINE: &str = "\
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a

    proofs:
        outline NonNegativity:
            assumption finite_a: is_finite(a)
            lemma square_nonneg: y >= 0.0
            check square_nonneg
            qed square_nonneg
";

const INCOMPLETE_OUTLINE: &str = "\
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a

    proofs:
        outline NonNegativity:
            assumption finite_a: is_finite(a)
            lemma square_nonneg: y >= 0.0
            check square_nonneg
";

const UNKNOWN_STEP: &str = "\
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a

    proofs:
        outline NonNegativity:
            assumption finite_a: is_finite(a)
            meditate deeper: y >= 0.0
            qed square_nonneg
";

const DANGLING_QED: &str = "\
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a

    proofs:
        outline NonNegativity:
            assumption finite_a: is_finite(a)
            qed never_declared
";

const NO_PROOFS: &str = "\
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a
";

use emath_test_harness::{Probe, Source, boot};

#[test]
fn proof_outlines() {
    boot();
    let mut probe = Probe::new("Proof outlines as sections (B13 + 05 §7.2) — THIN design+slice: obligation kinds as DATA (assumption / lemma / check / qed), refuse incomplete");
    probe.case("complete_outline_admits_as_data", |p| {
    let f0 = p.failures().len();

    let errors = check(COMPLETE_OUTLINE, "proof-complete");
    p.demand("1",errors.is_empty(), format!(
        "a complete obligation outline (assumption/lemma/check/qed) must \
         admit as data; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("incomplete_outline_refuses_naming_the_rule", |p| {
    let f0 = p.failures().len();

    let errors = check(INCOMPLETE_OUTLINE, "proof-incomplete");
    p.demand("1",errors
            .iter()
            .any(|e| e.starts_with("E-SYN-101") && e.contains("incomplete") && e.contains("qed")), format!(
        "an outline without a closing qed must refuse naming the \
         completeness rule; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unknown_step_kind_refuses_naming_the_four", |p| {
    let f0 = p.failures().len();

    let errors = check(UNKNOWN_STEP, "proof-unknown-step");
    p.demand("1",errors.iter().any(|e| e.starts_with("E-SYN-101")
            && e.contains("assumption")
            && e.contains("lemma")
            && e.contains("check")
            && e.contains("qed")), format!(
        "an unknown obligation kind must refuse naming the four kinds; \
         got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("dangling_qed_target_refuses", |p| {
    let f0 = p.failures().len();

    let errors = check(DANGLING_QED, "proof-dangling-qed");
    p.demand("1",errors
            .iter()
            .any(|e| e.starts_with("E-SYN-101") && e.contains("never_declared")), format!(
        "a qed naming an undeclared obligation must refuse; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("proofs_are_not_admission_tickets", |p| {
    let f0 = p.failures().len();

    let errors = check(NO_PROOFS, "proof-additive-authority");
    p.demand("1",errors.is_empty(), format!(
        "an unproved declaration must compile to its full artifact \
         (proofs are additive authority); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
