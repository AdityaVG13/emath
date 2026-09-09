//! L2 named-declaration shorthand (`emath function Name:`).

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
fn named_declarations() {
    boot();
    let mut probe = Probe::new("L2 named-declaration shorthand (`emath function Name:`).");
    probe.case("named_shorthand_lowers_to_definitions", |p| {
    let f0 = p.failures().len();

    let source = "emath function Square:\n    y = x^2\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), format!(
        "L2 must rewrite: {}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2", (expansion.level().as_str()) == ("L2"), format!("expected {:?}, got {:?}", ("L2"), (expansion.level().as_str())));
    if p.failures().len() != f0 { return; }
    let again = expand_scratch(&expansion.expanded);
    p.demand("3",!again.rewritten(), format!(
        "L2 product must be Canonical: {}",
        again.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4", (again.level().as_str()) == ("canonical"), format!("expected {:?}, got {:?}", ("canonical"), (again.level().as_str())));
    if p.failures().len() != f0 { return; }
    p.demand("5",expansion.expanded.contains("emath function Square:"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("6",expansion.expanded.contains("definitions:"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("7",expansion.expanded.contains("y = x^2"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    let tree = parse_ok(p, source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    p.demand("8", (decl.name) == ("Square"), format!("expected {:?}, got {:?}", ("Square"), (decl.name)));
    if p.failures().len() != f0 { return; }
    p.demand("9",decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "definitions")
        ), format!(
        "L2 must lower to definitions, got {:?}",
        decl.body
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l2_example_file_parses", |p| {
    let f0 = p.failures().len();

    let source = "emath function Square:\n    y = x^2\n    example x = 3\n";
    let tree = parse_ok(p, source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    p.demand("1", (decl.name) == ("Square"), format!("expected {:?}, got {:?}", ("Square"), (decl.name)));
    if p.failures().len() != f0 { return; }

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("l2-square", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let report = emath_exec_ir::runner::run_package(&checked.package);
    p.eq("3", report.declarations[0].tests[0].outputs.get("y"), Some(&Value::F64(9.0)));

    });
    probe.case("contracted_l3_is_not_rewritten", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/hello-square.emath");
    let expansion = expand_scratch(source);
    p.demand("1",!expansion.rewritten(), format!( "L3 must stay identity"));
    if p.failures().len() != f0 { return; }
    p.demand("2", (expansion.level().as_str()) == ("canonical"), format!("expected {:?}, got {:?}", ("canonical"), (expansion.level().as_str())));
    if p.failures().len() != f0 { return; }
    let again = expand_scratch(&expansion.expanded);
    p.eq("3", &again.expanded, &expansion.expanded);
    p.demand("4",!again.rewritten(), stringify!(!again.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("5", (again.level().as_str()) == ("canonical"), format!("expected {:?}, got {:?}", ("canonical"), (again.level().as_str())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("bodyless_named_declaration_is_e_syn_143", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/l2_named_declaration_bodyless.emath");
    p.demand("1",has_error(source, "E-SYN-143"), format!(
        "bodyless L2 must refuse with E-SYN-143, not wrap as L0"
    ));
    if p.failures().len() != f0 { return; }
    let expansion = expand_scratch(source);
    p.demand("2",!expansion.rewritten(), format!(
        "bodyless L2 must not become Scratch"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!expansion.expanded.contains("emath function Scratch:"), stringify!(!expansion.expanded.contains("emath function Scratch:")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("conflicting_signature_is_e_syn_149", |p| {
    let f0 = p.failures().len();

    let source =
        include_str!("../../../tests/invalid/l2_named_declaration_signature_conflict.emath");
    p.demand("1",has_error(source, "E-SYN-149"), format!(
        "header `n` vs body `x` must refuse, not coerce"
    ));
    if p.failures().len() != f0 { return; }
    let expansion = expand_scratch(source);
    p.demand("2",!expansion.rewritten(), format!( "conflicting L2 must not rewrite"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("matching_head_args_still_expand", |p| {
    let f0 = p.failures().len();

    let source = "emath function Square(x: Float64):\n    y = x^2\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), format!(
        "matching head-args must still expand: {}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",expansion.expanded.contains("definitions:"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!has_error(source, "E-SYN-149"), format!(
        "matching names must not look like a signature conflict"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("cannot_infer_domain_without_hole_is_e_syn_150", |p| {
    let f0 = p.failures().len();

    let source =
        include_str!("../../../tests/invalid/l2_named_declaration_cannot_infer_domain.emath");
    p.demand("1",has_error(source, "E-SYN-150"), format!(
        "unknown callee `mystery` must refuse, not become a silent input"
    ));
    if p.failures().len() != f0 { return; }
    let expansion = expand_scratch(source);
    p.demand("2",!expansion.rewritten(), format!( "unknown-callee L2 must not rewrite"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("hole_for_unknown_callee_is_admitted", |p| {
    let f0 = p.failures().len();

    let source = "emath function Mystery:\n    mystery = ?\n    y = mystery(x)\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), format!(
        "a hole for `mystery` must admit the L2 body: {}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!has_error(source, "E-SYN-150"), format!(
        "declared hole is the domain, not a silent inference"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
