//! Set literals (B01+U3) failure-first parse tests.
//!
//! Contracts (each failed against the pre-parser, which refused every
//! `{` in expression position with E-SYN-110):
//! - `{2, 3, 5}` parses as a set literal (`ExprKind::Set`).
//! - `{n in 0..100 if is_prime(n)}` parses as a set comprehension.
//! - `Point:{x: 1.0, y: 2.0}` parses as an inline record literal.
//! - `v in s` parses as the membership operator (`BinaryOp::In`); binder
//!   position (`sum n in 0..10`) stays a binder — X13 charter disjointness.
//! - Bare `{x: 1}` (record spelling without a path prefix) is ambiguous:
//!   refuses with pinned code `E-SYN-154`, never silently a set.
//!
//! Phase B (eval: `Value::Set`, `TypeNode::Set`, comprehension lowering)
//! lands after emath-ir; see internal/status/compliance/.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, BinderKind, ExprKind, Item, StmtKind};
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
use emath_syntax::formatter::format;
use emath_syntax::parse_lossless;
use emath_syntax::parse_str;

fn def_expr<'a>(
    tree: &'a emath_core::tree::SyntaxTree,
    name: &str,
) -> Option<&'a emath_core::tree::Expr> {
    let item = tree.items.first()?;
    let Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "definitions" {
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Assign { target, value }
                        if target.segments.first().is_some_and(|s| s == name) =>
                    {
                        return Some(value);
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn parse_definition(p: &mut Probe, source: &str, name: &str) -> emath_core::tree::Expr {
    let (tree, diagnostics) = parse_str(source);
    p.demand(
        "parse",
        !diagnostics.has_errors(),
        format!(
            "expected clean parse, got {:?}",
            diagnostics
                .errors()
                .map(|error| (error.code, error.message.clone()))
                .collect::<Vec<_>>()
        ),
    );
    def_expr(&tree, name)
        .cloned()
        .unwrap_or_else(|| panic!("definition `{name}` not found"))
}

use emath_test_harness::{Probe, boot};

#[test]
fn sets() {
    boot();
    let mut probe = Probe::new("Set literals (B01+U3) failure-first parse tests. Contracts (each failed against the pre-parser, which refused every `{` in expression position with");
    probe.case("set_literal_parses", |p| {
    let f0 = p.failures().len();

    let value = parse_definition(p, 
        "emath function Probe:\n    definitions:\n        s = {2, 3, 5}\n",
        "s",
    );
    let ExprKind::Set(items) = &value.kind else {
        panic!(
            "`{{2, 3, 5}}` must parse as ExprKind::Set, got {:?}",
            value.kind
        );
    };
    p.eq("1", items.len(), 3);
    p.demand("2",matches!(items[0].kind, ExprKind::Int(_)), stringify!(matches!(items[0].kind, ExprKind::Int(_))));
    if p.failures().len() != f0 { return; }

    });
    probe.case("set_comprehension_parses_with_guard", |p| {
    let f0 = p.failures().len();

    let value = parse_definition(p, 
        "emath function Probe:\n    definitions:\n        s = {n in 0..100 if is_prime(n)}\n",
        "s",
    );
    let ExprKind::SetComprehension {
        element,
        var,
        domain,
        guard,
    } = &value.kind
    else {
        panic!(
            "`{{n in 0..100 if is_prime(n)}}` must parse as SetComprehension, got {:?}",
            value.kind
        );
    };
    p.demand("1", (var) == ("n"), format!("expected {:?}, got {:?}", ("n"), (var)));
    if p.failures().len() != f0 { return; }
    p.demand("2",matches!(element.kind, ExprKind::Path { .. }), format!(
        "element is the bound name"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",matches!(&domain.kind, ExprKind::Range { .. }), format!(
        "domain is the range"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",guard.is_some(), format!( "guard must be captured"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("inline_record_parses_with_path_prefix", |p| {
    let f0 = p.failures().len();

    let value = parse_definition(p, 
        "emath function Probe:\n    definitions:\n        p = Point:{x: 1.0, y: 2.0}\n",
        "p",
    );
    let ExprKind::Record { type_path, fields } = &value.kind else {
        panic!("`Point:{{...}}` must parse as Record, got {:?}", value.kind);
    };
    p.eq("1", type_path.clone(), vec!["Point".to_string()]);
    p.eq("2", fields.len(), 2);
    p.demand("3", (fields[0].0) == ("x"), format!("expected {:?}, got {:?}", ("x"), (fields[0].0)));
    if p.failures().len() != f0 { return; }
    p.demand("4", (fields[1].0) == ("y"), format!("expected {:?}, got {:?}", ("y"), (fields[1].0)));
    if p.failures().len() != f0 { return; }
    p.demand("5",matches!(fields[0].1.kind, ExprKind::Float(_)), stringify!(matches!(fields[0].1.kind, ExprKind::Float(_))));
    if p.failures().len() != f0 { return; }

    });
    probe.case("membership_operator_parses_outside_binders", |p| {

    let value = parse_definition(p, 
        "emath function Probe:\n    definitions:\n        m = v in s\n",
        "m",
    );
    let ExprKind::Binary { op, .. } = &value.kind else {
        panic!("`v in s` must parse as Binary, got {:?}", value.kind);
    };
    p.eq("1", *op, BinaryOp::In);

    });
    probe.case("binder_in_stays_binder_not_membership", |p| {
    let f0 = p.failures().len();

    // X13 charter: binder `in` (keyword position, after the bound name)
    // and membership `in` (between two expressions) are provably disjoint.
    let value = parse_definition(p, 
        "emath function Probe:\n    definitions:\n        t = sum n in 0..10: n\n",
        "t",
    );
    let ExprKind::Binder { kind, binders, .. } = &value.kind else {
        panic!("binder must stay a binder, got {:?}", value.kind);
    };
    p.demand("1",matches!(kind, BinderKind::Sum), stringify!(matches!(kind, BinderKind::Sum)));
    if p.failures().len() != f0 { return; }
    p.eq("2", binders.len(), 1);

    });
    probe.case("bare_record_brace_is_refused_with_pinned_code", |p| {
    let f0 = p.failures().len();

    // Negative control: `{x: 1}` in expression position
    // with no path prefix is ambiguous between a one-field record and a
    // malformed set. Phase 1 refuses; the pinned code is E-SYN-154.
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/set_braces_ambiguous.emath"
    ));
    p.demand("1",fixture.contains("expect: E-SYN-154"), format!(
        "fixture must pin E-SYN-154"
    ));
    if p.failures().len() != f0 { return; }
    let (_tree, diagnostics) =
        parse_str("emath function Probe:\n    definitions:\n        r = {x: 1}\n");
    p.demand("2",diagnostics.errors().any(|error| error.code == "E-SYN-154"), format!(
        "bare record brace must refuse E-SYN-154, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("elp_x12_both_brace_forms_share_one_profile", |p| {
    // X12: both `{}` spellings share one edition language profile (ELP).
    // The scan demands, for set literal, set comprehension, and path-prefixed
    // record alike: (a) the brace form parses; (b) the canonical formatter
    // preserves the brace spelling; (c) the formatted form reparses cleanly;
    // (d) formatting is idempotent. The record-vs-set ambiguity is resolved
    // by exactly one decision — path prefix selects record, otherwise
    // set/comprehension, and record spelling without a path refuses
    // E-SYN-154 (asserted by `bare_record_brace_is_refused_with_pinned_code`).
    let f0 = p.failures().len();

    let cases = [
        "emath function Probe:\n    definitions:\n        s = {2, 3, 5}\n",
        "emath function Probe:\n    definitions:\n        s = {n in 0..100 if is_prime(n)}\n",
        "emath function Probe:\n    definitions:\n        p = Point:{x: 1.0, y: 2.0}\n",
    ];
    for source in cases {
        let parsed = parse_lossless(source, FileId(0), &Limits::default());
        p.demand("1",!parsed.diagnostics.has_errors(), format!(
            "ELP scan fixture must parse: {source:?}"
        ));
        if p.failures().len() != f0 { return; }
        let once = format(&parsed.tree, &parsed.comments);
        p.demand("2",once.contains('{') && once.contains('}'), format!(
            "brace spelling must survive formatting: {once}"
        ));
        if p.failures().len() != f0 { return; }
        let reparsed = parse_lossless(&once, FileId(0), &Limits::default());
        p.demand("3",!reparsed.diagnostics.has_errors(), format!(
            "formatted form must reparse: {once}"
        ));
        if p.failures().len() != f0 { return; }
        let twice = format(&reparsed.tree, &reparsed.comments);
        p.eq("4", &twice, &once);
    }

    });
    probe.case("sets_comprehensions_membership_and_records_execute", |p| {
    let f0 = p.failures().len();

    let mut session = CompilerSession::new(Limits::default());
    let source = include_str!("../../../tests/fixtures/language/intro/sets-records.emath");
    let checked = session.check_owned("sets-records", source);
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    p.eq("2", values.get("pair"), Some(&Value::Set(vec![
            Value::I64(2),
            Value::I64(3),
            Value::I64(5)
        ])));
    p.eq("3", values.get("two_in_tens"), Some(&Value::Bool(false)));
    p.eq("4", values.get("tens"), Some(&Value::Set((90..100).map(Value::I64).collect::<Vec<_>>())));
    p.demand("5",matches!(
        values.get("origin"),
        Some(Value::Record { type_name, fields })
            if type_name == "Point"
                && fields.get("x") == Some(&Value::F64(0.0))
                && fields.get("y") == Some(&Value::F64(0.0))
    ), stringify!(matches!(
        values.get("origin"),
        Some(Value::Record { type_name, fields })
            if type_name == "Point"
                && fields.get("x") == Some(&Value::F64(0.0))
                && fields.get("y") == Some(&Value::F64(0.0))
    )));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
