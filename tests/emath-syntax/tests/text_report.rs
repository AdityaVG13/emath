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
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("text-report", source)
}

use emath_test_harness::{Probe, boot};

#[test]
fn text_report() {
    boot();
    let mut probe = Probe::new("Executable `core::text` and pure deterministic report construction.");
    probe.case("text_unicode_and_reports_execute_deterministically", |p| {
    let f0 = p.failures().len();

    let package = checked(REPORT);
    p.demand("1",!package.diagnostics.has_errors(), format!(
        "{:?}",
        package.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let first = run_package(&package.package);
    let second = run_package(&package.package);
    let definitions = &first.declarations[0].tests[0].definitions;
    p.eq("2", definitions.get("normalized"), Some(&Value::Text("Café".to_string())));
    p.eq("3", definitions.get("code_points"), Some(&Value::I64(4)));
    p.eq("4", definitions.get("markdown"), Some(&Value::Text(
            "# Experiment\n\n## Results\n\nx = 4\n".to_string()
        )));
    p.eq("5", definitions.get("latex"), Some(&Value::Text(
            "\\section{Experiment}\n\\subsection{Results}\nx = 4\n".to_string()
        )));
    p.eq("6", &first.declarations[0].tests[0].definitions, &second.declarations[0].tests[0].definitions);

    });
    probe.case("nfc_equivalent_text_has_the_same_meaning_identity", |p| {
    let f0 = p.failures().len();

    let decomposed = checked("emath function T:\n    definitions:\n        x = \"Cafe\u{301}\"\n");
    let composed = checked("emath function T:\n    definitions:\n        x = \"Café\"\n");
    p.demand("1",!decomposed.diagnostics.has_errors(), stringify!(!decomposed.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.demand("2",!composed.diagnostics.has_errors(), stringify!(!composed.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.eq("3", decomposed.package.meaning_id(&[]).expect("decomposed"), composed.package.meaning_id(&[]).expect("composed"));
;

    });
    probe.case("report_side_effect_operation_refuses", |p| {
    let f0 = p.failures().len();

    let result = checked(
        "emath function BadReport:\n    definitions:\n        report = document(\"Title\", section(\"Body\", \"value\"))\n        written = render_file(report, \"out.md\")\n",
    );
    p.demand("1",result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-TYPE-003"
                && diagnostic.message.contains("render_file")), format!(
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
