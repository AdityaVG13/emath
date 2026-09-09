//! L0 scratch grammar: expressions, plot, solve, convert without declarations.

use emath_core::limits::Limits;
use emath_core::tree::{Item, StmtKind};
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
use emath_syntax::{expand_scratch, parse_str};
use std::collections::BTreeMap;

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

fn first_decl<'a>(tree: &'a emath_core::tree::SyntaxTree) -> &'a emath_core::tree::Declaration {
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    decl
}

use emath_test_harness::{Probe, boot};

#[test]
fn scratch_expressions() {
    boot();
    let mut probe = Probe::new("L0 scratch grammar: expressions, plot, solve, convert without declarations.");
    probe.case("two_plus_two_parses_as_implicit_function", |p| {
    let f0 = p.failures().len();

    let tree = parse_ok(p, "2+2\n");
    let decl = first_decl(&tree);
    p.demand("1", (decl.name) == ("Scratch"), format!("expected {:?}, got {:?}", ("Scratch"), (decl.name)));
    if p.failures().len() != f0 { return; }
    p.demand("2",decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "definitions")
        ), format!(
        "L0 must lower to definitions, got {:?}",
        decl.body
    ));
    if p.failures().len() != f0 { return; }

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("l0-two-plus-two", "2+2\n");
    p.demand("3",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let report = emath_exec_ir::runner::run_package(&checked.package);
    p.eq("4", report.declarations[0].tests[0].definitions.get("result"), Some(&Value::F64(4.0)));

    });
    probe.case("two_plus_two_example_file_parses", |p| {

    let source = include_str!("../../../tests/fixtures/language/intro/scratch.emath");
    let _tree = parse_ok(p, source);

    });
    probe.case("plot_solve_convert_expand", |p| {
    let f0 = p.failures().len();

    let plot = expand_scratch("plot sin(x) on -3.14..3.14\n");
    p.demand("1",plot.rewritten(), format!( "plot must wrap"));
    if p.failures().len() != f0 { return; }
    p.demand("2",plot.expanded.contains("sin(x)"), format!( "{}", plot.expanded));
    if p.failures().len() != f0 { return; }
    p.demand("3",plot.expanded.contains("emath function Scratch:"), format!(
        "{}",
        plot.expanded
    ));
    if p.failures().len() != f0 { return; }

    let solve = expand_scratch("solve x^2 = 2 over Real\n");
    p.demand("4",solve.rewritten(), stringify!(solve.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("5",solve.expanded.contains("solve(residual) wrt x"), format!(
        "{}",
        solve.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("6",solve
            .notes
            .iter()
            .any(|note| note.inferred.contains("Real")), format!(
        "{:?}",
        solve.notes
    ));
    if p.failures().len() != f0 { return; }

    let convert = expand_scratch("convert 1 km to m\n");
    p.demand("7",convert.rewritten(), stringify!(convert.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("8",convert.expanded.contains("(1 km) / (1 m)"), format!(
        "{}",
        convert.expanded
    ));
    if p.failures().len() != f0 { return; }

    for (name, source, given, expected, tolerance) in [
        (
            "plot",
            "plot sin(x) on -3.14..3.14\n",
            BTreeMap::from([("x".to_string(), Value::F64(0.0))]),
            0.0,
            0.0,
        ),
        (
            "solve",
            "solve x^2 = 2 over Real\n",
            BTreeMap::from([("x".to_string(), Value::F64(1.0))]),
            2.0_f64.sqrt(),
            1e-10,
        ),
        (
            "convert",
            "convert 1 km to m\n",
            BTreeMap::new(),
            1000.0,
            0.0,
        ),
    ] {
        let mut session = CompilerSession::new(Limits::default());
        let checked = session.check_owned(name, source);
        p.demand("9",!checked.diagnostics.has_errors(), format!(
            "{name}: {:?}",
            checked.diagnostics.errors().collect::<Vec<_>>()
        ));
        if p.failures().len() != f0 { return; }
        let report = emath_exec_ir::runner::run_package_with_given(&checked.package, Some(&given));
        let value = report.declarations[0].tests[0]
            .definitions
            .values()
            .last()
            .expect("intent computes a result");
        let Value::F64(actual) = value else {
            panic!("{name} must compute a scalar, got {value:?}");
        };
        p.demand("10",(actual - expected).abs() <= tolerance, format!(
            "{name}: expected {expected}, got {actual}"
        ));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.case("mix_scratch_and_declaration_is_e_syn_141", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/scratch_expressions.emath");
    p.demand("1",has_error(source, "E-SYN-141"), format!(
        "mixed scratch + declaration must refuse with E-SYN-141"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("junk_words_are_e_syn_145_not_a_silent_function", |p| {
    let f0 = p.failures().len();

    p.demand("1",has_error("this is not emath at all\n", "E-SYN-145"), format!(
        "non-expression scratch must refuse"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
