//! Level 2 head-args: `emath function name(args) -> T:`.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::{Item, TypeKind};
use emath_syntax::formatter::format;
use emath_syntax::{parse_lossless, parse_str};

const SQUARE: &str = "\
emath function square(x: Float64) -> Float64:
    definitions:
        square = x * x
";

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
            diagnostics.errors().map(|error| error.code).collect::<Vec<_>>()
        ),
    );
    tree
}

use emath_test_harness::{Probe, boot};

#[test]
fn head_args() {
    boot();
    let mut probe = Probe::new("Level 2 head-args: `emath function name(args) -> T:`.");
    probe.case("head_args_stateless_function_parses", |p| {
    let f0 = p.failures().len();

    let tree = parse_ok(p, SQUARE);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    let signature = decl
        .signature
        .as_ref()
        .expect("head-args must populate Declaration.signature");
    p.eq("1", signature.params.len(), 1);
    p.demand("2", (signature.params[0].name) == ("x"), format!("expected {:?}, got {:?}", ("x"), (signature.params[0].name)));
    if p.failures().len() != f0 { return; }
    p.demand("3",signature.ret.is_some(), stringify!(signature.ret.is_some()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("untyped_head_args_store_infer_marker", |p| {
    let f0 = p.failures().len();

    let tree = parse_ok(p, 
        "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n",
    );
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    let signature = decl.signature.as_ref().expect("untyped head-args parse");
    p.eq("1", signature.params.len(), 1);
    p.demand("2", (signature.params[0].name) == ("x"), format!("expected {:?}, got {:?}", ("x"), (signature.params[0].name)));
    if p.failures().len() != f0 { return; }
    p.demand("3",matches!(
            &signature.params[0].ty.kind,
            TypeKind::Path { segments, .. } if segments.last().map(String::as_str) == Some("Infer")
        ), format!(
        "untyped head-arg must store the Infer marker, got {:?}",
        signature.params[0].ty
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("head_args_mixed_with_inputs_refused_e_syn_122", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function square(x: Float64) -> Float64:
    inputs:
        x: Float64
    definitions:
        square = x * x
";
    p.demand("1",has_error(source, "E-SYN-122"), format!(
        "head-args + inputs: must refuse with E-SYN-122"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("head_return_mixed_with_outputs_refused_e_syn_122", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function square(x: Float64) -> Float64:
    outputs:
        square: Float64
    definitions:
        square = x * x
";
    p.demand("1",has_error(source, "E-SYN-122"), format!(
        "-> T + outputs: must refuse with E-SYN-122"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("head_args_on_stateful_function_refused_e_syn_123", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function square(x: Float64) -> Float64:
    state:
        s: Float64
    definitions:
        square = x * x
";
    p.demand("1",has_error(source, "E-SYN-123"), format!(
        "head-args + state: must refuse with E-SYN-123"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("head_args_on_policy_refused_e_syn_123", |p| {
    let f0 = p.failures().len();

    let source = "\
emath policy Scorer(x: Float64) -> Float64:
    definitions:
        Scorer = x
";
    p.demand("1",has_error(source, "E-SYN-123"), format!(
        "head-args on policy must refuse with E-SYN-123"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("head_args_formatter_round_trips_canonically", |p| {
    let f0 = p.failures().len();

    let parsed = parse_lossless(SQUARE, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!( "fixture must parse"));
    if p.failures().len() != f0 { return; }
    let once = format(&parsed.tree, &parsed.comments);
    p.demand("2",once.contains("emath function square(x: Float64) -> Float64:"), format!(
        "canonical head must keep args: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!once.contains("inputs:"), format!(
        "formatter must not expand head-args into inputs:: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("4", format(
            &parse_lossless(&once, FileId(0), &Limits::default()).tree,
            &[]
        ), once);

    });
    probe.case("untyped_head_args_format_without_infer", |p| {
    let f0 = p.failures().len();

    let source = "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!( "fixture must parse"));
    if p.failures().len() != f0 { return; }
    let once = format(&parsed.tree, &parsed.comments);
    p.demand("2",once.contains("emath function square(x) -> Float64:"), format!(
        "Infer marker must be omitted: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!once.contains("Infer"), format!(
        "formatter must not print Infer: {once}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("empty_example_body_parses_as_worked_example", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function TwentyOne:
    outputs:
        y: Float64
    definitions:
        y = 3 * 7
    tests:
        example <worked>:
        example named:
";
    let tree = parse_ok(p, source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    let tests = decl.body.iter().find_map(|stmt| match &stmt.kind {
        emath_core::tree::StmtKind::Section(section) if section.name == "tests" => {
            Some(&section.suite.statements)
        }
        _ => None,
    });
    let tests = tests.expect("tests section");
    p.eq("1", tests.len(), 2);
    for stmt in tests {
        let emath_core::tree::StmtKind::Section(example) = &stmt.kind else {
            panic!("expected example section, got {:?}", stmt.kind);
        };
        p.demand("2", (example.name) == ("example"), format!("expected {:?}, got {:?}", ("example"), (example.name)));
        if p.failures().len() != f0 { return; }
        p.demand("3",example.suite.statements.is_empty(), format!(
            "empty example body stays empty, got {:?}",
            example.suite.statements
        ));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.case("empty_definitions_still_refuses_e_syn_112", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Branch:
    definitions:
";
    p.demand("1",has_error(source, "E-SYN-112"), format!(
        "empty definitions: must still require an indented block"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
