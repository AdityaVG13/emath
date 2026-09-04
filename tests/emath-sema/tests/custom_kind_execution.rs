//! Custom-kind execution story
//! (CAPABILITY "emath kind | partial (schema validation) | no").
//!
//! Contracts:
//! - **Kind definitions register**: `emath kind Gauge:` with a valid
//!   `schema:`/`lower:` body checks clean (schema validated) and
//!   leaves a registered marker, so later applications get an honest
//!   story;
//! - **Function-shaped kind applications execute** through the same typed
//!   definitions, reference VM, and backend path as their declared base kind;
//! - **Undefined application names stay generic**: applying a kind that
//!   was never defined keeps the plain Phase-1-subset refusal;
//! - ordinary kinds admit unchanged.
//!
//! Docs of record: CAPABILITY.md `emath kind` row + ch.8 "Execution
//! story (today)".

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
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
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const KIND_DEFINED_AND_APPLIED: &str = "\
emath kind Gauge:
    extends function

    schema:
        require section inputs

    lower:
        model.inputs = section.inputs

emath Gauge HalfGauge:
    inputs:
        x: Float64

    definitions:
        y = x / 2

    tests:
        example <half>:
            given x = 8
            expect y == 4
";

const KIND_UNDEFINED_APPLICATION: &str = "\
emath Never HalfGauge:
    inputs:
        x: Float64

    definitions:
        y = x / 2
";

const KIND_DEFINITION_ALONE: &str = "\
emath kind Gauge:
    extends function

    schema:
        require section inputs

    lower:
        model.inputs = section.inputs
";

const PLAIN_FUNCTION: &str = "\
emath function PlainFn:
    inputs:
        x: Float64

    definitions:
        y = x / 2
";

#[test]
fn defined_function_kind_application_executes() {
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
    let checked = session.check_owned("kind-apply", KIND_DEFINED_AND_APPLIED);
    let errors: Vec<_> = checked.diagnostics.errors().collect();
    assert!(
        errors.is_empty(),
        "defined function-shaped kind must admit; got: {errors:#?}"
    );
    let report = run_package(&checked.package);
    let half_gauge = report
        .declarations
        .iter()
        .find(|declaration| declaration.name == "HalfGauge")
        .expect("custom-kind application runs");
    assert_eq!(
        half_gauge.tests[0].definitions.get("y"),
        Some(&Value::F64(4.0))
    );
}

#[test]
fn undefined_application_keeps_generic_refusal() {
    let errors = check(KIND_UNDEFINED_APPLICATION, "kind-undefined");
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("E-KIND-100") && e.contains("outside the Phase 1 subset")),
        "an UNDEFINED application keeps the generic Phase-1-subset \
         refusal; got: {errors:#?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("NO RUN PATH")),
        "the no-run-path story must not fire for undefined kinds; got: \
         {errors:#?}"
    );
}

#[test]
fn valid_kind_definition_checks_clean() {
    let errors = check(KIND_DEFINITION_ALONE, "kind-def");
    assert!(
        errors.is_empty(),
        "a valid `emath kind` definition (schema + lower) checks clean; \
         got: {errors:#?}"
    );
}

#[test]
fn plain_functions_admit_unchanged() {
    let errors = check(PLAIN_FUNCTION, "kind-plain-guard");
    assert!(
        errors.is_empty(),
        "the kind-execution story must not affect ordinary functions; \
         got: {errors:#?}"
    );
}
