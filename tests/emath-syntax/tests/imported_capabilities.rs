//! Imported `emath capability` schema, without a parser fork.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

#[test]
fn imported_capability_schema_admits() {
    let source = include_str!("../../../tests/fixtures/language/intro/imported-capabilities.emath");
    let (tree, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());
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
    assert_eq!(declaration.item_kind, "custom");
    assert_eq!(declaration.as_kind, "capability");

    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("capability", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "imported capability schema must admit: {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(checked.package.declarations[0].kind_label, "capability");
    let first_id = checked
        .package
        .meaning_id(&[])
        .expect("capability MeaningID");
    let second_id = session
        .check_owned("capability-repeat", source)
        .package
        .meaning_id(&[])
        .expect("repeat capability MeaningID");
    assert_eq!(first_id, second_id);
}

#[test]
fn unknown_capability_section_refuses() {
    let source = include_str!("../../../tests/invalid/imported_capabilities.emath");
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("capability-invalid", source);
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYN-101")
    );
}
