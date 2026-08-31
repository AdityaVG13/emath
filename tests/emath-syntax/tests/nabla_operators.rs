//! `emath-r3-nabla-pack-o6jp`: nabla operators pack with world-dependent
//! meaning (spec 04 section 2.3) — parser pack-mount slice.
//!
//! `use sci::physics::notation::nabla` mounts the pack (opt-in only, X4:
//! the pack owns the glyphs bundle-wide once mounted). Mount is data: the
//! glyphs desugar to EXISTING discrete-stencil builtins (`core::pde::*`),
//! no IR enum grows. Field shape and spacing mirror the real builtins
//! exactly (checked against `admit/lowering.rs`): fields are Matrix
//! values, spacing is ONE explicit cell-width argument — mesh spacing is
//! world data, never ambient (arity honesty).
//!
//! Desugars pinned here:
//! - `∇(u, dx)`        → `core::pde::gradient(u, dx)` (2 args)
//! - `∇²(u, dx)`       → `core::pde::laplacian_2d(u, dx)` (2 args)
//! - `∇·(vx, vy, dx)`  → `core::pde::div_2d(vx, vy, dx)` (3 args)
//! - `∇×(u, v, dx)`    → 2D scalar curl as pure sugar:
//!   `core::pde::gradient_2d_x(v, dx) − core::pde::gradient_2d_y(u, dx)`
//! - `∇×` with 3D arity (4 args) refuses typed: the 3D curl OperatorDef
//!   is pending in the discrete stencil world (honest boundary, named in
//!   the diagnostic).
//!
//! Without the use line every nabla glyph refuses (`E-SYN-101`) naming
//! the pack import — glyphs are never ambient.
//!
//! Failure-first: every fold pin below is RED until the lexer glyphs and
//! the pack-mount hook land.

use emath_core::tree::{ExprKind, StmtKind};
use emath_syntax::parse_str;

const MOUNT: &str = "use sci::physics::notation::nabla\n\n";

fn def_expr_of(source: &str) -> ExprKind {
    let (tree, diags) = parse_str(source);
    assert!(!diags.has_errors(), "{diags:?}");
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("declaration expected");
    };
    let Some(stmt) = decl.body.iter().find(|stmt| {
        matches!(&stmt.kind, StmtKind::Section(s) if s.name == "definitions")
    }) else {
        panic!("definitions section expected");
    };
    let StmtKind::Section(definitions) = &stmt.kind else {
        unreachable!()
    };
    match &definitions.suite.statements[0].kind {
        StmtKind::Assign { value, .. } => value.kind.clone(),
        other => panic!("assignment expected, got {other:?}"),
    }
}

fn assert_target(expr: &ExprKind, expected: &[&str], arity: usize) {
    let ExprKind::Call { function, args } = expr else {
        panic!("call expected, got {expr:?}")
    };
    let ExprKind::Path { segments, .. } = &function.kind else {
        panic!("call target must be a path, got {:?}", function.kind)
    };
    let got: Vec<&str> = segments.iter().map(String::as_str).collect();
    assert_eq!(
        got, expected,
        "desugar target mismatch for {expected:?}, got {got:?}"
    );
    assert_eq!(args.len(), arity, "desugar arity mismatch for {expected:?}");
}

#[test]
fn unmounted_nabla_glyph_refuses_naming_the_pack() {
    // Opt-in only: without the use line the glyph is refused with the
    // pack import named — never a silent unknown-ident.
    let (_, diags) = parse_str(
        "emath function f:\n    inputs:\n        u: Float64\n        dx: Float64\n    definitions:\n        g = ∇(u, dx)\n",
    );
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-101"
                && error.message.contains("sci::physics::notation::nabla")),
        "unmounted ∇ must refuse E-SYN-101 naming the pack, got {:?}",
        diags.errors().map(|e| e.code.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn nabla_grad_desugars_to_gradient_call() {
    // `∇` is the Vector-field gradient (1D); fields for the rank-2 forms
    // are Matrix values — shapes mirror the real builtins exactly.
    let expr = def_expr_of(&format!(
        "{MOUNT}emath function f:\n    inputs:\n        u: Float64\n        dx: Float64\n    definitions:\n        g = ∇(u, dx)\n"
    ));
    assert_target(&expr, &["core", "pde", "gradient"], 2);
}

#[test]
fn nabla_lap_desugars_to_laplacian_2d_call() {
    // ∇² on the stencil lane is the discrete 5-point Laplacian over a
    // Matrix field; ONE explicit cell-width argument (2D is implicit in
    // the field shape).
    let expr = def_expr_of(&format!(
        "{MOUNT}emath function f:\n    inputs:\n        u: Float64\n        dx: Float64\n    definitions:\n        l = ∇²(u, dx)\n"
    ));
    assert_target(&expr, &["core", "pde", "laplacian_2d"], 2);
}

#[test]
fn nabla_div_desugars_to_div_2d_call() {
    let expr = def_expr_of(&format!(
        "{MOUNT}emath function f:\n    inputs:\n        vx: Float64\n        vy: Float64\n        dx: Float64\n    definitions:\n        d = ∇·(vx, vy, dx)\n"
    ));
    assert_target(&expr, &["core", "pde", "div_2d"], 3);
}

#[test]
fn nabla_curl_2d_desugars_to_component_sugar() {
    // 2D scalar curl = ∂v/∂x − ∂u/∂y, expressed through existing
    // component-gradient builtins with one shared cell width —
    // computable today, no new op.
    let expr = def_expr_of(&format!(
        "{MOUNT}emath function f:\n    inputs:\n        u: Float64\n        v: Float64\n        dx: Float64\n    definitions:\n        c = ∇×(u, v, dx)\n"
    ));
    let ExprKind::Binary {
        op: emath_core::tree::BinaryOp::Sub,
        left,
        right,
    } = &expr
    else {
        panic!("curl 2D must desugar to a subtraction, got {expr:?}")
    };
    assert_target(&left.kind, &["core", "pde", "gradient_2d_x"], 2);
    assert_target(&right.kind, &["core", "pde", "gradient_2d_y"], 2);
}

#[test]
fn nabla_curl_3d_arity_refuses_typed() {
    // The 3D curl OperatorDef is pending in the discrete stencil world;
    // a 4-argument ∇× (three fields + spacing) refuses with the boundary
    // named, never a silent wrong-world computation.
    let (_, diags) = parse_str(&format!(
        "{MOUNT}emath function f:\n    inputs:\n        u: Float64\n        v: Float64\n        w: Float64\n        dx: Float64\n    definitions:\n        c = ∇×(u, v, w, dx)\n"
    ));
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-101"
                && error.message.contains("3D curl")),
        "3D-arity ∇× must refuse typed naming the pending OperatorDef, got {:?}",
        diags.errors().map(|e| e.code.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn unmounted_ascii_builtins_still_admit_as_plain_calls() {
    // The pack gates GLYPHS only; the direct builtin spellings
    // (`laplacian_2d(u, dx)`) are untouched plain calls.
    let expr = def_expr_of(
        "emath function f:\n    inputs:\n        u: Float64\n        dx: Float64\n    definitions:\n        l = laplacian_2d(u, dx)\n",
    );
    assert_target(&expr, &["laplacian_2d"], 2);
}
