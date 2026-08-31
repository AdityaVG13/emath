//! Sequences as values, structurally decreasing indexed recurrences, and
//! generating functions (B07+B33).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const FIB_RECURRENCE: &str = "\
emath function fib:
    inputs:
        n: Float64

    outputs:
        y: Float64

    definitions:
        fib[0] = 0
        fib[1] = 1
        fib[n] = fib[n-1] + fib[n-2]
        y = fib[n]

    tests:
        example <tenth>:
            given n = 10
            expect y == 55
";

const GENERATING_FUNCTION: &str = "\
emath function generating_coefficient:
    inputs:
        n: Float64

    outputs:
        y: Float64

    definitions:
        fibonacci = generating_function([0, 1], [1, 1], 64)
        y = coefficient(fibonacci, n)

    tests:
        example <coefficient>:
            given n = 10
            expect y == 55
";

const CONVOLUTION: &str = "\
emath function fibonacci_convolution:
    inputs:
        n: Float64

    outputs:
        y: Float64

    definitions:
        fibonacci = generating_function([0, 1], [1, 1], 64)
        square = convolution(fibonacci, fibonacci, 16)
        y = square[n]

    tests:
        example <coefficient>:
            given n = 5
            expect y == 10
";

fn evaluated_output(text: &str, name: &str, output: &str) -> Value {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(name, text);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    report.declarations[0].tests[0]
        .definitions
        .get(output)
        .unwrap_or_else(|| panic!("missing `{output}` in {report:#?}"))
        .clone()
}

#[test]
fn indexed_fibonacci_recurrence_evaluates() {
    assert_eq!(
        evaluated_output(FIB_RECURRENCE, "seq-fibonacci", "y"),
        Value::F64(55.0)
    );
}

#[test]
fn non_decreasing_recurrence_refuses() {
    let source = FIB_RECURRENCE.replace(
        "fib[n] = fib[n-1] + fib[n-2]",
        "fib[n] = fib[n+1] + fib[n-2]",
    );
    let errors = check(&source, "seq-nonterminating");
    assert!(
        errors
            .iter()
            .any(|error| error.starts_with("E-SEQ-TERMINATION")),
        "a forward self-reference must refuse termination checking: {errors:#?}"
    );
}

#[test]
fn generating_function_extracts_coefficients() {
    assert_eq!(
        evaluated_output(GENERATING_FUNCTION, "seq-generating", "y"),
        Value::F64(55.0)
    );
}

#[test]
fn generating_function_convolution_is_cauchy_product() {
    assert_eq!(
        evaluated_output(CONVOLUTION, "seq-convolution", "y"),
        Value::F64(10.0)
    );
}
