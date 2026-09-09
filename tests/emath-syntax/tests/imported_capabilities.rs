//! Imported `emath capability` schema, without a parser fork.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::{parse_str};

use emath_test_harness::{Probe, boot};

#[test]
fn imported_capabilities() {
    boot();
    let mut probe = Probe::new("Imported `emath capability` schema, without a parser fork.");
    probe.case("imported_capability_schema_admits", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/imported-capabilities.emath");
    let (tree, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let declaration = tree
        .items
        .iter()
        .find_map(|item| match item {
            emath_core::tree::Item::Declaration(declaration) if declaration.name == "Softmax" => {
                Some(declaration)
            }
            _ => None,
        })
        .expect("Softmax declaration");
    p.demand("2", (declaration.item_kind) == ("custom"), format!("expected {:?}, got {:?}", ("custom"), (declaration.item_kind)));
    if p.failures().len() != f0 { return; }
    p.demand("3", (declaration.as_kind) == ("capability"), format!("expected {:?}, got {:?}", ("capability"), (declaration.as_kind)));
    if p.failures().len() != f0 { return; }

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("capability", source);
    p.demand("4",!checked.diagnostics.has_errors(), format!(
        "imported capability schema must admit: {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("5", (checked.package.declarations[0].kind_label) == ("capability"), format!("expected {:?}, got {:?}", ("capability"), (checked.package.declarations[0].kind_label)));
    if p.failures().len() != f0 { return; }
    let first_id = checked
        .package
        .meaning_id(&[])
        .expect("capability MeaningID");
    let second_id = session
        .check_owned("capability-repeat", source)
        .package
        .meaning_id(&[])
        .expect("repeat capability MeaningID");
    p.eq("6", &first_id, &second_id);

    });
    probe.case("unknown_capability_section_refuses", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/imported_capabilities.emath");
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("capability-invalid", source);
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYN-101"), stringify!(checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYN-101")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
