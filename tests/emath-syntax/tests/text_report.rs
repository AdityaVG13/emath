//! Executable `core::text` and pure deterministic report construction.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;

const REPORT: &str = "\
emath function Report:
    definitions:
        normalized = nfc(\"Cafe\u{301}\")
        code_points = text_length(normalized)
        results = section(\"Results\", \"x = {code_points}\")
        report = document(\"Experiment\", results)
        markdown = render_markdown(report)
        latex = render_latex(report)

    tests:
        example <pure_render>:
            expect code_points == 4
";

fn checked(source: &str) -> emath_sema::CheckResult {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("text-report", source)
}

#[test]
fn text_unicode_and_reports_execute_deterministically() {
    let package = checked(REPORT);
    assert!(
        !package.diagnostics.has_errors(),
        "{:?}",
        package.diagnostics.errors().collect::<Vec<_>>()
    );
    let first = run_package(&package.package);
    let second = run_package(&package.package);
    let definitions = &first.declarations[0].tests[0].definitions;
    assert_eq!(
        definitions.get("normalized"),
        Some(&Value::Text("Café".to_string()))
    );
    assert_eq!(definitions.get("code_points"), Some(&Value::I64(4)));
    assert_eq!(
        definitions.get("markdown"),
        Some(&Value::Text(
            "# Experiment\n\n## Results\n\nx = 4\n".to_string()
        ))
    );
    assert_eq!(
        definitions.get("latex"),
        Some(&Value::Text(
            "\\section{Experiment}\n\\subsection{Results}\nx = 4\n".to_string()
        ))
    );
    assert_eq!(
        first.declarations[0].tests[0].definitions,
        second.declarations[0].tests[0].definitions
    );
}

#[test]
fn nfc_equivalent_text_has_the_same_meaning_identity() {
    let decomposed = checked("emath function T:\n    definitions:\n        x = \"Cafe\u{301}\"\n");
    let composed = checked("emath function T:\n    definitions:\n        x = \"Café\"\n");
    assert!(!decomposed.diagnostics.has_errors());
    assert!(!composed.diagnostics.has_errors());
    assert_eq!(
        decomposed.package.meaning_id(&[]).expect("decomposed"),
        composed.package.meaning_id(&[]).expect("composed")
    );
}

#[test]
fn report_side_effect_operation_refuses() {
    let result = checked(
        "emath function BadReport:\n    definitions:\n        report = document(\"Title\", section(\"Body\", \"value\"))\n        written = render_file(report, \"out.md\")\n",
    );
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-TYPE-003"
                && diagnostic.message.contains("render_file")),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
}
