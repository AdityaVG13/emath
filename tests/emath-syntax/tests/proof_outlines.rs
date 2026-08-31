//! Proof outlines as sections (bead emath-r3-proofs-0qua, B13 + 05
//! §7.2) — THIN design+slice per orch ruling: obligation kinds as DATA
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

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
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

#[test]
fn complete_outline_admits_as_data() {
    let errors = check(COMPLETE_OUTLINE, "proof-complete");
    assert!(
        errors.is_empty(),
        "a complete obligation outline (assumption/lemma/check/qed) must \
         admit as data; got: {errors:#?}"
    );
}

#[test]
fn incomplete_outline_refuses_naming_the_rule() {
    let errors = check(INCOMPLETE_OUTLINE, "proof-incomplete");
    assert!(
        errors.iter().any(|e| e.starts_with("E-SYN-101")
            && e.contains("incomplete")
            && e.contains("qed")),
        "an outline without a closing qed must refuse naming the \
         completeness rule; got: {errors:#?}"
    );
}

#[test]
fn unknown_step_kind_refuses_naming_the_four() {
    let errors = check(UNKNOWN_STEP, "proof-unknown-step");
    assert!(
        errors.iter().any(|e| e.starts_with("E-SYN-101")
            && e.contains("assumption")
            && e.contains("lemma")
            && e.contains("check")
            && e.contains("qed")),
        "an unknown obligation kind must refuse naming the four kinds; \
         got: {errors:#?}"
    );
}

#[test]
fn dangling_qed_target_refuses() {
    let errors = check(DANGLING_QED, "proof-dangling-qed");
    assert!(
        errors.iter().any(|e| e.starts_with("E-SYN-101")
            && e.contains("never_declared")),
        "a qed naming an undeclared obligation must refuse; got: {errors:#?}"
    );
}

#[test]
fn proofs_are_not_admission_tickets() {
    let errors = check(NO_PROOFS, "proof-additive-authority");
    assert!(
        errors.is_empty(),
        "an unproved declaration must compile to its full artifact \
         (proofs are additive authority); got: {errors:#?}"
    );
}
