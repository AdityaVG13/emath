//! nothing-returns-nothing (Vision law 2): a program whose expressions
//! have no evaluable world returns the symbolic form with its meaning
//! label attached instead of erroring; genuinely impossible math still
//! refuses typed.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

const UNBOUND_SQUARE: &str = "\
emath function UnboundSquare:
    inputs:
        x: Float64

    outputs:
        y: Float64
        k: Float64

    definitions:
        y = x * x
        k = 2.0
";

#[test]
fn unbound_program_returns_labeled_symbolic_form_instead_of_erroring() {
    let result = check("unbound-square", UNBOUND_SQUARE);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );

    let report = run_package(&result.package);
    let run = &report.declarations[0];
    assert_eq!(
        run.tests.len(),
        1,
        "the zero-test declaration still runs once"
    );
    let test = &run.tests[0];

    // The symbolic form carries its meaning label (nothing crosses the
    // exit unlabeled) and is not a refusal.
    assert_eq!(test.verdict.meaning_label(), Some("symbolic-only"));
    assert!(!test.verdict.is_refused(), "{test:?}");

    let forms = test.verdict.symbolic_forms().expect("forms attached");
    assert_eq!(forms.get("y").map(String::as_str), Some("x * x"));
    let holes = test.verdict.symbolic_holes().expect("holes attached");
    assert_eq!(holes.get("x").map(String::as_str), Some("Float64"));

    // Definitions with an evaluable world still compute alongside.
    assert_eq!(test.definitions.get("k"), Some(&Value::F64(2.0)));
    assert_eq!(test.outputs.get("k"), Some(&Value::F64(2.0)));
    assert!(
        !test.outputs.contains_key("y"),
        "the uncomputed output is not a naked number"
    );
    assert_eq!(report.summary.symbolic, 1);
}

#[test]
fn impossible_units_still_refuse_typed() {
    let source = include_str!("../../../language/examples/physics/newton-second.emath")
        .replace("force = mass * acceleration", "force = mass + acceleration");
    let result = check("unbound-unit-mismatch", &source);
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-UNIT-101"),
        "genuinely impossible math must still refuse typed"
    );
}
