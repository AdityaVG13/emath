//! Bootstrap migration tests, moved from `crates/emath-hir/src/migrate.rs`.

use emath_core::Span;
use emath_core::tree::{Declaration, Section, Stmt, StmtKind, Suite};
use emath_hir::migrate_declaration;

fn decl_with(names: &[&str]) -> Declaration {
    Declaration {
        name: "Greeter".into(),
        generics: Vec::new(),
        item_kind: "custom".into(),
        as_kind: "function".into(),
        attributes: Vec::new(),
        body: names
            .iter()
            .map(|name| Stmt {
                kind: StmtKind::Section(Section {
                    name: (*name).into(),
                    generic: None,
                    args: None,
                    suite: Suite::default(),
                    source: Span::default(),
                    head_source: Span::default(),
                }),
                source: Span::default(),
            })
            .collect(),
        signature: None,
        source: Span::default(),
        head_source: Span::default(),
    }
}

#[test]
fn migrate_maps_singular_request_to_goals() {
    let migrated = migrate_declaration(&decl_with(&["request"]), "");
    assert_eq!(migrated.sections, vec!["goals".to_string()]);
    assert!(
        migrated.issues.iter().any(|issue| {
            issue.code == "E-MIGR-002" && issue.detail.contains("`request:` moved to `goals:`")
        }),
        "migrator must emit E-MIGR-002 for `request:` → `goals:`, got {:?}",
        migrated.issues
    );
}
