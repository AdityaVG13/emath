//!: U8 string interpolation.
//!
//! Purity constraints keep interpolation evidence-grade: a hole may
//! carry ONLY a name or a dotted path (never an expression), with an
//! optional FIXED format spec (`{x:.3f}`; the spec grammar is
//! `.` digits `f`, nothing else), and `{{`/`}}` are escapes for literal
//! braces. An expression hole (`{f(x)}`) is a parse-time refusal, the
//! 's negative control.
//!
//! Failure-first: the refusal pins are RED until the validation lands
//! (today any string content parses unchecked); the valid-form pins are
//! the over-refusal guards that keep the validation from eating plain
//! strings (they discriminate against over-strict mutants).

use emath_core::tree::ExprKind;
use emath_syntax::install_source_parser;

fn parse_defn_string(content: &str) -> Result<String, String> {
    install_source_parser();
    let source = format!("emath function f:\n    definitions:\n        s = \"{content}\"\n");
    let (tree, diags) = emath_syntax::parse_str(&source);
    if diags.has_errors() {
        let codes = diags
            .errors()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(codes);
    }
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        return Err("no declaration".into());
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .ok_or("no definitions")?;
    for stmt in &defs.suite.statements {
        if let emath_core::tree::StmtKind::Assign { value, .. } = &stmt.kind {
            let ExprKind::Str(text) = &value.kind else {
                return Err(format!("expected Str, got {:?}", value.kind));
            };
            return Ok(text.clone());
        }
    }
    Err("no string assignment".into())
}

#[test]
fn plain_string_without_holes_unchanged() {
    // Over-refusal guard: validation must not eat plain strings.
    assert_eq!(parse_defn_string("hello").unwrap(), "hello");
}

#[test]
fn interpolated_template_parses_with_template_intact() {
    // The template value keeps the raw spelling; substitution is the
    // string-world's job (documented Phase 1 boundary).
    assert_eq!(parse_defn_string("x = {x}").unwrap(), "x = {x}");
}

#[test]
fn fixed_format_spec_admits() {
    // `{x:.3f}` — the fixed spec grammar is `.` digits `f`.
    assert_eq!(parse_defn_string("x = {x:.3f}").unwrap(), "x = {x:.3f}");
    assert_eq!(parse_defn_string("x = {x:.0f}").unwrap(), "x = {x:.0f}");
}

#[test]
fn dotted_path_hole_admits() {
    // Names AND dotted paths are pure: `{a.b.c}` is admissible.
    assert_eq!(parse_defn_string("y = {a.b.c}").unwrap(), "y = {a.b.c}");
}

#[test]
fn expression_hole_refuses() {
    // The negative control: `{f(x)}` is an expression in the
    // hole — purity refuses it at parse time.
    let error = parse_defn_string("r = {f(x)}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("names or paths"),
        "expression hole must refuse E-SYN-101 naming the purity rule, got {error}"
    );
}

#[test]
fn arithmetic_hole_refuses() {
    let error = parse_defn_string("r = {a + b}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("names or paths"),
        "arithmetic hole must refuse, got {error}"
    );
}

#[test]
fn indexing_hole_refuses() {
    // Indexing is not a name or path: `{x[0]}` is outside the purity
    // fence.
    let error = parse_defn_string("r = {x[0]}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("names or paths"),
        "indexing hole must refuse, got {error}"
    );
}

#[test]
fn unknown_format_spec_refuses() {
    // The spec is FIXED, not arbitrary: only `.Nf` exists.
    let error = parse_defn_string("r = {x:.3q}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("fixed format"),
        "unknown spec must refuse, got {error}"
    );
    let error = parse_defn_string("r = {x:3f}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("fixed format"),
        "spec without the dot must refuse, got {error}"
    );
    let error = parse_defn_string("r = {x:.f}").unwrap_err();
    assert!(
        error.contains("E-SYN-101") && error.contains("fixed format"),
        "spec without digits must refuse, got {error}"
    );
}

#[test]
fn unescaped_brace_refuses() {
    // A `{` that opens neither a valid hole nor an escape is refused
    // (stronger than a lint: an unparsed hole is never silently text).
    let error = parse_defn_string("set {2, 3}").unwrap_err();
    assert!(
        error.contains("E-SYN-101"),
        "junk hole must refuse, got {error}"
    );
    let error = parse_defn_string("open {").unwrap_err();
    assert!(
        error.contains("E-SYN-101"),
        "trailing lone brace must refuse, got {error}"
    );
}

#[test]
fn escaped_braces_admit() {
    // `{{` escapes to a literal `{`, `}}` to a literal `}`.
    assert_eq!(parse_defn_string("{{literal}}").unwrap(), "{{literal}}");
    assert_eq!(
        parse_defn_string("x = {x} {{raw}}").unwrap(),
        "x = {x} {{raw}}"
    );
}

#[test]
fn interpolation_evaluates_with_fixed_format_and_escaped_braces() {
    use emath_core::limits::Limits;
    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    install_source_parser();
    let source = "\
emath function report:
    inputs:
        x: Float64
    definitions:
        text = \"x = {x:.3f}, literal {{raw}}\"
";
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let checked = session.check_owned("runtime-string", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::from([("x".to_string(), Value::F64(1.23456))]),
        &BTreeMap::new(),
    )
    .expect("template evaluates");
    assert_eq!(
        values.get("text"),
        Some(&Value::Text("x = 1.235, literal {raw}".to_string()))
    );
}

#[test]
fn invalid_fixture_refuses_at_parse() {
    // The negative control as a fixture-shaped source.
    install_source_parser();
    let source = include_str!("../../../tests/invalid/string_interpolation_expression_hole.emath");
    let (_tree, diags) = emath_syntax::parse_str(source);
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("names or paths")),
        "fixture must refuse the expression hole, got {diags:?}"
    );
}
