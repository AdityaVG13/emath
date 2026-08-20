//! Level 2 head-args: `emath function name(args) -> T:`.

use emath_core::limits::Limits;
use emath_core::tree::{Item, TypeKind};
use emath_core::FileId;
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

fn parse_ok(text: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diagnostics) = parse_str(text);
    assert!(
        !diagnostics.has_errors(),
        "must parse cleanly, got {:?}",
        diagnostics
            .errors()
            .map(|error| error.code)
            .collect::<Vec<_>>()
    );
    tree
}

#[test]
fn head_args_stateless_function_parses() {
    let tree = parse_ok(SQUARE);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    let signature = decl
        .signature
        .as_ref()
        .expect("head-args must populate Declaration.signature");
    assert_eq!(signature.params.len(), 1);
    assert_eq!(signature.params[0].name, "x");
    assert!(signature.ret.is_some());
}

#[test]
fn untyped_head_args_store_infer_marker() {
    let tree = parse_ok(
        "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n",
    );
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    let signature = decl.signature.as_ref().expect("untyped head-args parse");
    assert_eq!(signature.params.len(), 1);
    assert_eq!(signature.params[0].name, "x");
    assert!(
        matches!(
            &signature.params[0].ty.kind,
            TypeKind::Path { segments, .. } if segments.last().map(String::as_str) == Some("Infer")
        ),
        "untyped head-arg must store the Infer marker, got {:?}",
        signature.params[0].ty
    );
}

#[test]
fn head_args_mixed_with_inputs_refused_e_syn_122() {
    let source = "\
emath function square(x: Float64) -> Float64:
    inputs:
        x: Float64
    definitions:
        square = x * x
";
    assert!(
        has_error(source, "E-SYN-122"),
        "head-args + inputs: must refuse with E-SYN-122"
    );
}

#[test]
fn head_return_mixed_with_outputs_refused_e_syn_122() {
    let source = "\
emath function square(x: Float64) -> Float64:
    outputs:
        square: Float64
    definitions:
        square = x * x
";
    assert!(
        has_error(source, "E-SYN-122"),
        "-> T + outputs: must refuse with E-SYN-122"
    );
}

#[test]
fn head_args_on_stateful_function_refused_e_syn_123() {
    let source = "\
emath function square(x: Float64) -> Float64:
    state:
        s: Float64
    definitions:
        square = x * x
";
    assert!(
        has_error(source, "E-SYN-123"),
        "head-args + state: must refuse with E-SYN-123"
    );
}

#[test]
fn head_args_on_policy_refused_e_syn_123() {
    let source = "\
emath policy Scorer(x: Float64) -> Float64:
    definitions:
        Scorer = x
";
    assert!(
        has_error(source, "E-SYN-123"),
        "head-args on policy must refuse with E-SYN-123"
    );
}

#[test]
fn head_args_formatter_round_trips_canonically() {
    let parsed = parse_lossless(SQUARE, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors(), "fixture must parse");
    let once = format(&parsed.tree, &parsed.comments);
    assert!(
        once.contains("emath function square(x: Float64) -> Float64:"),
        "canonical head must keep args: {once}"
    );
    assert!(
        !once.contains("inputs:"),
        "formatter must not expand head-args into inputs:: {once}"
    );
    assert_eq!(
        format(
            &parse_lossless(&once, FileId(0), &Limits::default()).tree,
            &[]
        ),
        once,
        "fmt(fmt(s)) must equal fmt(s)"
    );
}

#[test]
fn untyped_head_args_format_without_infer() {
    let source = "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors(), "fixture must parse");
    let once = format(&parsed.tree, &parsed.comments);
    assert!(
        once.contains("emath function square(x) -> Float64:"),
        "Infer marker must be omitted: {once}"
    );
    assert!(
        !once.contains("Infer"),
        "formatter must not print Infer: {once}"
    );
}

#[test]
fn cache_policy_example_parses_as_one_declaration() {
    let text = include_str!("../../../language/examples/integration/cache-policy.emath");
    let (tree, diagnostics) = parse_str(text);
    let names: Vec<String> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration(decl) => Some(decl.name.clone()),
            _ => None,
        })
        .collect();
    let sections: Vec<String> = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(decl) => Some(
                decl.body
                    .iter()
                    .filter_map(|stmt| match &stmt.kind {
                        emath_core::tree::StmtKind::Section(section) => Some(section.name.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        !diagnostics.has_errors(),
        "cache-policy.emath must parse; errors={:?} sections={sections:?}",
        diagnostics
            .errors()
            .map(|error| format!("{}@{}", error.code, error.primary.start))
            .collect::<Vec<_>>()
    );
    assert_eq!(names, vec!["AdaptiveCachePolicy".to_string()]);
    assert!(
        sections.contains(&"goals".to_string()),
        "declaration body must keep goals; sections={sections:?}"
    );
}

#[test]
fn empty_example_body_parses_as_worked_example() {
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
    let tree = parse_ok(source);
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
    assert_eq!(tests.len(), 2, "both empty examples must parse");
    for stmt in tests {
        let emath_core::tree::StmtKind::Section(example) = &stmt.kind else {
            panic!("expected example section, got {:?}", stmt.kind);
        };
        assert_eq!(example.name, "example");
        assert!(
            example.suite.statements.is_empty(),
            "empty example body stays empty, got {:?}",
            example.suite.statements
        );
    }
}

#[test]
fn empty_definitions_still_refuses_e_syn_112() {
    let source = "\
emath function Branch:
    definitions:
";
    assert!(
        has_error(source, "E-SYN-112"),
        "empty definitions: must still require an indented block"
    );
}
