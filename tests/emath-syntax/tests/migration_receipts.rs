//! Migrate receipt contract core (
//! 05 §5) — library thin slice. The `emath migrate` CLI subcommand is
//! DEFERRED (the CLI dispatch files were under active foreign
//! modification); the contract core is honest now:
//! - rules self-classify (`Respell` — a rule that cannot classify
//!   itself does not ship);
//! - identity verification is LOAD-BEARING: a respell emits only when
//!   the re-lowered semantic identity is byte-identical; an
//!   identity-breaking rewrite refuses E-MIG-VERIFY-FAIL and emits
//!   nothing (the migration itself is a bug);
//! - a source that does not admit refuses E-MIG-SOURCE-REFUSES —
//!   migrate never rewrites a refusing source;
//! - idempotence: rewrite == input is a no-op with an empty rule list;
//! - the receipt is canonical stable JSON: replay is byte-identical.
//!
//! The concrete canonical-format rule binding (lossless formatter)
//! lives HERE in the test (production `emath-sema` deliberately does
//! not link `emath-syntax` — kernel seam): the caller injects the
//! rewrite, the core verifies it.
//!
//! Failure-first evidence: the suite was written before the module
//! existed (RED = E0432 unresolved import `emath_sema::migrate`).

use emath_core::limits::Limits;
use emath_sema::migrate;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

/// The canonical-format respell binding: lossless formatter rewrite.
/// (The rule id rides with the caller until the rule registry lands.)
fn canonical_format(source: &str) -> String {
    let lossless = emath_syntax::parse_lossless(source, emath_core::FileId(0), &Limits::default());
    emath_syntax::format_lossless(&lossless)
}

const CANONICAL: &str = "\
emath function plain:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = x * 2.0
";

const NON_CANONICAL: &str = "\
emath function plain:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y=x*2.0
";

const REFUSING_SOURCE: &str = "\
emath function broken:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = undefined_name
";

#[test]
fn canonical_source_is_idempotent_no_op() {
    install_source_parser();
    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        CANONICAL,
        CANONICAL,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    assert!(
        outcome.receipt.rules_applied.is_empty(),
        "second-run idempotence: a canonical source applies no rules; got: {:#?}",
        outcome.receipt.rules_applied
    );
    assert_eq!(outcome.receipt.verdict, "complete");
    assert!(outcome.receipt.identity_verified);
    assert!(outcome.rewritten_source.is_none());
}

#[test]
fn noncanonical_source_respells_with_identity_verified() {
    install_source_parser();
    let rewritten = canonical_format(NON_CANONICAL);
    assert_ne!(rewritten, NON_CANONICAL, "fixture must be non-canonical");
    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        NON_CANONICAL,
        &rewritten,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    assert!(
        outcome.receipt.refusals.is_empty(),
        "a verified respell must not refuse; got: {:#?}",
        outcome.receipt.refusals
    );
    assert_eq!(outcome.receipt.rules_applied.len(), 1);
    let rule = &outcome.receipt.rules_applied[0];
    assert_eq!(rule.kind, migrate::RuleKind::Respell);
    assert_eq!(rule.identity_delta, "none");
    assert_eq!(
        rule.before_hash, rule.after_hash,
        "respell must verify identity preservation byte-for-byte"
    );
    assert!(outcome.receipt.identity_verified);
    assert_eq!(
        outcome.rewritten_source.as_deref(),
        Some(rewritten.as_str()),
        "verified respell emits the rewritten source"
    );
}

#[test]
fn identity_breaking_rewrite_refuses_and_emits_nothing() {
    install_source_parser();
    // A rewrite that changes meaning (the load-bearing property): the
    // verify gate must refuse and emit NOTHING.
    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        CANONICAL,
        "emath function plain:\n    inputs:\n        x: Float64\n\n    outputs:\n        y: Float64\n\n    definitions:\n        y = x * 3.0\n",
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    assert!(
        matches!(
            &outcome.receipt.refusals[..],
            [r] if r.code == migrate::E_MIG_VERIFY_FAIL
        ),
        "an identity-breaking rewrite must refuse E-MIG-VERIFY-FAIL; got: {:#?}",
        outcome.receipt.refusals
    );
    assert!(outcome.rewritten_source.is_none());
    assert_eq!(outcome.receipt.verdict, "partial-refused");
}

#[test]
fn refusing_source_is_named_refusal_not_rewrite() {
    install_source_parser();
    let outcome = migrate::migrate_verified_rewrite(
        "broken.emath",
        REFUSING_SOURCE,
        REFUSING_SOURCE,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    assert!(
        outcome
            .receipt
            .refusals
            .iter()
            .any(|r| r.code == migrate::E_MIG_SOURCE_REFUSES),
        "a source that does not admit refuses E-MIG-SOURCE-REFUSES; got: {:#?}",
        outcome.receipt.refusals
    );
    assert!(outcome.rewritten_source.is_none());
    assert_eq!(outcome.receipt.verdict, "partial-refused");
}

#[test]
fn receipt_json_is_canonical_and_replay_is_byte_identical() {
    install_source_parser();
    let rewritten = canonical_format(NON_CANONICAL);
    let first = migrate::migrate_verified_rewrite(
        "plain.emath",
        NON_CANONICAL,
        &rewritten,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    let second = migrate::migrate_verified_rewrite(
        "plain.emath",
        NON_CANONICAL,
        &rewritten,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    assert_eq!(
        first.receipt.to_canonical_json(),
        second.receipt.to_canonical_json(),
        "replay: same input = byte-identical receipt"
    );
    let json = first.receipt.to_canonical_json();
    assert!(
        json.starts_with("{\"schema\":\"emath.migration-receipt v1\""),
        "receipt JSON must lead with the versioned schema key; got: {json}"
    );
    assert!(json.contains("\"identity_verified\":true"));
    assert!(json.contains("\"kind\":\"respell\""));
    assert!(json.contains("\"identity_delta\":\"none\""));
}

#[test]
fn semantic_rule_records_checked_before_and_after_meaning_ids() {
    install_source_parser();
    let corrected = CANONICAL.replace("x * 2.0", "x * 3.0");
    let outcome = migrate::migrate_semantic_rewrite(
        "plain.emath",
        CANONICAL,
        &corrected,
        migrate::RULE_SEMANTIC_CORRECTION.id,
    );
    assert!(
        outcome.receipt.refusals.is_empty(),
        "a registered semantic correction must admit both meanings: {:#?}",
        outcome.receipt.refusals
    );
    let [rule] = outcome.receipt.rules_applied.as_slice() else {
        panic!("semantic correction must apply exactly one rule");
    };
    assert_eq!(rule.kind, migrate::RuleKind::Semantic);
    assert_ne!(rule.before_hash, rule.after_hash);
    assert_eq!(
        rule.identity_delta,
        format!("{} -> {}", rule.before_hash, rule.after_hash)
    );
    assert_eq!(
        outcome.rewritten_source.as_deref(),
        Some(corrected.as_str())
    );
}

#[test]
fn ambiguous_semantic_site_refuses_with_ordered_candidates() {
    let outcome =
        migrate::refuse_ambiguous_site("legacy-log.emath", "log(x)", &["ln(x)", "log10(x)"]);
    assert!(outcome.rewritten_source.is_none());
    let [refusal] = outcome.receipt.refusals.as_slice() else {
        panic!("ambiguous site must produce one refusal");
    };
    assert_eq!(refusal.code, migrate::E_MIG_AMBIGUOUS_SITE);
    assert_eq!(refusal.candidates, ["ln(x)", "log10(x)"]);
    let json = outcome.receipt.to_canonical_json();
    assert!(json.contains("\"code\":\"E-MIG-AMBIGUOUS-SITE\""));
    assert!(json.contains("\"candidates\":[\"ln(x)\",\"log10(x)\"]"));
}

#[test]
fn every_shipped_rule_has_an_explicit_proof_class() {
    let rules = migrate::registered_rules();
    assert!(
        rules
            .iter()
            .any(|rule| rule.kind == migrate::RuleKind::Semantic),
        "the registry must ship a real semantic migration rule"
    );
    assert!(
        migrate::validate_rule_registry(rules).is_ok(),
        "unclassified, duplicate, or malformed rules must refuse to ship"
    );
}
