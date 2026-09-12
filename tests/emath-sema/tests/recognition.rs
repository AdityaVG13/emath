//! Kind-registry admission is leftover. Custom kinds do not remap a
//! later declaration into a constructor.

use emath_core::tree::{Declaration, Expr, ExprKind, Section, Stmt, StmtKind};
use emath_sema::admit::SemanticTrace;
use emath_sema::recognition::{KindDef, SchemaRule, admit_declaration};
use emath_test_harness::Probe;
use std::collections::BTreeMap;

fn span() -> emath_core::Span {
    emath_core::Span {
        file: emath_core::FileId(0),
        start: 0,
        end: 1,
    }
}

fn equation_section() -> Section {
    let bool_expr = || Expr {
        kind: ExprKind::Bool(true),
        source: span(),
    };
    Section {
        name: "equation".to_string(),
        generic: None,
        args: None,
        suite: emath_core::tree::Suite {
            statements: vec![Stmt {
                kind: StmtKind::Equation {
                    left: bool_expr(),
                    right: bool_expr(),
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
    BTreeMap::from([(
        "Liquid".to_string(),
        KindDef {
            name: "Liquid".to_string(),
            extends: None,
            schema: vec![SchemaRule::RequireSection("equation".to_string())],
        },
    )])
}

fn admit(tree_decl: &Declaration) -> emath_core::Diagnostics {
    let mut package = emath_ir::SemanticPackage::new();
    let mut diagnostics = emath_core::Diagnostics::new();
    admit_declaration(
        tree_decl,
        &kind_defs(),
        &mut package,
        &mut diagnostics,
        &mut SemanticTrace::default(),
    );
    diagnostics
}

#[test]
fn recognition_requires_the_equation_section() {
    let mut p = Probe::new("kind-registry admission refuses E-KIND-GONE");
    p.case("present-gone", |p| {
        let decl = application(vec![Stmt {
            kind: StmtKind::Section(equation_section()),
            source: span(),
        }]);
        let codes: Vec<&str> = admit(&decl).errors().map(|d| d.code).collect();
        p.eq("codes", codes, vec!["E-KIND-GONE"]);
    });
    p.case("missing-gone", |p| {
        let codes: Vec<&str> = admit(&application(Vec::new()))
            .errors()
            .map(|d| d.code)
            .collect();
        p.eq("codes", codes, vec!["E-KIND-GONE"]);
    });
    p.finish();
}
