#![forbid(unsafe_code)]
//! Negative witnesses for `extern operator` declarations after the
//! constructor cutover: `extern` is not a declaration kind, so every
//! spelling (generic, unsigned, plain) refuses at admission with
//! `E-KIND-001` instead of admitting with the generic list or signature
//! silently dropped. The pre-cutover typed codes (`E-TYPE-112` for
//! generics, `E-SYN-101` for a missing signature) belonged to the
//! extern admission lane, which no user file can reach.

use emath_core::{FileId, Span};
use emath_sema::admit::check_tree;
use emath_syntax::parse_str;
use emath_syntax::tree::{Declaration, Item, SyntaxTree};
use emath_test_harness::{Probe, boot};

fn span() -> Span {
    Span {
        file: FileId(0),
        start: 0,
        end: 1,
    }
}

fn declaration_without_signature() -> Declaration {
    Declaration {
        name: "distance".into(),
        generics: Vec::new(),
        item_kind: "extern".into(),
        as_kind: "operator".into(),
        attributes: Vec::new(),
        body: Vec::new(),
        signature: None,
        source: span(),
        head_source: span(),
    }
}

fn tree_with(decl: Declaration) -> SyntaxTree {
    SyntaxTree {
        source: span(),
        items: vec![Item::Declaration(decl)],
    }
}

fn has_code(result: &emath_sema::CheckResult, code: &str) -> bool {
    result.diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("every extern operator spelling refuses as a non-constructor kind");
    p.case("generic", |p| {
        let source =
            "extern operator semantic_distance<D: Nat>(a: Float64, b: Float64) -> Float64:\n";
        let (tree, _) = parse_str(source);
        let result = check_tree(&tree);
        p.demand(
            "E-KIND-001",
            has_code(&result, "E-KIND-001"),
            "generic extern operator must be refused as a non-constructor kind",
        );
    });
    p.case("no-signature", |p| {
        let tree = tree_with(declaration_without_signature());
        let result = check_tree(&tree);
        p.demand(
            "E-KIND-001",
            has_code(&result, "E-KIND-001"),
            "extern operator without signature must be refused as a non-constructor kind",
        );
    });
    p.case("plain", |p| {
        let source =
            "extern operator semantic_distance(a: Float64, b: Float64) -> Float64:\n";
        let (tree, _) = parse_str(source);
        let result = check_tree(&tree);
        p.demand(
            "E-KIND-001",
            has_code(&result, "E-KIND-001"),
            "plain extern operator must be refused as a non-constructor kind",
        );
        p.eq(
            "refused",
            result.diagnostics.errors().count() > 0,
            true,
        );
    });
    p.finish();
}
