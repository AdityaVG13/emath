//! Jacobian-as-value surface (Track A3). The uniform
//! surface mirrors the shipped binder/expression forms:
//! `jacobian(<expr>) wrt <var>, <var>, ...` — same postfix `wrt` list
//! used by `minimize(objective) wrt x, y`. A scalar body yields a row
//! Jacobian (`Matrix[1, n]`); a list-vector body yields the full
//! matrix (`Matrix[m, n]`). These tests are written failure-first
//! against the intended surface: the `jacobian` keyword does not exist
//! in the parser yet, so every test must fail until passes 2-4 land
//! the AST/IR form, evaluation, and the typed refusals.

const JACOBIAN_SCALAR_BODY: &str = "\
emath function JacobianScalarBody:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[1, 2]

    definitions:
        f = x * y + sin(x + y)
        J = jacobian(f) wrt x, y
";

const JACOBIAN_VECTOR_BODY: &str = "\
emath function JacobianVectorBody:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y + sin(x)
        J = jacobian([f1, f2]) wrt x, y
";

const JACOBIAN_VECTOR_SINGLE_VAR: &str = "\
emath function JacobianVectorSingleVar:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 1]

    definitions:
        f1 = x * x
        f2 = y + x
        J = jacobian([f1, f2]) wrt x
";

const JACOBIAN_UNKNOWN_WRT_VARIABLE: &str = "\
emath function JacobianUnknownWrt:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[1, 2]

    definitions:
        f = x * y
        J = jacobian(f) wrt x, z
";

const JACOBIAN_SHAPE_MISMATCH: &str = "\
emath function JacobianShapeMismatch:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f = x * y
        J = jacobian(f) wrt x, y
";

const JACOBIAN_NONDIF_FERENTIABLE_BODY: &str = "\
emath function JacobianNondifferentiable:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        J = jacobian([x > 0.0, y]) wrt x, y
";

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn jacobian() {
    boot();
    let mut probe = Probe::new("Jacobian-as-value surface (Track A3). The uniform surface mirrors the shipped binder/expression forms: `jacobian(<expr>) wrt <var>, <var>, ...` — same");
    probe.case("jacobian_of_scalar_body_with_wrt_list_admits", |p| {
    let f0 = p.failures().len();

    // The row Jacobian of a scalar expression w.r.t. two variables is
    // a `Matrix[1, 2]` value: `[df/dx, df/dy]`.
    let errors = check(JACOBIAN_SCALAR_BODY, "jacobian-scalar-body");
    p.demand("1",errors.is_empty(), format!(
        "jacobian(f) wrt x, y must admit as a Matrix[1, 2] value; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("jacobian_of_vector_body_with_wrt_list_admits", |p| {
    let f0 = p.failures().len();

    // A list vector `[f1, f2]` differentiates component-wise: each row
    // is the derivative of that component w.r.t. every `wrt` variable.
    let errors = check(JACOBIAN_VECTOR_BODY, "jacobian-vector-body");
    p.demand("1",errors.is_empty(), format!(
        "jacobian([f1, f2]) wrt x, y must admit as a Matrix[2, 2] value; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("jacobian_of_vector_body_with_single_wrt_admits", |p| {
    let f0 = p.failures().len();

    // One `wrt` variable with an m-component body gives a Matrix[m, 1]
    // column Jacobian.
    let errors = check(JACOBIAN_VECTOR_SINGLE_VAR, "jacobian-vector-single-var");
    p.demand("1",errors.is_empty(), format!(
        "jacobian([f1, f2]) wrt x must admit as a Matrix[2, 1] value; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("jacobian_wrt_unknown_variable_is_a_typed_refusal_not_a_generic_crash", |p| {
    let f0 = p.failures().len();

    // The refusal must target the offending `wrt` variable with the
    // typed code `E-TYPE-010` (the derivative form's existing
    // input-scope refusal; jacobian cells are derivatives, so the
    // jacobian shares it) — never a silent half-admit.
    let errors = check(JACOBIAN_UNKNOWN_WRT_VARIABLE, "jacobian-unknown-wrt");
    p.demand("1",errors.iter().any(|error| error.starts_with("E-TYPE-010")), format!(
        "jacobian(f) wrt x, z with `z` undeclared must refuse with E-TYPE-010 (input-scope); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("jacobian_shape_mismatch_is_a_typed_dimension_refusal", |p| {
    let f0 = p.failures().len();

    // A scalar body with two `wrt` variables is `Matrix[1, 2]`; the
    // declared `Matrix[2, 2]` output must refuse with the typed
    // dimension code — never a silent resize and never a generic
    // crash.
    let errors = check(JACOBIAN_SHAPE_MISMATCH, "jacobian-shape-mismatch");
    p.demand("1",errors.iter().any(|error| {
            error.starts_with("E-TYPE-012")
                && error.contains("Matrix[1, 2]")
                && error.contains("Matrix[2, 2]")
        }), format!(
        "declared/inferred matrix shape mismatch must refuse with E-TYPE-012 naming both shapes; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("jacobian_non_numeric_component_is_a_typed_nondifferentiability_refusal", |p| {
    let f0 = p.failures().len();

    // A non-numeric component (`x > 0.0`) is not a differentiable
    // body; the refusal must be the typed "must be numeric" code, not
    // a silent zero or crash.
    let errors = check(
        JACOBIAN_NONDIF_FERENTIABLE_BODY,
        "jacobian-nondifferentiable",
    );
    p.demand("1",errors.iter().any(|error| {
            error.starts_with("E-TYPE-012") && error.contains("derivative body must be numeric")
        }), format!(
        "non-numeric jacobian components must refuse with E-TYPE-012 (numeric body); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
