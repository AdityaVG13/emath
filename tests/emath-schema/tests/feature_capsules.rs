use std::str::FromStr;

use emath_core::SemanticHash;
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureClass, Maturity};
use emath_schema::{
    CLASS_RULES, capsule_semantic_hash, parse_feature_capsule, validate_maturity_transition,
};
use emath_term::Term;

/// The authored authority capsule for exact add: the first executable
/// reference body in the tree.
fn exact_add_text() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../language/spec/capabilities/core/add.emath"
    ))
    .unwrap()
}

#[test]
fn exact_add_capsule_carries_executable_reference_body() {
    let text = exact_add_text();
    let (capsule, issues) = parse_feature_capsule(&text);
    assert!(issues.is_empty(), "exact add capsule refuses: {issues:?}");
    let capsule = capsule.unwrap();

    let body = capsule.slots.get("reference_body").expect("body slot");
    let params = capsule.slots.get("reference_params").expect("params slot");
    let signature = capsule
        .slots
        .get("reference_signature")
        .expect("signature slot");
    let (CapsuleSlot::Value(term_text), CapsuleSlot::Value(params_text)) = (body, params) else {
        panic!("executable reference body must be concrete data");
    };
    let CapsuleSlot::Value(signature_text) = signature else {
        panic!("executable reference signature must be concrete data");
    };
    assert_eq!(params_text, "lhs,rhs");
    assert_eq!(signature_text, "add=2");

    // The body is executable data for the generic term compiler: it parses
    // as a canonical first-order term and round trips byte-exactly.
    let term = Term::parse_canonical(term_text).expect("canonical term");
    assert_eq!(term.canonical(), *term_text);

    // Canonical bytes are stable across parses.
    let first = capsule.canonical_bytes();
    let second = parse_feature_capsule(&text).0.unwrap().canonical_bytes();
    assert_eq!(first, second);

    // The semantic hash binds the body.
    assert_eq!(capsule.semantic_hash, capsule_semantic_hash(&text).unwrap());

    // An executable body stays an authored reference.
    assert_eq!(
        capsule.slots.get("reference"),
        Some(&CapsuleSlot::Value("authored".to_string()))
    );
}

#[test]
fn reference_body_participates_in_semantic_hash() {
    let text = exact_add_text();
    let swapped = text.replace("var(lhs),var(rhs)", "var(rhs),var(lhs)");
    assert_ne!(
        capsule_semantic_hash(&text).unwrap(),
        capsule_semantic_hash(&swapped).unwrap()
    );
    let (_, issues) = parse_feature_capsule(&swapped);
    assert!(
        issues.iter().any(|issue| issue.code == "E-CAPSULE-022"),
        "mutated body must refuse its stale declared hash: {issues:?}"
    );
}

#[test]
fn executable_reference_refusals_are_typed_and_stable() {
    let text = exact_add_text();
    let expect_code = |mutated: &str, code: &str| {
        let (_, issues) = parse_feature_capsule(mutated);
        assert!(
            issues.iter().any(|issue| issue.code == code),
            "missing {code}: {issues:?}"
        );
    };
    // Malformed canonical term text.
    let broken = text.replace(
        "apply(add,var(lhs),var(rhs))",
        "apply(add,var(lhs),var(rhs)",
    );
    expect_code(&broken, "E-CAPSULE-023");
    // Partial body: signature missing.
    let partial = text.replace("    reference_signature: \"add=2\"\n", "");
    expect_code(&partial, "E-CAPSULE-023");
    // Term violates the declared arity.
    let arity = text.replace("reference_signature: \"add=2\"", "reference_signature: \"add=3\"");
    expect_code(&arity, "E-CAPSULE-024");
    // Conflicting arity declarations for one symbol.
    let conflicting = text.replace(
        "reference_signature: \"add=2\"",
        "reference_signature: \"add=2,add=3\"",
    );
    expect_code(&conflicting, "E-CAPSULE-024");
    // Free variable outside the declared parameter list.
    let free = text.replace("reference_params: \"lhs,rhs\"", "reference_params: \"rhs\"");
    expect_code(&free, "E-CAPSULE-025");
    // Executable body with a non-authored reference mode.
    let mode = text.replace("reference: \"authored\"", "reference: \"provider\"");
    expect_code(&mode, "E-CAPSULE-026");
}

fn seed(class: FeatureClass, ordinal: usize) -> String {
    let id = format!("seed.{}.feature_{ordinal}", class.as_str());
    let reference = match class {
        FeatureClass::Method | FeatureClass::Provider => "provider",
        FeatureClass::Constitution
        | FeatureClass::Theory
        | FeatureClass::Diagnostic
        | FeatureClass::Migration => "authored",
        _ => "authored",
    };
    let body = format!(
        "emath feature Seed{ordinal}:\n\
         schema: {FEATURE_CAPSULE_SCHEMA}\n\
         feature_id: {id}\n\
         class: {}\n\
         maturity: proposed\n\
         summary: seed {ordinal}\n\
         source: catalog.seed-{ordinal}\n\
         surface: n/a(class-rule | no source spelling in seed)\n\
         semantics: seed semantics {ordinal}\n\
         exactness: exact\n\
         effects: pure\n\
         worlds: n/a(class-rule | no world restriction)\n\
         providers: n/a(class-rule | local reference)\n\
         artifacts: value\n\
         reference: {reference}\n\
         conformance: positive,negative,mutation\n\
         migration: n/a(initial | no prior meaning)\n\
         authority_target: capsule-candidate\n\
         presentation: seed\n\
         agent: owner=language;checks=feature_capsules\n\
         edge: depends_on -> std.constitution.language\n\
         projection: semantics -> required\n",
        class.as_str()
    );
    let hash = capsule_semantic_hash(&body).unwrap();
    body.replace(
        "maturity: proposed",
        &format!("semantic_hash: {hash}\nmaturity: proposed"),
    )
}

#[test]
fn all_twenty_class_rules_cover_fifty_six_canonical_seed_capsules() {
    assert_eq!(CLASS_RULES.len(), 20);
    let mut canonical = Vec::new();
    for ordinal in 0..56 {
        let class = FeatureClass::ALL[ordinal % FeatureClass::ALL.len()];
        let text = seed(class, ordinal);
        let (capsule, issues) = parse_feature_capsule(&text);
        assert!(issues.is_empty(), "{class:?} seed {ordinal}: {issues:?}");
        let capsule = capsule.unwrap();
        assert_eq!(capsule.feature_id.class(), class.as_str());
        let first = capsule.canonical_bytes();
        let second = parse_feature_capsule(&text).0.unwrap().canonical_bytes();
        assert_eq!(first, second);
        canonical.push(first);
    }
    assert_eq!(canonical.len(), 56);
}

#[test]
fn capsule_boundaries_refuse_untyped_or_falsely_live_data() {
    let valid = seed(FeatureClass::Capability, 60);
    for (needle, replacement, code) in [
        (
            "schema: emath.feature-capsule",
            "schema: emath.feature-capsule.v2",
            "E-CAPSULE-005",
        ),
        ("class: capability", "class: theory", "E-CAPSULE-014"),
        (
            "conformance: positive,negative,mutation\n",
            "",
            "E-CAPSULE-004",
        ),
        ("exactness: exact", "exactness: n/a", "E-CAPSULE-010"),
        ("edge: depends_on", "edge: resembles", "E-CAPSULE-011"),
    ] {
        let (_, issues) = parse_feature_capsule(&valid.replace(needle, replacement));
        assert!(
            issues.iter().any(|issue| issue.code == code),
            "missing {code}: {issues:?}"
        );
    }

    let cataloged_live = valid
        .replace("maturity: proposed", "maturity: cataloged")
        .replace(
            "projection: semantics -> required",
            "projection: semantics -> provided",
        );
    let (_, issues) = parse_feature_capsule(&cataloged_live);
    assert!(issues.iter().any(|issue| issue.code == "E-CAPSULE-016"));

    let stable_hole = valid
        .replace("maturity: proposed", "maturity: stable")
        .replace(
            "semantics: seed semantics 60",
            "semantics: hole(semantics | open law)",
        );
    let (_, issues) = parse_feature_capsule(&stable_hole);
    assert!(issues.iter().any(|issue| issue.code == "E-CAPSULE-017"));
}

#[test]
fn maturity_transitions_are_explicit_and_bounded() {
    for pair in Maturity::ALL.windows(2) {
        validate_maturity_transition(pair[0], pair[1]).unwrap();
    }
    validate_maturity_transition(Maturity::Deprecated, Maturity::Stable).unwrap();
    validate_maturity_transition(Maturity::Retired, Maturity::Deprecated).unwrap();
    assert_eq!(
        validate_maturity_transition(Maturity::Cataloged, Maturity::Stable)
            .unwrap_err()
            .code,
        "E-CAPSULE-020"
    );
}

#[test]
fn semantic_hash_excludes_presentation_and_rejects_domain_substitution() {
    let first = seed(FeatureClass::Capability, 61);
    let changed = first.replace("presentation: seed", "presentation: infix plus");
    assert_eq!(
        capsule_semantic_hash(&first).unwrap(),
        capsule_semantic_hash(&changed).unwrap()
    );
    let (capsule, issues) = parse_feature_capsule(&first);
    assert!(issues.is_empty());
    assert_eq!(
        capsule.unwrap().semantic_hash,
        capsule_semantic_hash(&first).unwrap()
    );
    assert!(
        SemanticHash::from_str(&format!(
            "distribution-{}",
            capsule_semantic_hash(&first).unwrap()
        ))
        .is_err()
    );

    let two_edges = format!("{first}edge: uses -> std.type.int\n");
    let changed_second = two_edges.replace(
        "edge: uses -> std.type.int",
        "edge: uses -> std.type.float64",
    );
    assert_ne!(
        capsule_semantic_hash(&two_edges).unwrap(),
        capsule_semantic_hash(&changed_second).unwrap()
    );
    let duplicate_operational = format!("{two_edges}path: first\npath: second\n");
    assert_eq!(
        capsule_semantic_hash(&duplicate_operational)
            .unwrap_err()
            .code,
        "E-CAPSULE-021"
    );
}
