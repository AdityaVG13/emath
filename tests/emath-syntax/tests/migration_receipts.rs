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

use emath_test_harness::{Probe, boot};

#[test]
fn migration_receipts() {
    boot();
    let mut probe = Probe::new("Migrate receipt contract core ( 05 §5) — library thin slice. The `emath migrate` CLI subcommand is DEFERRED (the CLI dispatch files were under active");
    probe.case("canonical_source_is_idempotent_no_op", |p| {
    let f0 = p.failures().len();

    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        CANONICAL,
        CANONICAL,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    p.demand("1",outcome.receipt.rules_applied.is_empty(), format!(
        "second-run idempotence: a canonical source applies no rules; got: {:#?}",
        outcome.receipt.rules_applied
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2", (outcome.receipt.verdict) == ("complete"), format!("expected {:?}, got {:?}", ("complete"), (outcome.receipt.verdict)));
    if p.failures().len() != f0 { return; }
    p.demand("3",outcome.receipt.identity_verified, stringify!(outcome.receipt.identity_verified));
    if p.failures().len() != f0 { return; }
    p.demand("4",outcome.rewritten_source.is_none(), stringify!(outcome.rewritten_source.is_none()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("noncanonical_source_respells_with_identity_verified", |p| {
    let f0 = p.failures().len();

    let rewritten = canonical_format(NON_CANONICAL);
    p.demand("1", rewritten != NON_CANONICAL, format!("must differ from {NON_CANONICAL:?}"));
    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        NON_CANONICAL,
        &rewritten,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    p.demand("2",outcome.receipt.refusals.is_empty(), format!(
        "a verified respell must not refuse; got: {:#?}",
        outcome.receipt.refusals
    ));
    if p.failures().len() != f0 { return; }
    p.eq("3", outcome.receipt.rules_applied.len(), 1);
    let rule = &outcome.receipt.rules_applied[0];
    p.eq("4", rule.kind, migrate::RuleKind::Respell);
    p.demand("5", (rule.identity_delta) == ("none"), format!("expected {:?}, got {:?}", ("none"), (rule.identity_delta)));
    if p.failures().len() != f0 { return; }
    p.eq("6", rule.before_hash.clone(), rule.after_hash.clone());
    p.demand("7",outcome.receipt.identity_verified, stringify!(outcome.receipt.identity_verified));
    if p.failures().len() != f0 { return; }
    p.eq("8", outcome.rewritten_source.as_deref(), Some(rewritten.as_str()));

    });
    probe.case("identity_breaking_rewrite_refuses_and_emits_nothing", |p| {
    let f0 = p.failures().len();

    // A rewrite that changes meaning (the load-bearing property): the
    // verify gate must refuse and emit NOTHING.
    let outcome = migrate::migrate_verified_rewrite(
        "plain.emath",
        CANONICAL,
        "emath function plain:\n    inputs:\n        x: Float64\n\n    outputs:\n        y: Float64\n\n    definitions:\n        y = x * 3.0\n",
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    p.demand("1",matches!(
            &outcome.receipt.refusals[..],
            [r] if r.code == migrate::E_MIG_VERIFY_FAIL
        ), format!(
        "an identity-breaking rewrite must refuse E-MIG-VERIFY-FAIL; got: {:#?}",
        outcome.receipt.refusals
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",outcome.rewritten_source.is_none(), stringify!(outcome.rewritten_source.is_none()));
    if p.failures().len() != f0 { return; }
    p.demand("3", (outcome.receipt.verdict) == ("partial-refused"), format!("expected {:?}, got {:?}", ("partial-refused"), (outcome.receipt.verdict)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("refusing_source_is_named_refusal_not_rewrite", |p| {
    let f0 = p.failures().len();

    let outcome = migrate::migrate_verified_rewrite(
        "broken.emath",
        REFUSING_SOURCE,
        REFUSING_SOURCE,
        migrate::RULE_CANONICAL_FORMAT.id,
    );
    p.demand("1",outcome
            .receipt
            .refusals
            .iter()
            .any(|r| r.code == migrate::E_MIG_SOURCE_REFUSES), format!(
        "a source that does not admit refuses E-MIG-SOURCE-REFUSES; got: {:#?}",
        outcome.receipt.refusals
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",outcome.rewritten_source.is_none(), stringify!(outcome.rewritten_source.is_none()));
    if p.failures().len() != f0 { return; }
    p.demand("3", (outcome.receipt.verdict) == ("partial-refused"), format!("expected {:?}, got {:?}", ("partial-refused"), (outcome.receipt.verdict)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("receipt_json_is_canonical_and_replay_is_byte_identical", |p| {
    let f0 = p.failures().len();

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
    p.eq("1", first.receipt.to_canonical_json(), second.receipt.to_canonical_json());
    let json = first.receipt.to_canonical_json();
    p.demand("2",json.starts_with("{\"schema\":\"emath.migration-receipt v1\""), format!(
        "receipt JSON must lead with the versioned schema key; got: {json}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",json.contains("\"identity_verified\":true"), stringify!(json.contains("\"identity_verified\":true")));
    if p.failures().len() != f0 { return; }
    p.demand("4",json.contains("\"kind\":\"respell\""), stringify!(json.contains("\"kind\":\"respell\"")));
    if p.failures().len() != f0 { return; }
    p.demand("5",json.contains("\"identity_delta\":\"none\""), stringify!(json.contains("\"identity_delta\":\"none\"")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("semantic_rule_records_checked_before_and_after_meaning_ids", |p| {
    let f0 = p.failures().len();

    let corrected = CANONICAL.replace("x * 2.0", "x * 3.0");
    let outcome = migrate::migrate_semantic_rewrite(
        "plain.emath",
        CANONICAL,
        &corrected,
        migrate::RULE_SEMANTIC_CORRECTION.id,
    );
    p.demand("1",outcome.receipt.refusals.is_empty(), format!(
        "a registered semantic correction must admit both meanings: {:#?}",
        outcome.receipt.refusals
    ));
    if p.failures().len() != f0 { return; }
    let [rule] = outcome.receipt.rules_applied.as_slice() else {
        panic!("semantic correction must apply exactly one rule");
    };
    p.eq("2", rule.kind, migrate::RuleKind::Semantic);
    p.ne("3", rule.before_hash.clone(), rule.after_hash.clone());
    p.eq("4", rule.identity_delta.clone(), format!("{} -> {}", rule.before_hash, rule.after_hash));
    p.eq("5", outcome.rewritten_source.as_deref(), Some(corrected.as_str()));

    });
    probe.case("ambiguous_semantic_site_refuses_with_ordered_candidates", |p| {
    let f0 = p.failures().len();

    let outcome =
        migrate::refuse_ambiguous_site("legacy-log.emath", "log(x)", &["ln(x)", "log10(x)"]);
    p.demand("1",outcome.rewritten_source.is_none(), stringify!(outcome.rewritten_source.is_none()));
    if p.failures().len() != f0 { return; }
    let [refusal] = outcome.receipt.refusals.as_slice() else {
        panic!("ambiguous site must produce one refusal");
    };
    p.eq("2", refusal.code, migrate::E_MIG_AMBIGUOUS_SITE);
    p.demand("3", (refusal.candidates) == (["ln(x)", "log10(x)"]), format!("expected {:?}, got {:?}", (["ln(x)", "log10(x)"]), (refusal.candidates)));
    if p.failures().len() != f0 { return; }
    let json = outcome.receipt.to_canonical_json();
    p.demand("4",json.contains("\"code\":\"E-MIG-AMBIGUOUS-SITE\""), stringify!(json.contains("\"code\":\"E-MIG-AMBIGUOUS-SITE\"")));
    if p.failures().len() != f0 { return; }
    p.demand("5",json.contains("\"candidates\":[\"ln(x)\",\"log10(x)\"]"), stringify!(json.contains("\"candidates\":[\"ln(x)\",\"log10(x)\"]")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("every_shipped_rule_has_an_explicit_proof_class", |p| {
    let f0 = p.failures().len();

    let rules = migrate::registered_rules();
    p.demand("1",rules
            .iter()
            .any(|rule| rule.kind == migrate::RuleKind::Semantic), format!(
        "the registry must ship a real semantic migration rule"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",migrate::validate_rule_registry(rules).is_ok(), format!(
        "unclassified, duplicate, or malformed rules must refuse to ship"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
