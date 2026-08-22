//! Continuous `emath model` admission: explicit `derivative(state) = rhs`.

use emath_core::limits::Limits;
use emath_ir::ExprNode;
use emath_sema::admit::CheckResult;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check_source(name: &str, source: &str) -> CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

fn decay_model() -> &'static str {
    "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        derivative(x) = -k * x
"
}

#[test]
fn model_derivative_equation_admits_as_rate() {
    let result = check_source("decay", decay_model());
    assert!(
        !result.diagnostics.has_errors(),
        "explicit ODE model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert_eq!(decl.kind_label, "model");
    assert_eq!(decl.state.len(), 1);
    assert!(decl.definitions.contains_key("der_x"));
    let rate = decl.definitions["der_x"];
    assert!(matches!(
        result.package.expr(rate),
        Some(ExprNode::Binary { .. })
    ));
}

#[test]
fn model_der_call_and_wrt_time_admit() {
    let source = "\
emath model Spring:
    inputs:
        m: Float64
        c: Float64
        k: Float64
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
        derivative v wrt t = (-c * v - k * x) / m
";
    let result = check_source("spring", source);
    assert!(
        !result.diagnostics.has_errors(),
        "der/wrt spellings must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert!(decl.definitions.contains_key("der_x"));
    assert!(decl.definitions.contains_key("der_v"));
}

#[test]
fn implicit_mass_times_derivative_is_refused() {
    let source = "\
emath model Implicit:
    inputs:
        m: Float64
    state:
        v: Float64
    equations:
        m * derivative(v) = -v
";
    let result = check_source("implicit", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "implicit DAE left-hand side must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn function_cannot_use_equations() {
    let source = "\
emath function NotAModel:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    equations:
        derivative(x) = 0
";
    let result = check_source("fn-eq", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-KIND-010"),
        "equations on function must be E-KIND-010, got {codes:?}"
    );
}

#[test]
fn incomplete_rates_are_refused() {
    let source = "\
emath model Incomplete:
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
";
    let result = check_source("incomplete", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-NAME-025"),
        "missing der(v) must be E-NAME-025, got {codes:?}"
    );
}

#[test]
fn explicit_mass_spring_example_admits() {
    let source = include_str!(
        "../../../language/examples/numerical/explicit-mass-spring.emath"
    );
    let result = check_source("explicit-mass-spring", source);
    assert!(
        !result.diagnostics.has_errors(),
        "A2 mass-spring example must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert_eq!(decl.kind_label, "model");
    assert!(decl.definitions.contains_key("der_x"));
    assert!(decl.definitions.contains_key("der_v"));
}

#[test]
fn empty_model_is_refused() {
    let source = "\
emath model Empty:
    inputs:
        x: Float64
";
    let result = check_source("empty-model", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-KIND-011"),
        "empty model must be E-KIND-011, got {codes:?}"
    );
}
