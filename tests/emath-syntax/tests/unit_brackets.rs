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

#[test]
fn compound_unit_bracket_parses() {
    let source = "\
emath function f() -> Float64:
    definitions:
        g = 9.81 [unit m/s^2]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "compound unit must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "g").expect("expected `g` binding");
    match &expr.kind {
        ExprKind::Quantity { value, unit } => {
            // Value should be the float literal 9.81
            assert!(
                matches!(&value.kind, ExprKind::Float(v) if v == "9.81"),
                "value should be 9.81, got {:?}",
                value.kind
            );
            // Unit should be Div(Base("m"), Pow(Base("s"), 2))
            match unit {
                UnitExpr::Div(left, right) => {
                    assert!(
                        matches!(left.as_ref(), UnitExpr::Base(n) if n == "m"),
                        "left should be Base(\"m\"), got {:?}",
                        left
                    );
                    match right.as_ref() {
                        UnitExpr::Pow(base, exp) => {
                            assert!(
                                matches!(base.as_ref(), UnitExpr::Base(n) if n == "s"),
                                "pow base should be Base(\"s\"), got {:?}",
                                base
                            );
                            assert_eq!(*exp, 2, "exponent should be 2");
                        }
                        other => panic!("right should be Pow, got {:?}", other),
                    }
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn compound_unit_multiplication_parses() {
    let source = "\
emath function f() -> Float64:
    definitions:
        e = 100.0 [unit kg*m/s^2]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "compound unit with multiplication must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "e").expect("expected `e` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            // kg*m/s^2 = Div(Mul(Base("kg"), Base("m")), Pow(Base("s"), 2))
            match unit {
                UnitExpr::Div(num, den) => {
                    assert!(
                        matches!(num.as_ref(), UnitExpr::Mul(a, b)
                            if matches!(a.as_ref(), UnitExpr::Base(n) if n == "kg")
                            && matches!(b.as_ref(), UnitExpr::Base(n) if n == "m")),
                        "numerator should be kg*m, got {:?}",
                        num
                    );
                    assert!(
                        matches!(den.as_ref(), UnitExpr::Pow(base, 2)
                            if matches!(base.as_ref(), UnitExpr::Base(n) if n == "s")),
                        "denominator should be s^2, got {:?}",
                        den
                    );
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn compound_unit_parenthesized_denominator_parses() {
    let source = "\
emath function f() -> Float64:
    definitions:
        a = 9.81 [unit m/(s*s)]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "parenthesized denominator must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "a").expect("expected `a` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            // m/(s*s) = Div(Base("m"), Mul(Base("s"), Base("s")))
            match unit {
                UnitExpr::Div(left, right) => {
                    assert!(
                        matches!(left.as_ref(), UnitExpr::Base(n) if n == "m"),
                        "left should be m, got {:?}",
                        left
                    );
                    assert!(
                        matches!(right.as_ref(), UnitExpr::Mul(a, b)
                            if matches!(a.as_ref(), UnitExpr::Base(n) if n == "s")
                            && matches!(b.as_ref(), UnitExpr::Base(n) if n == "s")),
                        "right should be s*s, got {:?}",
                        right
                    );
                }
                other => panic!("unit should be Div, got {:?}", other),
            }
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn c2_trap_left_assoc_division_mul() {
    // `m/s*s` is left-associative: ((m/s)*s) = dimension length,
    // NOT acceleration. This is the C2 trap.
    let source = "\
emath function f() -> Float64:
    definitions:
        x = 1.0 [unit m/s*s]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "left-assoc unit must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
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
            assert_eq!(m_power, 1, "m should have power 1 (length)");
            assert_eq!(s_power, 0, "s should have power 0 (cancels out)");
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn acceleration_unit_flattens_correctly() {
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
            assert_eq!(m_power, 1, "m should have power 1");
            assert_eq!(s_power, -2, "s should have power -2 (acceleration)");
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn variable_indexing_still_works_with_unit_brackets() {
    // `v[0]` must still parse as indexing (v is not a numeric literal).
    // `9.81 [unit m/s^2]` must parse as a unit bracket (not indexing).
    let source = "\
emath function f(v: Vector[3]) -> Float64:
    definitions:
        idx = v[0]
        g = 9.81 [unit m/s^2]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "both indexing and unit brackets must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let idx_expr = def_expr(&tree, "idx").expect("expected `idx` binding");
    assert!(
        matches!(&idx_expr.kind, ExprKind::Index { .. }),
        "v[0] should parse as Index, got {:?}",
        idx_expr.kind
    );
    let g_expr = def_expr(&tree, "g").expect("expected `g` binding");
    assert!(
        matches!(&g_expr.kind, ExprKind::Quantity { .. }),
        "9.81 [unit m/s^2] should parse as Quantity, got {:?}",
        g_expr.kind
    );
}

#[test]
fn simple_unit_still_works() {
    // Simple unit `9.81 m` must still parse correctly.
    let source = "\
emath function f() -> Float64:
    definitions:
        g = 9.81 m
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "simple unit must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "g").expect("expected `g` binding");
    match &expr.kind {
        ExprKind::Quantity { unit, .. } => {
            assert!(
                matches!(unit, UnitExpr::Base(n) if n == "m"),
                "simple unit should be Base(\"m\"), got {:?}",
                unit
            );
        }
        other => panic!("expected Quantity, got {:?}", other),
    }
}

#[test]
fn bracket_without_unit_keyword_not_unit_bracket() {
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
        ExprKind::Float(v) => assert_eq!(v, "9.81"),
        ExprKind::Int(_) => {} // also acceptable
        other => panic!(
            "x should be bound to a numeric literal, not {:?}",
            other
        ),
    }
}

#[test]
fn formatter_roundtrips_compound_unit() {
    use emath_syntax::formatter::format;
    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::parse_lossless;

    let source = "emath function f() -> Float64:\n    definitions:\n        g = 9.81 [unit m/s^2]\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    assert!(
        !parsed.diagnostics.has_errors(),
        "source must parse cleanly"
    );
    let formatted = format(&parsed.tree, &parsed.comments);
    assert!(
        formatted.contains("[unit m/s^2]"),
        "formatter must preserve compound unit bracket: {formatted}"
    );
    // Roundtrip: fmt(fmt(s)) == fmt(s)
    let reparsed = parse_lossless(&formatted, FileId(0), &Limits::default());
    assert!(
        !reparsed.diagnostics.has_errors(),
        "formatted output must parse cleanly"
    );
    let reformatted = format(&reparsed.tree, &reparsed.comments);
    assert_eq!(formatted, reformatted, "formatter must roundtrip");
}

#[test]
fn formatter_roundtrips_simple_unit() {
    use emath_syntax::formatter::format;
    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::parse_lossless;

    let source = "emath function f() -> Float64:\n    definitions:\n        g = 9.81 m\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    assert!(
        !parsed.diagnostics.has_errors(),
        "source must parse cleanly"
    );
    let formatted = format(&parsed.tree, &parsed.comments);
    assert!(
        formatted.contains("9.81 m"),
        "formatter must preserve simple unit: {formatted}"
    );
    assert!(
        !formatted.contains("[unit"),
        "formatter must not wrap simple units in brackets: {formatted}"
    );
}

// ---- Conformance: canonical form and hash equality -------------------

#[test]
fn canonical_form_same_unit_different_spelling() {
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
    assert_eq!(
        a.canonical_form(),
        b.canonical_form(),
        "m/(s*s) and m/s^2 must have the same canonical form"
    );
    assert_eq!(
        a.canonical_form(),
        "m/s^2",
        "canonical form should be m/s^2, got {}",
        a.canonical_form()
    );
}

#[test]
fn canonical_form_different_units_never_collide() {
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
    assert_ne!(
        length.canonical_form(),
        accel.canonical_form(),
        "m/s*s (length) and m/s^2 (acceleration) must never collide"
    );
}

#[test]
fn canonical_form_energy_unit() {
    // `kg*m^2/s^2` should canonicalize to `kg*m^2/s^2`.
    let energy = UnitExpr::Div(
        Box::new(UnitExpr::Mul(
            Box::new(UnitExpr::Base("kg".into())),
            Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("m".into())), 2)),
        )),
        Box::new(UnitExpr::Pow(Box::new(UnitExpr::Base("s".into())), 2)),
    );
    assert_eq!(
        energy.canonical_form(),
        "kg*m^2/s^2",
        "energy canonical form, got {}",
        energy.canonical_form()
    );
}

#[test]
fn formatter_normalizes_to_canonical_form() {
    use emath_syntax::formatter::format;
    use emath_core::FileId;
    use emath_core::limits::Limits;
    use emath_syntax::parse_lossless;

    // Parse `m/(s*s)` and verify formatter outputs canonical `m/s^2`.
    let source = "emath function f() -> Float64:\n    definitions:\n        a = 9.81 [unit m/(s*s)]\n";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors(), "must parse cleanly");
    let formatted = format(&parsed.tree, &parsed.comments);
    assert!(
        formatted.contains("[unit m/s^2]"),
        "formatter must normalize m/(s*s) to m/s^2: {formatted}"
    );
}
