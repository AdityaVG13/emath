//! Sequences as values, structurally decreasing indexed recurrences, and
//! generating functions (B07+B33).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::session::CompilerSession;

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
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

fn evaluated_output(p: &mut Probe, text: &str, name: &str, output: &str) -> Value {
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(name, text);
    p.demand(
        "admit",
        !checked.diagnostics.has_errors(),
        format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()),
    );
    let report = run_package(&checked.package);
    report.declarations[0].tests[0]
        .definitions
        .get(output)
        .unwrap_or_else(|| panic!("missing `{output}` in {report:#?}"))
        .clone()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn sequences_recurrences() {
    boot();
    let mut probe = Probe::new("Sequences as values, structurally decreasing indexed recurrences, and generating functions (B07+B33).");
    probe.case("indexed_fibonacci_recurrence_evaluates", |p| {

    let y = evaluated_output(p, FIB_RECURRENCE, "seq-fibonacci", "y");
    p.eq("1", y, Value::F64(55.0));

    });
    probe.case("non_decreasing_recurrence_refuses", |p| {
    let f0 = p.failures().len();

    let source = FIB_RECURRENCE.replace(
        "fib[n] = fib[n-1] + fib[n-2]",
        "fib[n] = fib[n+1] + fib[n-2]",
    );
    let errors = check(&source, "seq-nonterminating");
    p.demand("1",errors
            .iter()
            .any(|error| error.starts_with("E-SEQ-TERMINATION")), format!(
        "a forward self-reference must refuse termination checking: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("generating_function_extracts_coefficients", |p| {

    let y = evaluated_output(p, GENERATING_FUNCTION, "seq-generating", "y");
    p.eq("1", y, Value::F64(55.0));

    });
    probe.case("generating_function_convolution_is_cauchy_product", |p| {

    let y = evaluated_output(p, CONVOLUTION, "seq-convolution", "y");
    p.eq("1", y, Value::F64(10.0));

    });
    probe.finish();
}
