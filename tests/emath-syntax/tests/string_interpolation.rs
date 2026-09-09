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

fn parse_defn_string(content: &str) -> Result<String, String> {
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

use emath_test_harness::{Probe, boot};

#[test]
fn string_interpolation() {
    boot();
    let mut probe = Probe::new("U8 string interpolation. Purity constraints keep interpolation evidence-grade: a hole may carry ONLY a name or a dotted path (never an expression),");
    probe.case("plain_string_without_holes_unchanged", |p| {
    let f0 = p.failures().len();

    // Over-refusal guard: validation must not eat plain strings.
    p.demand("1", (parse_defn_string("hello").unwrap()) == ("hello"), format!("expected {:?}, got {:?}", ("hello"), (parse_defn_string("hello").unwrap())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("interpolated_template_parses_with_template_intact", |p| {
    let f0 = p.failures().len();

    // The template value keeps the raw spelling; substitution is the
    // string-world's job (documented Phase 1 boundary).
    p.demand("1", (parse_defn_string("x = {x}").unwrap()) == ("x = {x}"), format!("expected {:?}, got {:?}", ("x = {x}"), (parse_defn_string("x = {x}").unwrap())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("fixed_format_spec_admits", |p| {
    let f0 = p.failures().len();

    // `{x:.3f}` — the fixed spec grammar is `.` digits `f`.
    p.demand("1", (parse_defn_string("x = {x:.3f}").unwrap()) == ("x = {x:.3f}"), format!("expected {:?}, got {:?}", ("x = {x:.3f}"), (parse_defn_string("x = {x:.3f}").unwrap())));
    if p.failures().len() != f0 { return; }
    p.demand("2", (parse_defn_string("x = {x:.0f}").unwrap()) == ("x = {x:.0f}"), format!("expected {:?}, got {:?}", ("x = {x:.0f}"), (parse_defn_string("x = {x:.0f}").unwrap())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("dotted_path_hole_admits", |p| {
    let f0 = p.failures().len();

    // Names AND dotted paths are pure: `{a.b.c}` is admissible.
    p.demand("1", (parse_defn_string("y = {a.b.c}").unwrap()) == ("y = {a.b.c}"), format!("expected {:?}, got {:?}", ("y = {a.b.c}"), (parse_defn_string("y = {a.b.c}").unwrap())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("expression_hole_refuses", |p| {
    let f0 = p.failures().len();

    // The negative control: `{f(x)}` is an expression in the
    // hole — purity refuses it at parse time.
    let error = parse_defn_string("r = {f(x)}").unwrap_err();
    p.demand("1",error.contains("E-SYN-101") && error.contains("names or paths"), format!(
        "expression hole must refuse E-SYN-101 naming the purity rule, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("arithmetic_hole_refuses", |p| {
    let f0 = p.failures().len();

    let error = parse_defn_string("r = {a + b}").unwrap_err();
    p.demand("1",error.contains("E-SYN-101") && error.contains("names or paths"), format!(
        "arithmetic hole must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("indexing_hole_refuses", |p| {
    let f0 = p.failures().len();

    // Indexing is not a name or path: `{x[0]}` is outside the purity
    // fence.
    let error = parse_defn_string("r = {x[0]}").unwrap_err();
    p.demand("1",error.contains("E-SYN-101") && error.contains("names or paths"), format!(
        "indexing hole must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unknown_format_spec_refuses", |p| {
    let f0 = p.failures().len();

    // The spec is FIXED, not arbitrary: only `.Nf` exists.
    let error = parse_defn_string("r = {x:.3q}").unwrap_err();
    p.demand("1",error.contains("E-SYN-101") && error.contains("fixed format"), format!(
        "unknown spec must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }
    let error = parse_defn_string("r = {x:3f}").unwrap_err();
    p.demand("2",error.contains("E-SYN-101") && error.contains("fixed format"), format!(
        "spec without the dot must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }
    let error = parse_defn_string("r = {x:.f}").unwrap_err();
    p.demand("3",error.contains("E-SYN-101") && error.contains("fixed format"), format!(
        "spec without digits must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unescaped_brace_refuses", |p| {
    let f0 = p.failures().len();

    // A `{` that opens neither a valid hole nor an escape is refused
    // (stronger than a lint: an unparsed hole is never silently text).
    let error = parse_defn_string("set {2, 3}").unwrap_err();
    p.demand("1",error.contains("E-SYN-101"), format!(
        "junk hole must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }
    let error = parse_defn_string("open {").unwrap_err();
    p.demand("2",error.contains("E-SYN-101"), format!(
        "trailing lone brace must refuse, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("escaped_braces_admit", |p| {
    let f0 = p.failures().len();

    // `{{` escapes to a literal `{`, `}}` to a literal `}`.
    p.demand("1", (parse_defn_string("{{literal}}").unwrap()) == ("{{literal}}"), format!("expected {:?}, got {:?}", ("{{literal}}"), (parse_defn_string("{{literal}}").unwrap())));
    if p.failures().len() != f0 { return; }
    p.demand("2", (parse_defn_string("x = {x} {{raw}}").unwrap()) == ("x = {x} {{raw}}"), format!("expected {:?}, got {:?}", ("x = {x} {{raw}}"), (parse_defn_string("x = {x} {{raw}}").unwrap())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("interpolation_evaluates_with_fixed_format_and_escaped_braces", |p| {
    let f0 = p.failures().len();

    use emath_core::limits::Limits;
    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    let source = "\
emath function report:
    inputs:
        x: Float64
    definitions:
        text = \"x = {x:.3f}, literal {{raw}}\"
";
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let checked = session.check_owned("runtime-string", source);
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::from([("x".to_string(), Value::F64(1.23456))]),
        &BTreeMap::new(),
    )
    .expect("template evaluates");
    p.eq("2", values.get("text"), Some(&Value::Text("x = 1.235, literal {raw}".to_string())));

    });
    probe.case("invalid_fixture_refuses_at_parse", |p| {
    let f0 = p.failures().len();

    // The negative control as a fixture-shaped source.
    let source = include_str!("../../../tests/invalid/string_interpolation_expression_hole.emath");
    let (_tree, diags) = emath_syntax::parse_str(source);
    p.demand("1",diags
            .errors()
            .any(|error| error.code == "E-SYN-101" && error.message.contains("names or paths")), format!(
        "fixture must refuse the expression hole, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
