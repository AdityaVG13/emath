//! First-ten-minutes corpus and beginner gate (bead emath-v9-06-2rdq.18).
//!
//! A new user must plot, solve, and evaluate without reading compiler
//! architecture — and without the compiler picking a hidden default for
//! them. Contracts:
//! - the first-ten-minutes example admits `check` and `expand --json` (L1);
//! - an unlabeled solve stays honest: the ledger labels the world
//!   candidates and chooses none (that is the beginner gate against
//!   silent defaults, not a refusal);
//! - a form that would REQUIRE a hidden default (bare `plot sin over
//!   0..6.28`: implicit function value + ambient range) is refused.

use emath_cli::{run, run_check, CliExit};
use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
use emath_syntax::exactness_ledger;
use std::path::PathBuf;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn cli(command: &str, rest: &[&str]) -> Vec<String> {
    std::iter::once(command.to_string())
        .chain(rest.iter().map(|argument| argument.to_string()))
        .collect()
}

#[test]
fn first_ten_minutes_corpus_admits_check_and_expand() {
    emath_syntax::install_source_parser();
    let path = repo_file("tests/fixtures/language/intro/first-ten-minutes.emath");
    let source = std::fs::read_to_string(&path).expect("example source");
    assert!(
        source.contains("plot sin(x)") && source.contains("solve x^2 = 2 over Real"),
        "the corpus file must carry the three beginner moves"
    );

    let (diagnostics, _, _) = run_check(&path);
    let errors: Vec<&str> = diagnostics
        .items()
        .iter()
        .filter(|item| item.severity == emath_core::Severity::Error && item.code.starts_with("E-"))
        .map(|item| item.code)
        .collect();
    assert!(
        errors.is_empty(),
        "first-ten-minutes corpus must admit check, got {errors:?}"
    );
    assert_eq!(run(&cli("check", &[&path.to_string_lossy()])), CliExit::Ok);
    assert_eq!(
        run(&cli("expand", &[&path.to_string_lossy(), "--json"])),
        CliExit::Ok
    );

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("first-ten-minutes", &source);
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let definitions = &report.declarations[0].tests[0].definitions;
    assert_eq!(definitions.get("y"), Some(&Value::F64(4.0)));
    let Value::F64(root) = definitions
        .get("solve_result")
        .expect("solve intent computes")
    else {
        panic!("solve result must be scalar");
    };
    assert!((root - 2.0_f64.sqrt()).abs() < 1e-10, "{root}");
    let Value::F64(plotted) = definitions
        .get("plot_result")
        .expect("plot intent evaluates")
    else {
        panic!("plot result must be scalar");
    };
    assert!((plotted - 2.0_f64.sin()).abs() < 1e-12, "{plotted}");
}

/// Boundary: an unlabeled solve does not silently pick a world. The ledger
/// keeps the domain OPEN and labels the candidates; `over Real` is what
/// declares it.
#[test]
fn unlabeled_solve_stays_labeled_not_silently_chosen() {
    emath_syntax::install_source_parser();
    let unlabeled = exactness_ledger("solve x^2 = 2\n");
    let domain = unlabeled
        .entries
        .iter()
        .find(|entry| entry.dimension.as_str() == "domain")
        .expect("domain ledger row");
    assert_eq!(domain.status.as_str(), "inferred", "no silent world choice");
    assert_ne!(domain.status.as_str(), "declared");
    assert!(
        domain.rationale.contains("Real") && domain.rationale.contains("label"),
        "candidates must be labeled in the ledger, got: {}",
        domain.rationale
    );

    let declared = exactness_ledger("solve x^2 = 2 over Real\n");
    let domain = declared
        .entries
        .iter()
        .find(|entry| entry.dimension.as_str() == "domain")
        .expect("domain ledger row");
    assert_eq!(
        domain.status.as_str(),
        "declared",
        "`over Real` must declare the domain"
    );
}

/// Negative control: the bare-function plot requires hidden defaults and
/// refuses with the pinned code.
#[test]
fn hidden_default_form_is_refused() {
    emath_syntax::install_source_parser();
    let fixture_path = repo_file("tests/invalid/first_ten_minutes.emath");
    let fixture = std::fs::read_to_string(&fixture_path).expect("fixture");
    assert!(
        fixture.contains("expect: E-TYPE-002"),
        "fixture must pin E-TYPE-002"
    );

    let (diagnostics, _, _) = run_check(&fixture_path);
    let codes: Vec<&str> = diagnostics
        .items()
        .iter()
        .filter(|item| item.severity == emath_core::Severity::Error)
        .map(|item| item.code)
        .collect();
    assert!(
        codes.contains(&"E-TYPE-002"),
        "hidden-default plot must refuse E-TYPE-002, got {codes:?}"
    );
    assert_eq!(
        run(&cli("check", &[&fixture_path.to_string_lossy()])),
        CliExit::Refused
    );
}
