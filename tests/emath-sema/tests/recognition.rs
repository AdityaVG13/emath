//! `emath-sema` recognition-level admission tests (migrated from
//! `crates/emath-sema/src/recognition.rs`).

use emath_core::tree::{Declaration, Expr, ExprKind, Section, Stmt, StmtKind};
use emath_sema::admit::SemanticTrace;
use emath_sema::recognition::{KindDef, SchemaRule, admit_declaration};
use std::collections::BTreeMap;

fn span() -> emath_core::Span {
    emath_core::Span {
        file: emath_core::FileId(0),
        start: 0,
        end: 1,
    }
}

fn equation_section() -> Section {
    Section {
        name: "equation".to_string(),
        generic: None,
        args: None,
        suite: emath_core::tree::Suite {
            statements: vec![Stmt {
                kind: StmtKind::Equation {
                    left: Expr {
                        kind: ExprKind::Bool(true),
                        source: span(),
                    },
                    right: Expr {
                        kind: ExprKind::Bool(true),
                        source: span(),
                    },
                },
                source: span(),
            }],
            source: span(),
        },
        source: span(),
        head_source: span(),
    }
}

fn application(body: Vec<Stmt>) -> Declaration {
    Declaration {
        name: "Water".to_string(),
        generics: Vec::new(),
        item_kind: "Liquid".to_string(),
        as_kind: "Liquid".to_string(),
        attributes: Vec::new(),
        body,
        signature: None,
        source: span(),
        head_source: span(),
    }
}

fn kind_defs() -> BTreeMap<String, KindDef> {
    let mut defs = BTreeMap::new();
    defs.insert(
        "Liquid".to_string(),
        KindDef {
            name: "Liquid".to_string(),
            extends: None,
            schema: vec![SchemaRule::RequireSection("equation".to_string())],
        },
    );
    defs
}

fn admit(tree_decl: &Declaration) -> emath_core::Diagnostics {
    let mut package = emath_ir::SemanticPackage::new();
    let mut diagnostics = emath_core::Diagnostics::new();
    let mut trace = SemanticTrace::default();
    admit_declaration(
        tree_decl,
        &kind_defs(),
        &mut package,
        &mut diagnostics,
        &mut trace,
    );
    diagnostics
}

#[test]
fn required_equation_section_is_admitted_not_contradicted() {
    let decl = application(vec![Stmt {
        kind: StmtKind::Section(equation_section()),
        source: span(),
    }]);
    let diagnostics = admit(&decl);
    assert!(
        diagnostics.is_empty(),
        "application with the required equation section must admit cleanly, got {diagnostics:?}"
    );
}

#[test]
fn missing_required_equation_section_yields_single_e_kind_003() {
    let decl = application(Vec::new());
    let diagnostics = admit(&decl);
    let codes: Vec<&str> = diagnostics.errors().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec!["E-KIND-003"],
        "one honest refusal, no contradiction"
    );
}
