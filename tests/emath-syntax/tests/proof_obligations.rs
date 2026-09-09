//! Proof-obligation machine records + ProofChecker contract (05 §7.2).
//!
//! Contracts:
//! - a complete outline lowers to `emath.proof-obligation v1` records:
//!   assumptions accumulate as hypotheses for later lemmas; claim text
//!   rides as data;
//! - the obligation hash is deterministic content identity over the
//!   canonical JSON (same outline = same hash; a changed claim = a
//!   different hash — mutation-checked);
//! - the ProofChecker CONTRACT is real: a checker receives records and
//!   returns typed verdicts; a checker that cannot decide stays silent
//!   (no fabricated verdicts); a missing checker is an empty verdict
//!   list — proofs remain additive authority, never admission tickets;
//! - the multi-record envelope is canonical stable JSON (replay =
//!   byte-identical).
//!
//! Failure-first evidence: suite written before the module existed
//! (RED = E0432 unresolved import `emath_sema::proofs`).

use emath_sema::proofs::{self, ProofChecker, ProofObligation};

const OUTLINE_STEPS: &[(&'static str, &str, Option<&str>)] = &[
    ("assumption", "finite_a", Some("is_finite(a)")),
    ("lemma", "square_nonneg", Some("y >= 0.0")),
];

use emath_test_harness::{Probe, boot};

#[test]
fn proof_obligations() {
    boot();
    let mut probe = Probe::new("Proof-obligation machine records + ProofChecker contract (05 §7.2). Contracts: - a complete outline lowers to `emath.proof-obligation v1` records:");
    probe.case("outline_lowers_to_schema_records", |p| {
    let f0 = p.failures().len();

    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    p.eq("1", records.len(), 2);
    let assumption = &records[0];
    p.demand("2", (assumption.kind) == ("assumption"), format!("expected {:?}, got {:?}", ("assumption"), (assumption.kind)));
    if p.failures().len() != f0 { return; }
    p.demand("3", (assumption.name) == ("finite_a"), format!("expected {:?}, got {:?}", ("finite_a"), (assumption.name)));
    if p.failures().len() != f0 { return; }
    p.demand("4",assumption.hypotheses.is_empty(), stringify!(assumption.hypotheses.is_empty()));
    if p.failures().len() != f0 { return; }
    let lemma = &records[1];
    p.demand("5", (lemma.kind) == ("lemma"), format!("expected {:?}, got {:?}", ("lemma"), (lemma.kind)));
    if p.failures().len() != f0 { return; }
    p.demand("6", (lemma.hypotheses) == (vec!["finite_a".to_string()]), format!("expected {:?}, got {:?}", (vec!["finite_a".to_string()]), (lemma.hypotheses)));
    if p.failures().len() != f0 { return; }
    p.eq("7", lemma.claim.as_deref(), Some("y >= 0.0"));

    });
    probe.case("record_json_is_canonical_v1", |p| {
    let f0 = p.failures().len();

    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let json = records[1].to_json();
    p.demand("1",json.starts_with("{\"schema\":\"emath.proof-obligation v1\""), format!(
        "record leads with the versioned schema; got: {json}"
    ));
    if p.failures().len() != f0 { return; }
    for key in [
        "\"outline\":",
        "\"kind\":",
        "\"name\":",
        "\"claim\":",
        "\"hypotheses\":",
    ] {
        p.demand("2",json.contains(key), format!( "record carries {key}; got: {json}"));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.case("obligation_hash_is_deterministic_and_claim_sensitive", |p| {

    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let again = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    p.eq("1", records[1].obligation_hash(), again[1].obligation_hash());
    let different_claim: &[(&'static str, &str, Option<&str>)] = &[
        ("assumption", "finite_a", Some("is_finite(a)")),
        ("lemma", "square_nonneg", Some("y >= 0.0 ")),
    ];
    let changed = proofs::lower_outline("NonNegativity", different_claim);
    p.ne("2", records[1].obligation_hash(), changed[1].obligation_hash());

    });
    probe.case("checker_contract_records_typed_verdicts_and_stays_silent_on_unknown", |p| {
    let f0 = p.failures().len();

    struct TrustingChecker;
    impl ProofChecker for TrustingChecker {
        fn name(&self) -> &'static str {
            "test.trusting"
        }
        fn check(&self, _obligation: &ProofObligation) -> Result<bool, String> {
            Ok(true)
        }
    }
    struct UndecidedChecker;
    impl ProofChecker for UndecidedChecker {
        fn name(&self) -> &'static str {
            "test.undecided"
        }
        fn check(&self, _obligation: &ProofObligation) -> Result<bool, String> {
            Err("cannot decide".to_string())
        }
    }
    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);

    let verdicts = proofs::check_with(&TrustingChecker, &records);
    p.eq("1", verdicts.len(), 1);
    p.demand("2",verdicts[0].discharged, stringify!(verdicts[0].discharged));
    if p.failures().len() != f0 { return; }
    p.demand("3", (verdicts[0].checker) == ("test.trusting"), format!("expected {:?}, got {:?}", ("test.trusting"), (verdicts[0].checker)));
    if p.failures().len() != f0 { return; }
    p.eq("4", verdicts[0].obligation_hash.clone(), records[1].obligation_hash());

    // A checker that cannot decide stays silent: no fabricated verdicts.
    let undecided = proofs::check_with(&UndecidedChecker, &records);
    p.demand("5",undecided.is_empty(), format!(
        "an undecided checker records nothing (never a guess); got: {undecided:#?}"
    ));
    if p.failures().len() != f0 { return; }

    // A missing checker is an empty verdict list — compilation is
    // unaffected either way (proofs are additive authority).

    });
    probe.case("envelope_is_canonical_and_replay_is_byte_identical", |p| {
    let f0 = p.failures().len();

    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let first = proofs::outline_records_json("NonNegativity", &records);
    let second = proofs::outline_records_json("NonNegativity", &records);
    p.eq("1", &first, &second);
    p.demand("2",first.contains("\"schema\":\"emath.proof-obligation v1\"")
            && first.contains("\"records\":["), format!(
        "envelope carries schema + records; got: {first}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
