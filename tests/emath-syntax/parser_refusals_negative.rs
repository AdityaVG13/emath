#![forbid(unsafe_code)]
//! Negative parser witnesses: constructs the Phase 1 strict subset does not
//! implement must be refused with a stable diagnostic instead of being
//! parsed into lossy trees (fn types as Path(["fn"]), type aliases with
//! the RHS dropped, generic extern operator parameters discarded, broken
//! section argument lists stored as args: None).

use emath_syntax::parse_str;

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    let found = diagnostics.errors().any(|e| e.code == code);
    found
}

#[test]
fn fn_type_in_field_is_refused_with_e_type_110() {
    let source = include_str!("fixtures/fn_type.emath");
    assert!(
        has_error(source, "E-TYPE-110"),
        "fn type must be refused, not parsed into a lossy Path"
    );
}

#[test]
fn type_alias_is_refused_with_e_type_111() {
    let source = include_str!("fixtures/type_alias.emath");
    assert!(
        has_error(source, "E-TYPE-111"),
        "type alias RHS must not be silently dropped"
    );
}

#[test]
fn generic_extern_operator_is_refused_with_e_type_112() {
    let source = include_str!("fixtures/extern_op.emath");
    assert!(
        has_error(source, "E-TYPE-112"),
        "generic extern operator must be refused, not have generics dropped"
    );
}

#[test]
fn broken_section_argument_list_is_refused_with_e_syn_101() {
    let source = include_str!("fixtures/broken_args.emath");
    assert!(
        has_error(source, "E-SYN-101"),
        "malformed argument list must refuse the statement, not record args: None"
    );
}

#[test]
fn plain_doc_without_refused_constructs_parses_clean() {
    let source = include_str!("fixtures/baseline.emath");
    let (_, diagnostics) = parse_str(source);
    assert!(
        !diagnostics.has_errors(),
        "baseline doc must parse without errors"
    );
}
