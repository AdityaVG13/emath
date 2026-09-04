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

fn parse_definition(source: &str, name: &str) -> emath_core::tree::Expr {
    let (tree, diagnostics) = parse_str(source);
    assert!(
        !diagnostics.has_errors(),
        "expected clean parse, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    );
    def_expr(&tree, name)
        .cloned()
        .unwrap_or_else(|| panic!("definition `{name}` not found"))
}

#[test]
fn set_literal_parses() {
    let value = parse_definition(
        "emath function Probe:\n    definitions:\n        s = {2, 3, 5}\n",
        "s",
    );
    let ExprKind::Set(items) = &value.kind else {
        panic!(
            "`{{2, 3, 5}}` must parse as ExprKind::Set, got {:?}",
            value.kind
        );
    };
    assert_eq!(items.len(), 3, "set literal element count");
    assert!(matches!(items[0].kind, ExprKind::Int(_)));
}

#[test]
fn set_comprehension_parses_with_guard() {
    let value = parse_definition(
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
    assert_eq!(var, "n");
    assert!(
        matches!(element.kind, ExprKind::Path { .. }),
        "element is the bound name"
    );
    assert!(
        matches!(&domain.kind, ExprKind::Range { .. }),
        "domain is the range"
    );
    assert!(guard.is_some(), "guard must be captured");
}

#[test]
fn inline_record_parses_with_path_prefix() {
    let value = parse_definition(
        "emath function Probe:\n    definitions:\n        p = Point:{x: 1.0, y: 2.0}\n",
        "p",
    );
    let ExprKind::Record { type_path, fields } = &value.kind else {
        panic!("`Point:{{...}}` must parse as Record, got {:?}", value.kind);
    };
    assert_eq!(type_path, &["Point".to_string()]);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "x");
    assert_eq!(fields[1].0, "y");
    assert!(matches!(fields[0].1.kind, ExprKind::Float(_)));
}

#[test]
fn membership_operator_parses_outside_binders() {
    let value = parse_definition(
        "emath function Probe:\n    definitions:\n        m = v in s\n",
        "m",
    );
    let ExprKind::Binary { op, .. } = &value.kind else {
        panic!("`v in s` must parse as Binary, got {:?}", value.kind);
    };
    assert_eq!(
        *op,
        BinaryOp::In,
        "`in` in expression position is membership"
    );
}

#[test]
fn binder_in_stays_binder_not_membership() {
    // X13 charter: binder `in` (keyword position, after the bound name)
    // and membership `in` (between two expressions) are provably disjoint.
    let value = parse_definition(
        "emath function Probe:\n    definitions:\n        t = sum n in 0..10: n\n",
        "t",
    );
    let ExprKind::Binder { kind, binders, .. } = &value.kind else {
        panic!("binder must stay a binder, got {:?}", value.kind);
    };
    assert!(matches!(kind, BinderKind::Sum));
    assert_eq!(binders.len(), 1);
}

#[test]
fn bare_record_brace_is_refused_with_pinned_code() {
    // Negative control: `{x: 1}` in expression position
    // with no path prefix is ambiguous between a one-field record and a
    // malformed set. Phase 1 refuses; the pinned code is E-SYN-154.
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/set_braces_ambiguous.emath"
    ));
    assert!(
        fixture.contains("expect: E-SYN-154"),
        "fixture must pin E-SYN-154"
    );
    let (_tree, diagnostics) =
        parse_str("emath function Probe:\n    definitions:\n        r = {x: 1}\n");
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-154"),
        "bare record brace must refuse E-SYN-154, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    );
}

/// X12: both `{}` spellings share one edition language profile (ELP).
/// The scan demands, for set literal, set comprehension, and path-prefixed
/// record alike: (a) the brace form parses; (b) the canonical formatter
/// preserves the brace spelling; (c) the formatted form reparses cleanly;
/// (d) formatting is idempotent. The record-vs-set ambiguity is resolved
/// by exactly one decision — path prefix selects record, otherwise
/// set/comprehension, and record spelling without a path refuses
/// E-SYN-154 (asserted by `bare_record_brace_is_refused_with_pinned_code`).
#[test]
fn elp_x12_both_brace_forms_share_one_profile() {
    let cases = [
        "emath function Probe:\n    definitions:\n        s = {2, 3, 5}\n",
        "emath function Probe:\n    definitions:\n        s = {n in 0..100 if is_prime(n)}\n",
        "emath function Probe:\n    definitions:\n        p = Point:{x: 1.0, y: 2.0}\n",
    ];
    for source in cases {
        let parsed = parse_lossless(source, FileId(0), &Limits::default());
        assert!(
            !parsed.diagnostics.has_errors(),
            "ELP scan fixture must parse: {source:?}"
        );
        let once = format(&parsed.tree, &parsed.comments);
        assert!(
            once.contains('{') && once.contains('}'),
            "brace spelling must survive formatting: {once}"
        );
        let reparsed = parse_lossless(&once, FileId(0), &Limits::default());
        assert!(
            !reparsed.diagnostics.has_errors(),
            "formatted form must reparse: {once}"
        );
        let twice = format(&reparsed.tree, &reparsed.comments);
        assert_eq!(
            twice, once,
            "formatter must be idempotent for the `{{}}` brace form"
        );
    }
}

#[test]
fn sets_comprehensions_membership_and_records_execute() {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = include_str!("../../../language/examples/intro/sets-records.emath");
    let checked = session.check_owned("sets-records", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("pair"),
        Some(&Value::Set(vec![
            Value::I64(2),
            Value::I64(3),
            Value::I64(5)
        ]))
    );
    assert_eq!(values.get("two_in_tens"), Some(&Value::Bool(false)));
    assert_eq!(
        values.get("tens"),
        Some(&Value::Set((90..100).map(Value::I64).collect::<Vec<_>>()))
    );
    assert!(matches!(
        values.get("origin"),
        Some(Value::Record { type_name, fields })
            if type_name == "Point"
                && fields.get("x") == Some(&Value::F64(0.0))
                && fields.get("y") == Some(&Value::F64(0.0))
    ));
}
