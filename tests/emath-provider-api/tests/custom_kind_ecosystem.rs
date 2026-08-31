//! Integrated custom-kind ecosystem acceptance for emath-h4pl.

use std::collections::BTreeMap;

use emath_core::Span;
use emath_core::tree::{Declaration, Section, Stmt, StmtKind, Suite};
use emath_hir::migrate_declaration;
use emath_ir::KindSchema;
use emath_registry::{
    Constraint, IndexSnapshot, PackageVersion, RegistryLock, check_kind_schema,
    check_provider_capability,
};
use emath_schema::{LowerOp, apply_lowering, parse_schema_language};

#[test]
fn custom_kind_registry_and_migration_succeed() {
    let (schema, issues) = parse_schema_language(
        "kind observable\nsection observations at-most-one fields\npredicate observations != empty\n",
    );
    assert!(issues.is_empty(), "custom schema must parse: {issues:?}");
    assert_eq!(schema.name(), "observable");
    assert!(schema.section("observations").is_some());

    let program = [
        LowerOp::Hoist {
            from: "observations".into(),
            into: "definitions".into(),
        },
        LowerOp::Bind {
            section: "definitions".into(),
            to: "result".into(),
        },
    ];
    let first = apply_lowering(&KindSchema::core_function(), &program).expect("lowering admitted");
    let second = apply_lowering(&KindSchema::core_function(), &program).expect("repeat admitted");
    assert_eq!(first.identity, second.identity);
    assert_eq!(first.trace, second.trace);

    let record = PackageVersion {
        version: "1.0.0".into(),
        content_id: "emath:pack:v1:test".into(),
        source_location: "registry://example/observable/1.0.0".into(),
        kind_schemas: vec!["observable".into()],
        provider_descriptors: vec!["evaluate.observable".into()],
        yanked: false,
        revoked: false,
        license: "Apache-2.0".into(),
        security_notes: Vec::new(),
        evidence_summary: "schema+provider conformance".into(),
        artifact_link: None,
    };
    let mut snapshot = IndexSnapshot::new();
    snapshot
        .packages
        .entry("example.observable".into())
        .or_default()
        .insert(record.version.clone(), record);
    let resolved = snapshot
        .resolve("example.observable", Constraint::Major(1))
        .expect("package resolves");
    check_kind_schema(resolved, "observable").expect("kind schema served");
    check_provider_capability(resolved, "evaluate.observable").expect("provider served");
    let lock = RegistryLock::from_pins(
        &snapshot,
        BTreeMap::from([("example.observable".into(), "1.0.0".into())]),
    );
    lock.verify(&snapshot).expect("offline lock reproduces");

    let migrated = migrate_declaration(&legacy_declaration(), "1");
    assert_eq!(migrated.sections, ["goals"]);
    assert!(
        migrated
            .issues
            .iter()
            .any(|issue| issue.code == "E-MIGR-002")
    );
}

#[test]
fn forbidden_custom_kind_lowering_is_refused() {
    let error = apply_lowering(
        &KindSchema::core_function(),
        &[LowerOp::Rename {
            from: "inputs".into(),
            to: "outputs".into(),
        }],
    )
    .expect_err("lowering must not overwrite a core section");
    assert_eq!(error[0].code, "E-KIND-021");
}

fn legacy_declaration() -> Declaration {
    Declaration {
        name: "Observable".into(),
        generics: Vec::new(),
        item_kind: "custom".into(),
        as_kind: "function".into(),
        attributes: Vec::new(),
        body: vec![Stmt {
            kind: StmtKind::Section(Section {
                name: "request".into(),
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
