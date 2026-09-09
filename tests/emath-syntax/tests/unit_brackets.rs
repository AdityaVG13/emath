//! F7/U4: Compound-unit bracket syntax tests.
//!
//! `9.81 [unit m/s^2]` — compound-unit literal with bracket notation.
//! The `unit` contextual keyword disambiguates from indexing.

use emath_core::tree::{ExprKind, Item, StmtKind, UnitExpr};
use emath_syntax::parse_str;

/// Find the expression bound to `name` inside a declaration's
/// `definitions:` section.
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
    }
    None
}

// ---- Conformance: canonical form and hash equality -------------------

use emath_test_harness::{Probe, boot};

#[test]
fn unit_brackets() {
    boot();
    let mut probe = Probe::new("F7/U4: Compound-unit bracket syntax tests. `9.81 [unit m/s^2]` — compound-unit literal with bracket notation. The `unit` contextual keyword");
    probe.case("compound_unit_bracket_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f() -> Float64:
    definitions:
        g = 9.81 [unit m/s^2]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "compound unit must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "g").expect("expected `g` binding");
    match &expr.kind {
        ExprKind::Quantity { value, unit } => {
            // Value should be the float literal 9.81
            p.demand("2",matches!(&value.kind, ExprKind::Float(v) if v == "9.81"), format!(
                "value should be 9.81, got {:?}",
                value.kind
            ));
            if p.failures().len() != f0 { return; }
            // Unit should be Div(Base("m"), Pow(Base("s"), 2))
            match unit {
                UnitExpr::Div(left, right) => {
                    p.demand("3",matches!(left.as_ref(), UnitExpr::Base(n) if n == "m"), format!(
                        "left should be Base(\"m\"), got {:?}",
                        left
                    ));
                    if p.failures().len() != f0 { return; }
                    match right.as_ref() {
                        UnitExpr::Pow(base, exp) => {
                            p.demand("4",matches!(base.as_ref(), UnitExpr::Base(n) if n == "s"), format!(
                                "pow base should be Base(\"s\"), got {:?}",
                                base
                            ));
                            if p.failures().len() != f0 { return; }
                            p.eq("5", *exp, 2);
                        }
                        other => panic!("right should be Pow, got {:?}", other),
                    }
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("compound_unit_multiplication_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f() -> Float64:
    definitions:
        e = 100.0 [unit kg*m/s^2]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "compound unit with multiplication must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "e").expect("expected `e` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            // kg*m/s^2 = Div(Mul(Base("kg"), Base("m")), Pow(Base("s"), 2))
            match unit {
                UnitExpr::Div(num, den) => {
                    p.demand("2",matches!(num.as_ref(), UnitExpr::Mul(a, b)
                            if matches!(a.as_ref(), UnitExpr::Base(n) if n == "kg")
                            && matches!(b.as_ref(), UnitExpr::Base(n) if n == "m")), format!(
                        "numerator should be kg*m, got {:?}",
                        num
                    ));
                    if p.failures().len() != f0 { return; }
                    p.demand("3",matches!(den.as_ref(), UnitExpr::Pow(base, 2)
                            if matches!(base.as_ref(), UnitExpr::Base(n) if n == "s")), format!(
                        "denominator should be s^2, got {:?}",
                        den
                    ));
                    if p.failures().len() != f0 { return; }
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("compound_unit_parenthesized_denominator_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f() -> Float64:
    definitions:
        a = 9.81 [unit m/(s*s)]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "parenthesized denominator must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "a").expect("expected `a` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            // m/(s*s) = Div(Base("m"), Mul(Base("s"), Base("s")))
            match unit {
                UnitExpr::Div(left, right) => {
                    p.demand("2",matches!(left.as_ref(), UnitExpr::Base(n) if n == "m"), format!(
                        "left should be m, got {:?}",
                        left
                    ));
                    if p.failures().len() != f0 { return; }
                    p.demand("3",matches!(right.as_ref(), UnitExpr::Mul(a, b)
                            if matches!(a.as_ref(), UnitExpr::Base(n) if n == "s")
                            && matches!(b.as_ref(), UnitExpr::Base(n) if n == "s")), format!(
                        "right should be s*s, got {:?}",
                        right
                    ));
                    if p.failures().len() != f0 { return; }
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("c2_trap_left_assoc_division_mul", |p| {
    let f0 = p.failures().len();

    // `m/s*s` is left-associative: ((m/s)*s) = dimension length,
    // NOT acceleration. This is the C2 trap.
    let source = "\
emath function f() -> Float64:
    definitions:
        x = 1.0 [unit m/s*s]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "left-assoc unit must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "x").expect("expected `x` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            // m/s*s = Mul(Div(Base("m"), Base("s")), Base("s"))
            // Flatten should give: m^1, s^-1, s^1 = m^1, s^0 = length
            let factors = unit.flatten();
            let m_power: i32 = factors
                .iter()
                .filter(|(n, _)| n == "m")
                .map(|(_, p)| p)
                .sum();
            let s_power: i32 = factors
                .iter()
                .filter(|(n, _)| n == "s")
                .map(|(_, p)| p)
                .sum();
            p.eq("2", m_power, 1);
            p.eq("3", s_power, 0);
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("acceleration_unit_flattens_correctly", |p| {

    // `m/s^2` should flatten to m^1, s^-2 (acceleration).
    let source = "\
emath function f() -> Float64:
    definitions:
        a = 1.0 [unit m/s^2]
";
    let (tree, _) = parse_str(source);
    let expr = def_expr(&tree, "a").expect("expected `a` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            let factors = unit.flatten();
            let m_power: i32 = factors
                .iter()
                .filter(|(n, _)| n == "m")
                .map(|(_, p)| p)
                .sum();
            let s_power: i32 = factors
                .iter()
                .filter(|(n, _)| n == "s")
                .map(|(_, p)| p)
                .sum();
            p.eq("1", m_power, 1);
            p.eq("2", s_power, -2);
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("variable_indexing_still_works_with_unit_brackets", |p| {
    let f0 = p.failures().len();

    // `v[0]` must still parse as indexing (v is not a numeric literal).
    // `9.81 [unit m/s^2]` must parse as a unit bracket (not indexing).
    let source = "\
emath function f(v: Vector[3]) -> Float64:
    definitions:
        idx = v[0]
        g = 9.81 [unit m/s^2]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "both indexing and unit brackets must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let idx_expr = def_expr(&tree, "idx").expect("expected `idx` binding");
    p.demand("2",matches!(&idx_expr.kind, ExprKind::Index { .. }), format!(
        "v[0] should parse as Index, got {:?}",
        idx_expr.kind
    ));
    if p.failures().len() != f0 { return; }
    let g_expr = def_expr(&tree, "g").expect("expected `g` binding");
    p.demand("3",matches!(&g_expr.kind, ExprKind::Quantity { .. }), format!(
        "9.81 [unit m/s^2] should parse as Quantity, got {:?}",
        g_expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("simple_unit_still_works", |p| {
    let f0 = p.failures().len();

    // Simple unit `9.81 m` must still parse correctly.
    let source = "\
emath function f() -> Float64:
    definitions:
        g = 9.81 m
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "simple unit must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "g").expect("expected `g` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            p.demand("2",matches!(unit, UnitExpr::Base(n) if n == "m"), format!(
                "simple unit should be Base(\"m\"), got {:?}",
                unit
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }

    });
    probe.case("bracket_without_unit_keyword_not_unit_bracket", |p| {
    let f0 = p.failures().len();

    // `9.81 [m]` without the `unit` keyword should NOT be parsed as
    // a unit bracket. The C3 fix breaks out of the postfix loop,
    // and since there's no `unit` keyword, it's not a unit bracket.
    // The key invariant: x is bound to Float("9.81"), not a Quantity
    // with unit "m", and not an Index.
    let source = "\
emath function f() -> Float64:
    definitions:
        x = 9.81 [m]
";
    let (tree, _diags) = parse_str(source);
    let expr = def_expr(&tree, "x").expect("expected `x` binding");
    match &expr.kind {
        ExprKind::Float(v) => { p.demand("1", (v) == ("9.81"), format!("expected {:?}, got {:?}", ("9.81"), (v))); if p.failures().len() != f0 { return; } },
        ExprKind::Int(_) => {} // also acceptable
        other => panic!("x should be bound to a numeric literal, not {:?}", other),
    }

    });
    probe.case("formatter_roundtrips_compound_unit", |p| {
    let f0 = p.failures().len();

    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::formatter::format;
    use emath_syntax::parse_lossless;

    let source =
        "emath function f() -> Float64:\n    definitions:\n        g = 9.81 [unit m/s^2]\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!(
        "source must parse cleanly"
    ));
    if p.failures().len() != f0 { return; }
    let formatted = format(&parsed.tree, &parsed.comments);
    p.demand("2",formatted.contains("[unit m/s^2]"), format!(
        "formatter must preserve compound unit bracket: {formatted}"
    ));
    if p.failures().len() != f0 { return; }
    // Roundtrip: fmt(fmt(s)) == fmt(s)
    let reparsed = parse_lossless(&formatted, FileId(0), &Limits::default());
    p.demand("3",!reparsed.diagnostics.has_errors(), format!(
        "formatted output must parse cleanly"
    ));
    if p.failures().len() != f0 { return; }
    let reformatted = format(&reparsed.tree, &reparsed.comments);
    p.eq("4", &formatted, &reformatted);

    });
    probe.case("formatter_roundtrips_simple_unit", |p| {
    let f0 = p.failures().len();

    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::formatter::format;
    use emath_syntax::parse_lossless;

    let source = "emath function f() -> Float64:\n    definitions:\n        g = 9.81 m\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!(
        "source must parse cleanly"
    ));
    if p.failures().len() != f0 { return; }
    let formatted = format(&parsed.tree, &parsed.comments);
    p.demand("2",formatted.contains("9.81 m"), format!(
        "formatter must preserve simple unit: {formatted}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!formatted.contains("[unit"), format!(
        "formatter must not wrap simple units in brackets: {formatted}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("canonical_form_same_unit_different_spelling", |p| {
    let f0 = p.failures().len();

    // `m/(s*s)` and `m/s^2` should produce the same canonical form.
    let a = UnitExpr::Div(
        Box::new(UnitExpr::Base("m".into())),
        Box::new(UnitExpr::Mul(
            Box::new(UnitExpr::Base("s".into())),
            Box::new(UnitExpr::Base("s".into())),
        )),
    );
    let b = UnitExpr::Div(
        Box::new(UnitExpr::Base("m".into())),
        Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("s".into())), 2)),
    );
    p.eq("1", a.canonical_form(), b.canonical_form());
    p.demand("2", (a.canonical_form()) == ("m/s^2"), format!("expected {:?}, got {:?}", ("m/s^2"), (a.canonical_form())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("canonical_form_different_units_never_collide", |p| {

    // `m/s*s` (length) and `m/s^2` (acceleration) must differ.
    let length = UnitExpr::Mul(
        Box::new(UnitExpr::Div(
            Box::new(UnitExpr::Base("m".into())),
            Box::new(UnitExpr::Base("s".into())),
        )),
        Box::new(UnitExpr::Base("s".into())),
    );
    let accel = UnitExpr::Div(
        Box::new(UnitExpr::Base("m".into())),
        Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("s".into())), 2)),
    );
    p.ne("1", length.canonical_form(), accel.canonical_form());

    });
    probe.case("canonical_form_energy_unit", |p| {
    let f0 = p.failures().len();

    // `kg*m^2/s^2` should canonicalize to `kg*m^2/s^2`.
    let energy = UnitExpr::Div(
        Box::new(UnitExpr::Mul(
            Box::new(UnitExpr::Base("kg".into())),
            Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("m".into())), 2)),
        )),
        Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("s".into())), 2)),
    );
    p.demand("1", (energy.canonical_form()) == ("kg*m^2/s^2"), format!("expected {:?}, got {:?}", ("kg*m^2/s^2"), (energy.canonical_form())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("formatter_normalizes_to_canonical_form", |p| {
    let f0 = p.failures().len();

    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::formatter::format;
    use emath_syntax::parse_lossless;

    // Parse `m/(s*s)` and verify formatter outputs canonical `m/s^2`.
    let source =
        "emath function f() -> Float64:\n    definitions:\n        a = 9.81 [unit m/(s*s)]\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!( "must parse cleanly"));
    if p.failures().len() != f0 { return; }
    let formatted = format(&parsed.tree, &parsed.comments);
    p.demand("2",formatted.contains("[unit m/s^2]"), format!(
        "formatter must normalize m/(s*s) to m/s^2: {formatted}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
