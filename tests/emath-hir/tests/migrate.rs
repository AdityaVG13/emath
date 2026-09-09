//! Bootstrap migration tests.

use emath_core::Span;
use emath_core::tree::{Declaration, Section, Stmt, StmtKind, Suite};
use emath_hir::migrate_declaration;
use emath_test_harness::Probe;

fn decl_with(names: &[&str]) -> Declaration {
    Declaration { name: "Greeter".into(), generics: Vec::new(), item_kind: "custom".into(), as_kind: "function".into(), attributes: Vec::new(), body: names.iter().map(|name| Stmt { kind: StmtKind::Section(Section { name: (*name).into(), generic: None, args: None, suite: Suite::default(), source: Span::default(), head_source: Span::default() }), source: Span::default() }).collect(), signature: None, source: Span::default(), head_source: Span::default() }
}

#[test]
fn bootstrap_migration() {
    let mut p = Probe::new("singular request migrates to goals with E-MIGR-002");
    let migrated = migrate_declaration(&decl_with(&["request"]), "");
    p.eq("sections", migrated.sections, vec!["goals".to_string()]);
    p.demand(
        "code",
        migrated.issues.iter().any(|i| i.code == "E-MIGR-002" && i.detail.contains("`request:` moved to `goals:`")),
        "migrator must emit E-MIGR-002 for request -> goals",
    );
    p.finish();
}
