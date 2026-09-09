//! L1 guided relationships and examples (`y = x^2 + 4`, `example x = 3`).

use emath_core::limits::Limits;
use emath_core::tree::{Item, StmtKind};
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
use emath_syntax::{expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

fn parse_ok(p: &mut Probe, text: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diagnostics) = parse_str(text);
    p.demand(
        "parse_ok",
        !diagnostics.has_errors(),
        format!(
            "must parse cleanly, got {:?}",
            diagnostics
                .errors()
                .map(|error| format!("{} {}", error.code, error.message))
                .collect::<Vec<_>>()
        ),
    );
    tree
}

use emath_test_harness::{Probe, boot};

#[test]
fn guided_relationships() {
    boot();
    let mut probe = Probe::new("L1 guided relationships and examples (`y = x^2 + 4`, `example x = 3`).");
    probe.case("relationship_plus_example_infers_input_and_tests", |p| {
    let f0 = p.failures().len();

    let source = "y = x^2 + 4\nexample x = 3\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), stringify!(expansion.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("2", (expansion.level().as_str()) == ("L1"), format!("expected {:?}, got {:?}", ("L1"), (expansion.level().as_str())));
    if p.failures().len() != f0 { return; }
    p.demand("3",expansion.expanded.contains("inputs:"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",expansion.expanded.contains("given x = 3"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    let tree = parse_ok(p, source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    p.demand("5",decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "tests")
        ), format!(
        "L1 example must lower to tests:, got {:?}",
        decl.body
    ));
    if p.failures().len() != f0 { return; }

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("l1-relationship", source);
    p.demand("6",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let report = emath_exec_ir::runner::run_package(&checked.package);
    p.eq("7", report.declarations[0].tests[0].outputs.get("y"), Some(&Value::F64(13.0)));

    });
    probe.case("l1_example_file_parses", |p| {

    let source = include_str!("../../../tests/fixtures/language/intro/scratch.emath");
    let _tree = parse_ok(p, source);

    });
    probe.case("conflicting_example_types_are_e_syn_142", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/guided_relationships.emath");
    p.demand("1",has_error(source, "E-SYN-142"), format!(
        "conflicting example types must refuse with E-SYN-142"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
