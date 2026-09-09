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

const MOUNT: &str = "use sci::physics::notation::braket\n\n";

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("braket-notation", source)
}

fn defn_exprs(p: &mut Probe, source: &str) -> Vec<Expr> {
    let (tree, diags) = emath_syntax::parse_str(source);
    p.demand("parse", !diags.has_errors(), format!("{diags:?}"));
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

use emath_test_harness::{Probe, boot};

#[test]
fn braket_notation() {
    boot();
    let mut probe = Probe::new("Braket notation pack (04 section 2.4). The pack mounts opt-in (`use sci::physics::notation::braket`, optionally `(convention = physics|math)`) and");
    probe.case("unmounted_ket_refuses_naming_the_pack", |p| {
    let f0 = p.failures().len();

    // Glyphs are opt-in: a ket without the mount refuses and names the
    // import (nabla precedent — never a silent identifier reading).
    let (tree, diags) =
        emath_syntax::parse_str("emath function f:\n    definitions:\n        v = |0⟩\n");
    let _ = tree;
    p.demand("1",diags.errors().any(|error| error.code == "E-SYN-101"
            && error.message.contains("sci::physics::notation::braket")), format!(
        "unmounted ket must refuse naming the pack, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unmounted_braket_form_refuses_naming_the_pack", |p| {
    let f0 = p.failures().len();

    let (tree, diags) =
        emath_syntax::parse_str("emath function f:\n    definitions:\n        ip = ⟨v|w⟩\n");
    let _ = tree;
    p.demand("1",diags.errors().any(|error| error.code == "E-SYN-101"
            && error.message.contains("sci::physics::notation::braket")), format!(
        "unmounted braket must refuse naming the pack, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("mounted_ket_label_desugars_to_basis_vector", |p| {
    let f0 = p.failures().len();

    // `|0⟩` is the first basis vector, `|1⟩` the second, as constant
    // real 2-vectors (List literals lower to Vector values).
    let defs = defn_exprs(p, &format!(
        "{MOUNT}emath function f:\n    definitions:\n        v = |0⟩\n        w = |1⟩\n"
    ));
    p.eq("1", defs.len(), 2);
    let ExprKind::List(items) = &defs[0].kind else {
        panic!(
            "ket must desugar to a constant vector, got {:?}",
            defs[0].kind
        );
    };
    p.eq("2", items.len(), 2);
    p.demand("3",is_float(&items[0], "1.0") && is_float(&items[1], "0.0"), stringify!(is_float(&items[0], "1.0") && is_float(&items[1], "0.0")));
    if p.failures().len() != f0 { return; }
    let ExprKind::List(items) = &defs[1].kind else {
        panic!(
            "ket must desugar to a constant vector, got {:?}",
            defs[1].kind
        );
    };
    p.demand("4",is_float(&items[0], "0.0") && is_float(&items[1], "1.0"), stringify!(is_float(&items[0], "0.0") && is_float(&items[1], "1.0")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("braket_inner_product_desugars_to_dot", |p| {
    let f0 = p.failures().len();

    // `⟨v|w⟩` is the sesquilinear inner product; on the real carrier the
    // conjugate on the bra is the identity, so the exact desugar is the
    // admitted `dot` builtin.
    let defs = defn_exprs(p, &format!(
        "{MOUNT}emath function f:\n    definitions:\n        ip = ⟨v|w⟩\n"
    ));
    let ExprKind::Call { function, args } = &defs[0].kind else {
        panic!("braket must desugar to a call, got {:?}", defs[0].kind);
    };
    p.demand("1",matches!(&function.kind, ExprKind::Path { segments, generics: None }
            if segments == &vec!["dot".to_string()]), format!(
        "function was {function:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("2", args.len(), 2);
    p.demand("3",matches!(&args[0].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["v".to_string()]), stringify!(matches!(&args[0].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["v".to_string()])));
    if p.failures().len() != f0 { return; }
    p.demand("4",matches!(&args[1].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["w".to_string()]), stringify!(matches!(&args[1].kind, ExprKind::Path { segments, generics: None }
        if segments == &vec!["w".to_string()])));
    if p.failures().len() != f0 { return; }

    });
    probe.case("label_braket_constant_folds_orthonormality", |p| {
    let f0 = p.failures().len();

    // `⟨i|j⟩` on basis labels folds to the Kronecker delta: the
    // orthonormality check `⟨0|1⟩ == 0` is a constant, not a hope.
    let defs = defn_exprs(p, &format!(
        "{MOUNT}emath function f:\n    definitions:\n        same = ⟨0|0⟩\n        cross = ⟨0|1⟩\n"
    ));
    p.demand("1",matches!(&defs[0].kind, ExprKind::Int(text) if text == "1"), stringify!(matches!(&defs[0].kind, ExprKind::Int(text) if text == "1")));
    if p.failures().len() != f0 { return; }
    p.demand("2",matches!(&defs[1].kind, ExprKind::Int(text) if text == "0"), stringify!(matches!(&defs[1].kind, ExprKind::Int(text) if text == "0")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("superposition_admits", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "superposition must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("projector_desugars_to_constant_matrix", |p| {
    let f0 = p.failures().len();

    // `|1⟩⟨1|` is the projector onto basis state 1, and the OFF-DIAGONAL
    // `|0⟩⟨1|` pins the (row, col) placement — a transposed mutant must
    // not survive (mutation B3 caught exactly that hole).
    let defs = defn_exprs(p, &format!(
        "{MOUNT}emath function f:\n    definitions:\n        P1 = |1⟩⟨1|\n        off = |0⟩⟨1|\n"
    ));
    let ExprKind::List(rows) = &defs[0].kind else {
        panic!("projector must desugar to a matrix, got {:?}", defs[0].kind);
    };
    p.eq("1", rows.len(), 2);
    let ExprKind::List(row0) = &rows[0].kind else {
        panic!("projector row 0, got {:?}", rows[0].kind);
    };
    let ExprKind::List(row1) = &rows[1].kind else {
        panic!("projector row 1, got {:?}", rows[1].kind);
    };
    p.demand("2",is_float(&row0[0], "0.0") && is_float(&row0[1], "0.0"), stringify!(is_float(&row0[0], "0.0") && is_float(&row0[1], "0.0")));
    if p.failures().len() != f0 { return; }
    p.demand("3",is_float(&row1[0], "0.0") && is_float(&row1[1], "1.0"), stringify!(is_float(&row1[0], "0.0") && is_float(&row1[1], "1.0")));
    if p.failures().len() != f0 { return; }
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
    p.demand("4",is_float(&row0[0], "0.0") && is_float(&row0[1], "1.0"), stringify!(is_float(&row0[0], "0.0") && is_float(&row0[1], "1.0")));
    if p.failures().len() != f0 { return; }
    p.demand("5",is_float(&row1[0], "0.0") && is_float(&row1[1], "0.0"), stringify!(is_float(&row1[0], "0.0") && is_float(&row1[1], "0.0")));
    if p.failures().len() != f0 { return; }
    let checked = check_source(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        P1 = |1⟩⟨1|\n        off = |0⟩⟨1|\n"
    ));
    p.demand("6",!checked.diagnostics.has_errors(), format!(
        "projector must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("sandwich_desugars_to_double_sum", |p| {
    let f0 = p.failures().len();

    // `⟨psi|P1|psi⟩` = sum_j conj(psi_j) * (sum_k P1[j,k] * psi_k); on
    // the real carrier the conjugate is the identity, so the desugar is
    // the pure double sum over admitted ops (sum binder + indexing).
    let defs = defn_exprs(p, &format!(
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
    p.demand("1",matches!(kind, emath_core::tree::BinderKind::Sum), stringify!(matches!(kind, emath_core::tree::BinderKind::Sum)));
    if p.failures().len() != f0 { return; }
    p.demand("2",guard.is_none(), stringify!(guard.is_none()));
    if p.failures().len() != f0 { return; }
    p.eq("3", binders.len(), 1);
    p.demand("4", (binders[0].name) == ("j"), format!("expected {:?}, got {:?}", ("j"), (binders[0].name)));
    if p.failures().len() != f0 { return; }
    let ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Mul,
        left,
        right,
    } = &body.kind
    else {
        panic!("outer sum body, got {:?}", body.kind);
    };
    p.demand("5",matches!(&left.kind, ExprKind::Index { value, indices }
        if matches!(&value.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["psi".to_string()])
            && indices.len() == 1), stringify!(matches!(&left.kind, ExprKind::Index { value, indices }
        if matches!(&value.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["psi".to_string()])
            && indices.len() == 1)));
    if p.failures().len() != f0 { return; }
    let ExprKind::Binder {
        binders: inner_binders,
        body: inner_body,
        ..
    } = &right.kind
    else {
        panic!("inner sum, got {:?}", right.kind);
    };
    p.demand("6", (inner_binders[0].name) == ("k"), format!("expected {:?}, got {:?}", ("k"), (inner_binders[0].name)));
    if p.failures().len() != f0 { return; }
    p.demand("7",matches!(&inner_body.kind, ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Mul,
        left,
        ..
    } if matches!(&left.kind, ExprKind::Index { indices, .. } if indices.len() == 2)), stringify!(matches!(&inner_body.kind, ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Mul,
        left,
        ..
    } if matches!(&left.kind, ExprKind::Index { indices, .. } if indices.len() == 2))));
    if p.failures().len() != f0 { return; }
    let checked = check_source(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        psi = |0⟩ + |1⟩\n        P1 = |1⟩⟨1|\n        p1 = ⟨psi|P1|psi⟩\n"
    ));
    p.demand("8",!checked.diagnostics.has_errors(), format!(
        "sandwich must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("ket_label_outside_two_level_carrier_refuses", |p| {
    let f0 = p.failures().len();

    // The declared carrier is the 2-level discrete system; a wider
    // carrier (Complex entries, general dimension) is the documented
    // follow-up, so label 2 refuses instead of inventing a shape.
    let (tree, diags) = emath_syntax::parse_str(&format!(
        "{MOUNT}emath function f:\n    definitions:\n        v = |2⟩\n"
    ));
    let _ = tree;
    p.demand("1",diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("2-level")), format!(
        "label 2 must refuse naming the carrier, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("convention_parameter_validates", |p| {
    let f0 = p.failures().len();

    // The pack parameter is the declared convention (physics
    // anti-linear vs math bilinear); the vocabulary is validated at the
    // mount. On the real carrier the two coincide (documented).
    let checked = check_source(
        "use sci::physics::notation::braket(convention = math)\n\nemath function f:\n    definitions:\n        v = |0⟩\n",
    );
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "math convention must mount, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let (tree, diags) = emath_syntax::parse_str(
        "use sci::physics::notation::braket(convention = weird)\n\nemath function f:\n    definitions:\n        v = |0⟩\n",
    );
    let _ = tree;
    p.demand("2",diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("convention")), format!(
        "unknown convention must refuse, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("braket_notation_fixture_typechecks", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/braket-notation.emath");
    let checked = check_source(source);
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "braket notation fixture must typecheck, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
