//! Open-declaration admission tests, moved from `crates/emath-hir/src/open.rs`.

use emath_core::tree::{Declaration, Section, Stmt, StmtKind, Suite};
use emath_core::{Diagnostics, Span};
use emath_hir::{OpenDecl, SectionFamily, SectionManifest, SectionViolationReason};
use emath_ir::kind_schema::KindSchema;

/// Unknown sections are schema violations of the HIR manifest
/// (`E-KIND-016`), not the sema function-constructors refusal
/// (`E-KIND-010`): one code, one predicate.
#[test]
fn unknown_section_emits_ekind016_not_ekind010() {
    let schema = KindSchema::core_function();
    let manifest = SectionManifest::new(
        schema,
        vec![
            "inputs".into(),
            "outputs".into(),
            "definitions".into(),
            "not_a_section".into(),
        ],
    );
    let mut diagnostics = Diagnostics::new();
    let violations = manifest.check(&mut diagnostics);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E-KIND-016");
    assert!(matches!(
        violations[0].reason,
        SectionViolationReason::UnknownSection
    ));
    assert!(
        diagnostics.items().iter().all(|d| d.code != "E-KIND-010"),
        "E-KIND-010 must stay the sema function-constructors predicate"
    );
}

/// Missing required sections still emit `E-KIND-011` after the
/// unknown-section split.
#[test]
fn missing_required_emits_ekind011() {
    let schema = KindSchema::core_function();
    let manifest = SectionManifest::new(schema, vec!["inputs".into()]);
    let mut diagnostics = Diagnostics::new();
    let violations = manifest.check(&mut diagnostics);
    assert!(
        violations
            .iter()
            .any(|v| v.code == "E-KIND-011" && v.reason == SectionViolationReason::MissingRequired)
    );
}

/// `requests:` is not a Goals-family alias. The kind schema does not
/// list it, so admission refuses with `E-KIND-016` and a `goals:` hint.
#[test]
fn requests_section_is_refused_with_goals_migration_hint() {
    let schema = KindSchema::core_function();
    let manifest = SectionManifest::new(
        schema,
        vec!["inputs".into(), "definitions".into(), "requests".into()],
    );
    let mut diagnostics = Diagnostics::new();
    let violations = manifest.check(&mut diagnostics);
    let refused = violations
        .iter()
        .find(|v| v.detail.contains("requests"))
        .expect("`requests:` must be a typed refusal");
    assert_eq!(refused.code, "E-KIND-016");
    assert!(matches!(
        refused.reason,
        SectionViolationReason::UnknownSection
    ));
    assert!(
        refused.detail.contains("goals:"),
        "refusal must hint the `goals:` spelling, got {}",
        refused.detail
    );

    let open = OpenDecl::from_bootstrap_declaration(&decl_with_section("requests"));
    let family = open
        .section("requests")
        .expect("section is collected")
        .family;
    assert_ne!(
        family,
        SectionFamily::Goals,
        "`requests` must not classify as the Goals family"
    );
    assert_eq!(family, SectionFamily::Extension);
}

fn decl_with_section(name: &str) -> Declaration {
    Declaration {
        name: "Greeter".into(),
        generics: Vec::new(),
        item_kind: "custom".into(),
        as_kind: "function".into(),
        attributes: Vec::new(),
        body: vec![Stmt {
            kind: StmtKind::Section(Section {
                name: name.into(),
                generic: None,
                args: None,
                suite: Suite::default(),
                source: Span::default(),
                head_source: Span::default(),
            }),
            source: Span::default(),
        }],
        signature: None,
        source: Span::default(),
        head_source: Span::default(),
    }
}
