#![forbid(unsafe_code)]
//! Negative witnesses for `extern operator` admission: generic declarations
//! (E-TYPE-112) and declarations without a signature (E-SYN-101) must be
//! refused instead of admitted with the generic list or signature silently
//! dropped.

use emath_core::{FileId, Span};
use emath_sema::admit::check_tree;
use emath_syntax::parse_str;
use emath_syntax::tree::{Declaration, Item, SyntaxTree};

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

fn checked(tree: &SyntaxTree) -> bool {
    let result = check_tree(tree, &());
    let errors = result.diagnostics.errors().count();
    errors > 0
}

#[test]
fn generic_extern_operator_is_refused_with_e_type_112() {
    let source = "extern operator semantic_distance<D: Nat>(a: Float64, b: Float64) -> Float64:\
";
    let (tree, _) = parse_str(source);
    let result = check_tree(&tree, &());
    let found = result.diagnostics.errors().any(|e| e.code == "E-TYPE-112");
    assert!(
        found,
        "generic extern operator must be refused at admission"
    );
}

#[test]
fn extern_without_signature_is_refused_with_e_syn_101() {
    let tree = tree_with(declaration_without_signature());
    let result = check_tree(&tree, &());
    let found = result.diagnostics.errors().any(|e| e.code == "E-SYN-101");
    assert!(found, "extern operator without signature must be refused");
}

#[test]
fn plain_extern_operator_is_not_refused() {
    let source = "extern operator semantic_distance(a: Float64, b: Float64) -> Float64:\
";
    let (tree, _) = parse_str(source);
    let result = check_tree(&tree, &());
    assert!(!checked(&tree), "plain extern operator must not error");
    let _ = result;
}
