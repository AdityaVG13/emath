//! Braket notation pack (04 section 2.4).
//!
//! The pack mounts opt-in (`use sci::physics::notation::braket`,
//! optionally `(convention = physics|math)`) and admits the braket
//! surface on the REAL 2-level discrete carrier:
//! - `|i⟩` ket (label 0/1) desugars to the constant basis vector;
//! - `⟨φ|ψ⟩` desugars to the admitted `dot` (sesquilinear conjugation
//!   is the identity on real entries — the Complex carrier is the
//!   documented follow-up);
//! - `⟨i|j⟩` constant-folds to the orthonormality value (0/1);
//! - `|i⟩⟨j|` desugars to the constant projector matrix;
//! - `⟨ψ|P|ψ⟩` desugars to the double sum (all admitted ops).
//! Unmounted glyphs refuse naming the pack (nabla precedent); labels
//! outside the 2-level carrier refuse; the convention vocabulary is
//! validated at the mount.
//!
//! Failure-first: every pin below is RED until the token/lexer/parser
//! arms land (`⟨`/`⟩` previously lexed as unknown glyphs).

use emath_core::limits::Limits;
use emath_core::tree::{Expr, ExprKind, StmtKind};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

const MOUNT: &str = "use sci::physics::notation::braket\n\n";

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("braket-notation", source)
}

fn defn_exprs(source: &str) -> Vec<Expr> {
    install_source_parser();
    let (tree, diags) = emath_syntax::parse_str(source);
    assert!(!diags.has_errors(), "{diags:?}");
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("declaration expected, got {:?}", tree.items.last());
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .expect("definitions section");
    defs.suite
        .statements
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::Assign { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn is_float(expr: &Expr, text: &str) -> bool {
    matches!(&expr.kind, ExprKind::Float(value) if value == text)
}

#[test]
fn unmounted_ket_refuses_naming_the_pack() {
    // Glyphs are opt-in: a ket without the mount refuses and names the
    // import (nabla precedent — never a silent identifier reading).
    let (tree, diags) =
        emath_syntax::parse_str("emath function f:\n    definitions:\n        v = |0⟩\n");
    let _ = tree;
    assert!(
        diags.errors().any(|error| error.code == "E-SYN-101"
            && error.message.contains("sci::physics::notation::braket")),
        "unmounted ket must refuse naming the pack, got {diags:?}"
    );
}

#[test]
fn unmounted_braket_form_refuses_naming_the_pack() {
    let (tree, diags) =
        emath_syntax::parse_str("emath function f:\n    definitions:\n        ip = ⟨v|w⟩\n");
    let _ = tree;
    assert!(
        diags.errors().any(|error| error.code == "E-SYN-101"
            && error.message.contains("sci::physics::notation::braket")),
        "unmounted braket must refuse naming the pack, got {diags:?}"
    );
}

#[test]
fn mounted_ket_label_desugars_to_basis_vector() {
    // `|0⟩` is the first basis vector, `|1⟩` the second, as constant
    // real 2-vectors (List literals lower to Vector values).
    let defs = defn_exprs(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        v = |0⟩\n        w = |1⟩\n"
    ));
    assert_eq!(defs.len(), 2);
    let ExprKind::List(items) = &defs[0].kind else {
        panic!(
            "ket must desugar to a constant vector, got {:?}",
            defs[0].kind
        );
    };
    assert_eq!(items.len(), 2);
    assert!(is_float(&items[0], "1.0") && is_float(&items[1], "0.0"));
    let ExprKind::List(items) = &defs[1].kind else {
        panic!(
            "ket must desugar to a constant vector, got {:?}",
            defs[1].kind
        );
    };
    assert!(is_float(&items[0], "0.0") && is_float(&items[1], "1.0"));
}

#[test]
fn braket_inner_product_desugars_to_dot() {
    // `⟨v|w⟩` is the sesquilinear inner product; on the real carrier the
    // conjugate on the bra is the identity, so the exact desugar is the
    // admitted `dot` builtin.
    let defs = defn_exprs(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        ip = ⟨v|w⟩\n"
    ));
    let ExprKind::Call { function, args } = &defs[0].kind else {
        panic!("braket must desugar to a call, got {:?}", defs[0].kind);
    };
    assert!(
        matches!(&function.kind, ExprKind::Path { segments, generics: None }
            if segments == &vec!["dot".to_string()]),
        "function was {function:?}"
    );
    assert_eq!(args.len(), 2);
    assert!(
        matches!(&args[0].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["v".to_string()])
    );
    assert!(
        matches!(&args[1].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["w".to_string()])
    );
}

#[test]
fn label_braket_constant_folds_orthonormality() {
    // `⟨i|j⟩` on basis labels folds to the Kronecker delta: the
    // orthonormality check `⟨0|1⟩ == 0` is a constant, not a hope.
    let defs = defn_exprs(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        same = ⟨0|0⟩\n        cross = ⟨0|1⟩\n"
    ));
    assert!(matches!(&defs[0].kind, ExprKind::Int(text) if text == "1"));
    assert!(matches!(&defs[1].kind, ExprKind::Int(text) if text == "0"));
}

#[test]
fn superposition_admits() {
    // `psi = (|0⟩ + |1⟩) * (1.0 / sqrt(2.0))` — the normalized
    // superposition. Spelling correction (documented, not a weakening):
    // the prose writes `/ sqrt(2.0)`, but (Vector, scalar)
    // division is not an admitted operator yet (the Div arm is
    // numeric-pairs only, lowering.rs); the exact multiplicative
    // spelling via VectorScale is admitted today and mathematically
    // identical. Division-by-scalar admission is a lowering follow-up.
    let checked = check_source(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        psi = (|0⟩ + |1⟩) * (1.0 / sqrt(2.0))\n"
    ));
    assert!(
        !checked.diagnostics.has_errors(),
        "superposition must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}

#[test]
fn projector_desugars_to_constant_matrix() {
    // `|1⟩⟨1|` is the projector onto basis state 1, and the OFF-DIAGONAL
    // `|0⟩⟨1|` pins the (row, col) placement — a transposed mutant must
    // not survive (mutation B3 caught exactly that hole).
    let defs = defn_exprs(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        P1 = |1⟩⟨1|\n        off = |0⟩⟨1|\n"
    ));
    let ExprKind::List(rows) = &defs[0].kind else {
        panic!("projector must desugar to a matrix, got {:?}", defs[0].kind);
    };
    assert_eq!(rows.len(), 2);
    let ExprKind::List(row0) = &rows[0].kind else {
        panic!("projector row 0, got {:?}", rows[0].kind);
    };
    let ExprKind::List(row1) = &rows[1].kind else {
        panic!("projector row 1, got {:?}", rows[1].kind);
    };
    assert!(is_float(&row0[0], "0.0") && is_float(&row0[1], "0.0"));
    assert!(is_float(&row1[0], "0.0") && is_float(&row1[1], "1.0"));
    // Off-diagonal: `|0⟩⟨1|` puts the 1 at [0, 1] (ket = row, bra = col).
    let ExprKind::List(rows) = &defs[1].kind else {
        panic!("projector must desugar to a matrix, got {:?}", defs[1].kind);
    };
    let ExprKind::List(row0) = &rows[0].kind else {
        panic!("off-diagonal row 0, got {:?}", rows[0].kind);
    };
    let ExprKind::List(row1) = &rows[1].kind else {
        panic!("off-diagonal row 1, got {:?}", rows[1].kind);
    };
    assert!(is_float(&row0[0], "0.0") && is_float(&row0[1], "1.0"));
    assert!(is_float(&row1[0], "0.0") && is_float(&row1[1], "0.0"));
    let checked = check_source(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        P1 = |1⟩⟨1|\n        off = |0⟩⟨1|\n"
    ));
    assert!(
        !checked.diagnostics.has_errors(),
        "projector must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}

#[test]
fn sandwich_desugars_to_double_sum() {
    // `⟨psi|P1|psi⟩` = sum_j conj(psi_j) * (sum_k P1[j,k] * psi_k); on
    // the real carrier the conjugate is the identity, so the desugar is
    // the pure double sum over admitted ops (sum binder + indexing).
    let defs = defn_exprs(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        p1 = ⟨psi|P1|psi⟩\n"
    ));
    let ExprKind::Binder {
        kind,
        binders,
        body,
        guard,
    } = &defs[0].kind
    else {
        panic!(
            "sandwich must desugar to a sum binder, got {:?}",
            defs[0].kind
        );
    };
    assert!(matches!(kind, emath_core::tree::BinderKind::Sum));
    assert!(guard.is_none());
    assert_eq!(binders.len(), 1);
    assert_eq!(binders[0].name, "j");
    let ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Mul,
        left,
        right,
    } = &body.kind
    else {
        panic!("outer sum body, got {:?}", body.kind);
    };
    assert!(matches!(&left.kind, ExprKind::Index { value, indices }
        if matches!(&value.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["psi".to_string()])
            && indices.len() == 1));
    let ExprKind::Binder {
        binders: inner_binders,
        body: inner_body,
        ..
    } = &right.kind
    else {
        panic!("inner sum, got {:?}", right.kind);
    };
    assert_eq!(inner_binders[0].name, "k");
    assert!(matches!(&inner_body.kind, ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Mul,
        left,
        ..
    } if matches!(&left.kind, ExprKind::Index { indices, .. } if indices.len() == 2)));
    let checked = check_source(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        psi = |0⟩ + |1⟩\n        P1 = |1⟩⟨1|\n        p1 = ⟨psi|P1|psi⟩\n"
    ));
    assert!(
        !checked.diagnostics.has_errors(),
        "sandwich must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}

#[test]
fn ket_label_outside_two_level_carrier_refuses() {
    // The declared carrier is the 2-level discrete system; a wider
    // carrier (Complex entries, general dimension) is the documented
    // follow-up, so label 2 refuses instead of inventing a shape.
    let (tree, diags) = emath_syntax::parse_str(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        v = |2⟩\n"
    ));
    let _ = tree;
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("2-level")),
        "label 2 must refuse naming the carrier, got {diags:?}"
    );
}

#[test]
fn convention_parameter_validates() {
    // The pack parameter is the declared convention (physics
    // anti-linear vs math bilinear); the vocabulary is validated at the
    // mount. On the real carrier the two coincide (documented).
    let checked = check_source(
        "use sci::physics::notation::braket(convention = math)\n\nemath function f:\n    definitions:\n        v = |0⟩\n",
    );
    assert!(
        !checked.diagnostics.has_errors(),
        "math convention must mount, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let (tree, diags) = emath_syntax::parse_str(
        "use sci::physics::notation::braket(convention = weird)\n\nemath function f:\n    definitions:\n        v = |0⟩\n",
    );
    let _ = tree;
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("convention")),
        "unknown convention must refuse, got {diags:?}"
    );
}

#[test]
fn braket_notation_fixture_typechecks() {
    let source = include_str!("../../../tests/fixtures/language/intro/braket-notation.emath");
    let checked = check_source(source);
    assert!(
        !checked.diagnostics.has_errors(),
        "braket notation fixture must typecheck, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}
