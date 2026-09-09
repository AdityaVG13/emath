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
use emath_test_harness::Probe;

fn legacy_declaration() -> Declaration {
    Declaration { name: "Observable".into(), generics: Vec::new(), item_kind: "custom".into(), as_kind: "function".into(), attributes: Vec::new(), body: vec![Stmt { kind: StmtKind::Section(Section { name: "request".into(), generic: None, args: None, suite: Suite::default(), source: Span::default(), head_source: Span::default() }), source: Span::default() }], signature: None, source: Span::default(), head_source: Span::default() }
}

#[test]
fn custom_kind_ecosystem() {
    let mut p = Probe::new("custom kinds lower, pin, resolve, and migrate end to end");
    p.case("schema-parses", |p| {
        let (schema, issues) = parse_schema_language("kind observable\nsection observations at-most-one fields\npredicate observations != empty\n");
        p.demand("no-issues", issues.is_empty(), format!("custom schema must parse: {issues:?}"));
        p.eq("name", schema.name(), "observable");
        p.demand("section", schema.section("observations").is_some(), "observations section present");
    });
    p.case("lowering-deterministic", |p| {
        let program = [LowerOp::Hoist { from: "observations".into(), into: "definitions".into() }, LowerOp::Bind { section: "definitions".into(), to: "result".into() }];
        let first = apply_lowering(&KindSchema::core_function(), &program).expect("lowering admitted");
        let second = apply_lowering(&KindSchema::core_function(), &program).expect("repeat admitted");
        p.eq("identity", first.identity, second.identity);
        p.eq("trace", first.trace, second.trace);
    });
    p.case("registry-resolves-and-locks", |p| {
        let record = PackageVersion { version: "1.0.0".into(), content_id: "emath:pack:v1:test".into(), source_location: "registry://example/observable/1.0.0".into(), kind_schemas: vec!["observable".into()], provider_descriptors: vec!["evaluate.observable".into()], yanked: false, revoked: false, license: "Apache-2.0".into(), security_notes: Vec::new(), evidence_summary: "schema+provider conformance".into(), artifact_link: None };
        let mut snapshot = IndexSnapshot::new();
        snapshot.packages.entry("example.observable".into()).or_default().insert(record.version.clone(), record);
        let resolved = snapshot.resolve("example.observable", Constraint::Major(1)).expect("package resolves");
        p.demand("kind", check_kind_schema(resolved, "observable").is_ok(), "kind schema served");
        p.demand("provider", check_provider_capability(resolved, "evaluate.observable").is_ok(), "provider served");
        let lock = RegistryLock::from_pins(&snapshot, BTreeMap::from([("example.observable".into(), "1.0.0".into())]));
        p.demand("lock", lock.verify(&snapshot).is_ok(), "offline lock reproduces");
    });
    p.case("legacy-migrates", |p| {
        let migrated = migrate_declaration(&legacy_declaration(), "1");
        p.eq("sections", migrated.sections, vec!["goals".to_string()]);
        p.demand("code", migrated.issues.iter().any(|i| i.code == "E-MIGR-002"), "legacy request migrates with E-MIGR-002");
    });
    p.case("forbidden-lowering-refused", |p| {
        let error = apply_lowering(&KindSchema::core_function(), &[LowerOp::Rename { from: "inputs".into(), to: "outputs".into() }]).expect_err("lowering must not overwrite a core section");
        p.eq("code", error[0].code.clone(), "E-KIND-021");
    });
    p.finish();
}
