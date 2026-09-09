//! Open-declaration admission tests.

use emath_core::tree::{Declaration, Section, Stmt, StmtKind, Suite};
use emath_core::{Diagnostics, Span};
use emath_hir::{OpenDecl, SectionFamily, SectionManifest, SectionViolationReason};
use emath_ir::kind_schema::KindSchema;
use emath_test_harness::Probe;

fn decl_with_section(name: &str) -> Declaration {
    Declaration { name: "Greeter".into(), generics: Vec::new(), item_kind: "custom".into(), as_kind: "function".into(), attributes: Vec::new(), body: vec![Stmt { kind: StmtKind::Section(Section { name: name.into(), generic: None, args: None, suite: Suite::default(), source: Span::default(), head_source: Span::default() }), source: Span::default() }], signature: None, source: Span::default(), head_source: Span::default() }
}

fn check(sections: &[&str]) -> (Vec<(String, SectionViolationReason, String)>, bool) {
    let schema = KindSchema::core_function();
    let manifest = SectionManifest::new(schema, sections.iter().map(|s| (*s).into()).collect());
    let mut diagnostics = Diagnostics::new();
    let violations = manifest.check(&mut diagnostics);
    let rows = violations.into_iter().map(|v| (v.code.to_string(), v.reason, v.detail)).collect();
    let has_010 = diagnostics.items().iter().any(|d| d.code == "E-KIND-010");
    (rows, has_010)
}

#[test]
fn open_declaration_admission() {
    let mut p = Probe::new("unknown sections are E-KIND-016, missing required are E-KIND-011");
    p.case("unknown-is-016", |p| {
        let (rows, has_010) = check(&["inputs", "outputs", "definitions", "not_a_section"]);
        p.eq("count", rows.len(), 1);
        p.eq("code", rows[0].0.clone(), "E-KIND-016".to_string());
        p.demand("reason", matches!(rows[0].1, SectionViolationReason::UnknownSection), "unknown section");
        p.demand("no-010", !has_010, "E-KIND-010 stays the sema predicate");
    });
    p.case("missing-is-011", |p| {
        let (rows, _) = check(&["inputs"]);
        p.demand("code", rows.iter().any(|v| v.0 == "E-KIND-011" && v.1 == SectionViolationReason::MissingRequired), "missing required is E-KIND-011");
    });
    p.case("requests-hints-goals", |p| {
        let (rows, _) = check(&["inputs", "definitions", "requests"]);
        let refused = rows.iter().find(|v| v.2.contains("requests")).expect("requests must be a typed refusal");
        p.eq("code", refused.0.clone(), "E-KIND-016".to_string());
        p.demand("reason", matches!(refused.1, SectionViolationReason::UnknownSection), "unknown section");
        p.contains("hint", &refused.2, "goals:");
        let open = OpenDecl::from_bootstrap_declaration(&decl_with_section("requests"));
        let family = open.section("requests").expect("section collected").family;
        p.ne("not-goals", family, SectionFamily::Goals);
        p.eq("extension", family, SectionFamily::Extension);
    });
    p.finish();
}
