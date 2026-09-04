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

#[test]
fn outline_lowers_to_schema_records() {
    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    assert_eq!(records.len(), 2, "assumption + lemma = two records");
    let assumption = &records[0];
    assert_eq!(assumption.kind, "assumption");
    assert_eq!(assumption.name, "finite_a");
    assert!(assumption.hypotheses.is_empty());
    let lemma = &records[1];
    assert_eq!(lemma.kind, "lemma");
    assert_eq!(
        lemma.hypotheses,
        vec!["finite_a".to_string()],
        "the assumption accumulates as the lemma's hypothesis"
    );
    assert_eq!(lemma.claim.as_deref(), Some("y >= 0.0"));
}

#[test]
fn record_json_is_canonical_v1() {
    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let json = records[1].to_json();
    assert!(
        json.starts_with("{\"schema\":\"emath.proof-obligation v1\""),
        "record leads with the versioned schema; got: {json}"
    );
    for key in [
        "\"outline\":",
        "\"kind\":",
        "\"name\":",
        "\"claim\":",
        "\"hypotheses\":",
    ] {
        assert!(json.contains(key), "record carries {key}; got: {json}");
    }
}

#[test]
fn obligation_hash_is_deterministic_and_claim_sensitive() {
    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let again = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    assert_eq!(
        records[1].obligation_hash(),
        again[1].obligation_hash(),
        "same outline = same obligation hash (determinism)"
    );
    let different_claim: &[(&'static str, &str, Option<&str>)] = &[
        ("assumption", "finite_a", Some("is_finite(a)")),
        ("lemma", "square_nonneg", Some("y >= 0.0 ")),
    ];
    let changed = proofs::lower_outline("NonNegativity", different_claim);
    assert_ne!(
        records[1].obligation_hash(),
        changed[1].obligation_hash(),
        "a changed claim must change the obligation hash (the hash pins \
         the content, mutation-checked)"
    );
}

#[test]
fn checker_contract_records_typed_verdicts_and_stays_silent_on_unknown() {
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
    assert_eq!(verdicts.len(), 1, "only the lemma is checkable");
    assert!(verdicts[0].discharged);
    assert_eq!(verdicts[0].checker, "test.trusting");
    assert_eq!(verdicts[0].obligation_hash, records[1].obligation_hash());

    // A checker that cannot decide stays silent: no fabricated verdicts.
    let undecided = proofs::check_with(&UndecidedChecker, &records);
    assert!(
        undecided.is_empty(),
        "an undecided checker records nothing (never a guess); got: {undecided:#?}"
    );

    // A missing checker is an empty verdict list — compilation is
    // unaffected either way (proofs are additive authority).
}

#[test]
fn envelope_is_canonical_and_replay_is_byte_identical() {
    let records = proofs::lower_outline("NonNegativity", OUTLINE_STEPS);
    let first = proofs::outline_records_json("NonNegativity", &records);
    let second = proofs::outline_records_json("NonNegativity", &records);
    assert_eq!(first, second, "replay: byte-identical envelope");
    assert!(
        first.contains("\"schema\":\"emath.proof-obligation v1\"")
            && first.contains("\"records\":["),
        "envelope carries schema + records; got: {first}"
    );
}
