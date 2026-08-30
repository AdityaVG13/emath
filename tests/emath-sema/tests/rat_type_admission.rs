//! Pass 2 (emath-rat-real-types-p5cj): `Rat` / `Rational` at type sites must
//! be admitted as `TypeNode::Rational`, not refused as outside the Phase 1
//! strict-f64 subset.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;

fn diagnostics_of(source: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("rat-admission", source);
    result
        .diagnostics
        .errors()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect()
}

fn install_source_parser() {
    // Idempotent installer from emath-syntax, mirrors other suites here.
    emath_syntax::install_source_parser();
}

#[test]
fn rat_type_sites_are_admitted() {
    let source = "\
emath function F:
    inputs:
        a: Rat
        b: Rational
    outputs:
        r: Rat
    definitions:
        r = a * b
";
    let messages = diagnostics_of(source);
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("E-TYPE-001") || message.contains("outside the Phase 1 subset")),
        "Rat/Rational type sites must not be refused, got {:?}",
        messages
    );
    assert!(
        messages.is_empty(),
        "Rat/Rational program must fully type-check, got {:?}",
        messages
    );
}
