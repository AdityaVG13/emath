//! `emath-syntax` canonical formatter tests (migrated from
//! `crates/emath-syntax/src/formatter.rs`).

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, Expr, ExprKind, Item, StmtKind};
use emath_syntax::formatter::format;
use emath_syntax::parse_lossless;

fn format_once(p: &mut Probe, text: &str) -> String {
    let parsed = parse_lossless(text, FileId(0), &Limits::default());
    p.demand("parse", !parsed.diagnostics.has_errors(), "fixture must parse");
    format(&parsed.tree, &parsed.comments)
}

fn def_expr<'a>(tree: &'a emath_core::tree::SyntaxTree, name: &str) -> Option<&'a Expr> {
    let item = tree.items.iter().find_map(|item| match item {
        Item::Declaration(decl) => Some(decl),
        _ => None,
    })?;
    for section in item.sections() {
        if section.name != "definitions" {
            continue;
        }
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::Let { name: n, value, .. } if n == name => return Some(value),
                StmtKind::Assign { target, value }
                    if target.segments.first().is_some_and(|s| s == name) =>
                {
                    return Some(value);
                }
                _ => {}
            }
        }
    }
    None
}

use emath_test_harness::{Probe, boot};

#[test]
fn formatter() {
    boot();
    let mut probe = Probe::new("`emath-syntax` canonical formatter tests (migrated from `crates/emath-syntax/src/formatter.rs`).");
    probe.case("equation_renders_at_sibling_indent_without_blank_line", |p| {
    // SURF-0013: an Equation statement inside a nested section renders
    // at sibling indent (one level, not two) with a single newline —
    // no blank line right after the equation.
    let f0 = p.failures().len();

    let source = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n";
    let once = format_once(p, source);
    p.demand("1",once.contains("        y = x * x"), format!(
        "equation must sit at sibling indent: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!once.contains("            y = x * x"), format!(
        "no double indent: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!once.contains("y = x * x\n\n"), format!(
        "no blank line immediately after the equation: {once}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("formatting_is_idempotent_and_parse_stable", |p| {
    // Golden: formatting is idempotent (`fmt(fmt(s)) == fmt(s)`) and
    // the formatted output parses back cleanly.
    let f0 = p.failures().len();

    let source = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n";
    let once = format_once(p, source);
    let _fmt_1 = format_once(p, &once);
        p.eq("1", _fmt_1, once.clone());
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    p.demand("2",!rebound.diagnostics.has_errors(), format!(
        "formatted output must parse back"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("corpus_files_are_lossless_round_trip", |p| {
    // SURF-0013: every valid corpus file must be byte-canonical under
    // the lossless formatter (`fmt(file) == file`). This pins the exact
    // canonical spellings: `produce rust.library` keeps its dot,
    // expression paths render as `state.scale`, and section generics
    // render `evaluate <y>:` / `example <name>:` (angle, spaced).
    let f0 = p.failures().len();

    for (name, text) in [
        ("square", include_str!("../../valid/square.emath")),
        (
            "affine_scorer",
            include_str!("../../valid/affine_scorer.emath"),
        ),
    ] {
        let parsed = parse_lossless(text, FileId(0), &Limits::default());
        p.demand("1",!parsed.diagnostics.has_errors(), format!(
            "{name}: fixture must parse"
        ));
        if p.failures().len() != f0 { return; }
        let canonical = format(&parsed.tree, &parsed.comments);
        p.demand("2", canonical == text, format!("expected {:?}, got {:?}", (text), (&canonical)));
    }

    });
    probe.case("corpus_canonical_reparse_is_stable", |p| {
    // SURF-0013: the canonical render of the corpus round-trips: the
    // formatted output parses back to the identical tree (format-parse
    // fixpoint), so re-formatting never changes the output.
    let f0 = p.failures().len();

    for (name, text) in [
        ("square", include_str!("../../valid/square.emath")),
        (
            "affine_scorer",
            include_str!("../../valid/affine_scorer.emath"),
        ),
    ] {
        let parsed = parse_lossless(text, FileId(0), &Limits::default());
        let canonical = format(&parsed.tree, &parsed.comments);
        let reborn = parse_lossless(&canonical, FileId(0), &Limits::default());
        p.demand("1",!reborn.diagnostics.has_errors(), format!(
            "{name}: canonical must parse back"
        ));
        if p.failures().len() != f0 { return; }
        p.eq("2", format(&reborn.tree, &reborn.comments), canonical);
    }

    });
    probe.case("numeric_units_surface_round_trips", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Timed:
    inputs:
        t: Duration
        rate: Per<Duration>
    outputs:
        y: Float64
    definitions:
        y = t / 1 s * rate * 1 s
    compile:
        target rust
        numeric interval-f64
        precision 53
        error-limit 1e-12
";
    let once = format_once(p, source);
    let _fmt_1 = format_once(p, &once);
        p.eq("1", _fmt_1, once.clone());
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    p.demand("2",!rebound.diagnostics.has_errors(), format!(
        "formatted numeric/units surface must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",once.contains("numeric interval-f64"), stringify!(once.contains("numeric interval-f64")));
    if p.failures().len() != f0 { return; }
    p.demand("4",once.contains("1 s"), stringify!(once.contains("1 s")));
    if p.failures().len() != f0 { return; }
    p.demand("5",once.contains("Per<Duration>"), stringify!(once.contains("Per<Duration>")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("item_attributes_round_trip", |p| {
    // ELP item attributes round-trip in the canonical `@` spelling:
    // `emath_item = { attribute }, "emath", ...`. The formatter must emit
    // the grammar form (not a Rust-style bracket) and remain idempotent.
    let f0 = p.failures().len();

    let source = "\
@capabilities(experimental-syntax)
@experimental
emath function P:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";
    let once = format_once(p, source);
    p.demand("1",once.starts_with("@capabilities(experimental-syntax)\n@experimental\nemath function P:"), format!(
        "attributes must render before the head in `@` form: {once}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!once.contains('['), format!(
        "attributes must not render as Rust-style brackets: {once}"
    ));
    if p.failures().len() != f0 { return; }
    let _fmt_3 = format_once(p, &once);
        p.eq("3", _fmt_3, once.clone());
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    p.demand("4",!rebound.diagnostics.has_errors(), format!(
        "formatted attributes must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_items_round_trip", |p| {
    // Notation declarations round-trip in the canonical spelling
    // `notation <fixity> <prec> "<glyph>" => <path> [alias "<str>"]`,
    // separated from sibling items by one blank line, and the reformatted
    // output stays idempotent and parse-stable.
    let f0 = p.failures().len();

    let source = "\
package tst.ex2

notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"

notation prefix 80 \"√\" => core::math::sqrt

notation postfix 90 \"inv\" => core::math::recip

emath function F:
    outputs:
        y: Float64
    definitions:
        y = 1.0
";
    let once = format_once(p, source);
    let _fmt_1 = format_once(p, &once);
        p.eq("1", _fmt_1, once.clone());
    p.demand("2",once.contains("notation infixl 40 \"⊕\" => core::math::pow alias \"pw\""), stringify!(once.contains("notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"")));
    if p.failures().len() != f0 { return; }
    p.demand("3",once.contains("notation prefix 80 \"√\" => core::math::sqrt"), stringify!(once.contains("notation prefix 80 \"√\" => core::math::sqrt")));
    if p.failures().len() != f0 { return; }
    p.demand("4",once.contains("notation postfix 90 \"inv\" => core::math::recip"), stringify!(once.contains("notation postfix 90 \"inv\" => core::math::recip")));
    if p.failures().len() != f0 { return; }
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    p.demand("5",!rebound.diagnostics.has_errors(), format!(
        "formatted notation must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("attribute_string_arguments_keep_quotes", |p| {
    // Quoted string arguments keep their quotes through the round trip so
    // identifier args and string args never merge on reformat.
    let f0 = p.failures().len();

    let source = "\
@capabilities(\"experimental-syntax\", nightly)
emath function P:
    outputs:
        y: Float64
    definitions:
        y = 1.0
";
    let once = format_once(p, source);
    p.demand("1",once.contains("@capabilities(\"experimental-syntax\", nightly)"), format!(
        "quoted form must round-trip: {once}"
    ));
    if p.failures().len() != f0 { return; }
    let _fmt_2 = format_once(p, &once);
        p.eq("2", _fmt_2, once.clone());

    });
    probe.case("parse_print_parse_preserves_tree", |p| {
    // parse(print(parse(src))) must keep tree identity: rationals, grouping,
    // colon-form conditionals, escaped strings, and `#` comments.
    let f0 = p.failures().len();

    let source = "\
# keep this comment
emath function f(a: Float64, b: Float64, c: Float64, x: Float64, v: Float64) -> Float64:
    definitions:
        r = 3//7
        q = 3//2 s
        d = a - (b - c)
        p = (a ^ b) ^ c
        s = if x > 0: 1 else: 0
        dv = derivative (v + v)
        msg = \"say \\\"hi\\\"\"
        t = (a,)
";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!(
        "fixture must parse: {:?}",
        parsed
            .diagnostics
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let printed = format(&parsed.tree, &parsed.comments);
    p.demand("2",printed.contains("# keep this comment"), format!(
        "formatter claims to keep comments, got {printed}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",printed.contains("3//7"), format!(
        "rational 3//7 must print, got {printed}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",printed.contains("if x > 0: 1 else: 0"), format!(
        "conditional must print colon form, not `then`: {printed}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("5",!printed.contains(" then "), format!(
        "conditional must not print the non-keyword `then`: {printed}"
    ));
    if p.failures().len() != f0 { return; }

    let rebound = parse_lossless(&printed, FileId(0), &Limits::default());
    p.demand("6",!rebound.diagnostics.has_errors(), format!(
        "print must parse back: {:?}\nprinted:\n{printed}",
        rebound
            .diagnostics
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.eq("7", format(&rebound.tree, &rebound.comments), printed);

    let r = def_expr(&rebound.tree, "r").expect("r");
    p.demand("8",matches!(&r.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "7"), format!(
        "3//7 must remain Rational, got {:?}",
        r.kind
    ));
    if p.failures().len() != f0 { return; }

    let q = def_expr(&rebound.tree, "q").expect("q");
    let ExprKind::Quantity { value, .. } = &q.kind else {
        panic!("3//2 s must remain Quantity, got {:?}", q.kind);
    };
    p.demand("9",matches!(&value.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "2"), format!(
        "quantity value must stay Rational 3//2, got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }

    let d = def_expr(&rebound.tree, "d").expect("d");
    let ExprKind::Binary {
        op: BinaryOp::Sub,
        right,
        ..
    } = &d.kind
    else {
        panic!("d must stay subtraction, got {:?}", d.kind);
    };
    p.demand("10",matches!(
            &right.kind,
            ExprKind::Binary {
                op: BinaryOp::Sub,
                ..
            }
        ), format!(
        "a - (b - c) must not flatten to (a - b) - c, got {:?}",
        d.kind
    ));
    if p.failures().len() != f0 { return; }

    let bound_p = def_expr(&rebound.tree, "p").expect("p");
    let ExprKind::Binary {
        op: BinaryOp::Pow,
        left,
        ..
    } = &bound_p.kind
    else {
        panic!("p must stay power, got {:?}", bound_p.kind);
    };
    p.demand("11",matches!(
            &left.kind,
            ExprKind::Binary {
                op: BinaryOp::Pow,
                ..
            }
        ), format!(
        "(a ^ b) ^ c must keep left grouping, got {:?}",
        bound_p.kind
    ));
    if p.failures().len() != f0 { return; }

    let s = def_expr(&rebound.tree, "s").expect("s");
    p.demand("12",matches!(&s.kind, ExprKind::If { .. }), format!(
        "colon-form if must reparse as If, got {:?}",
        s.kind
    ));
    if p.failures().len() != f0 { return; }

    let dv = def_expr(&rebound.tree, "dv").expect("dv");
    let ExprKind::Derivative { value, .. } = &dv.kind else {
        panic!("dv must stay Derivative, got {:?}", dv.kind);
    };
    p.demand("13",matches!(
            &value.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ), format!(
        "derivative (v + v) must not become (derivative v) + v, got {:?}",
        dv.kind
    ));
    if p.failures().len() != f0 { return; }

    let msg = def_expr(&rebound.tree, "msg").expect("msg");
    p.demand("14",matches!(&msg.kind, ExprKind::Str(text) if text == "say \"hi\""), format!(
        "escaped quotes must round-trip, got {:?}",
        msg.kind
    ));
    if p.failures().len() != f0 { return; }

    let t = def_expr(&rebound.tree, "t").expect("t");
    p.demand("15",matches!(&t.kind, ExprKind::Tuple(items) if items.len() == 1), format!(
        "1-tuple (a,) must not collapse to grouping, got {:?}",
        t.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
