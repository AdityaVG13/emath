//! Table literals (U9): semicolon rows and table literals.
//!
//! Two additive spellings (A1: no juxtaposition — cells are comma-separated):
//! - `[1, 2; 3, 4]` folds to the same nested-List shape admission already
//!   lowers to a Matrix (`;` is a row separator, nothing else reparses);
//! - `|x y| 1, 2 | 3, 4 |` folds to one Table literal carrying header
//!   names plus comma-separated rows; ≥2 headers keep `|` unambiguous with
//!   U1 cases arms (`| cond => ...`) and infix `or`.
//!
//! Failure-first: every fold pin below is RED until the `;` token, the
//! row-splitting list parse, and the table primary land.

use emath_core::tree::{ExprKind, StmtKind};
use emath_syntax::parse_str;

fn def_expr_of(p: &mut Probe, source: &str) -> ExprKind {
    let (tree, diags) = parse_str(source);
    p.demand("parse", !diags.has_errors(), format!("{diags:?}"));
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.first() else {
        panic!("declaration expected");
    };
    let Some(stmt) = decl
        .body
        .iter()
        .find(|stmt| matches!(&stmt.kind, StmtKind::Section(s) if s.name == "definitions"))
    else {
        panic!("definitions section expected");
    };
    let StmtKind::Section(definitions) = &stmt.kind else {
        unreachable!()
    };
    match &definitions.suite.statements[0].kind {
        StmtKind::Assign { value, .. } => value.kind.clone(),
        other => panic!("assignment expected, got {other:?}"),
    }
}

fn int_of(expr: &ExprKind) -> &str {
    let ExprKind::Int(text) = expr else {
        panic!("int expected, got {expr:?}")
    };
    text
}

// ---- admission: table content lowers through the Matrix path -------------

use emath_sema::CompilerSession;

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    session.check_owned("table-literals", source)
}

use emath_test_harness::{Probe, boot};

#[test]
fn table_literals() {
    boot();
    let mut probe = Probe::new("Table literals (U9): semicolon rows and table literals. Two additive spellings (A1: no juxtaposition — cells are comma-separated): - `[1, 2; 3, 4]`");
    probe.case("semicolon_rows_fold_to_matrix_lists", |p| {

    // `[1, 2; 3, 4]` is one 2x2 literal: two rows, two cells each. The pin
    // dies while `;` is unlexed (E-SYN-101 unexpected character) or when a
    // mutant collapses rows into one flat list.
    let expr = def_expr_of(p, "emath function f:\n    definitions:\n        m = [1, 2; 3, 4]\n");
    let ExprKind::List(rows) = &expr else {
        panic!("list expected, got {expr:?}")
    };
    p.eq("1", rows.len(), 2);
    let ExprKind::List(row0) = &rows[0].kind else {
        panic!("row 0 must be a list, got {:?}", rows[0].kind)
    };
    let ExprKind::List(row1) = &rows[1].kind else {
        panic!("row 1 must be a list, got {:?}", rows[1].kind)
    };
    p.eq("2", (int_of(&row0[0].kind), int_of(&row0[1].kind)), ("1", "2"));
    p.eq("3", (int_of(&row1[0].kind), int_of(&row1[1].kind)), ("3", "4"));

    });
    probe.case("single_cell_rows_stay_rank_two", |p| {

    // `[1; 2; 3]` is a 3x1 matrix (three one-cell rows), never a flat
    // vector — the `;` is what makes it rank 2.
    let expr = def_expr_of(p, "emath function f:\n    definitions:\n        v = [1; 2; 3]\n");
    let ExprKind::List(rows) = &expr else {
        panic!("list expected, got {expr:?}")
    };
    p.eq("1", rows.len(), 3);
    for row in rows {
        let ExprKind::List(cells) = &row.kind else {
            panic!("each row must be a list, got {:?}", row.kind)
        };
        p.eq("2", cells.len(), 1);
    }

    });
    probe.case("flat_list_is_unchanged_by_semicolon_support", |p| {
    let f0 = p.failures().len();

    // Regression: a comma-only list never gains a nesting level.
    let expr = def_expr_of(p, "emath function f:\n    definitions:\n        v = [1, 2, 3]\n");
    let ExprKind::List(items) = &expr else {
        panic!("list expected, got {expr:?}")
    };
    p.eq("1", items.len(), 3);
    p.demand("2",items
            .iter()
            .all(|item| matches!(item.kind, ExprKind::Int(_))), format!(
        "no row wrapping without `;`, got {items:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("ragged_semicolon_rows_refuse", |p| {
    let f0 = p.failures().len();

    // `[1, 2, 3; 4]`: row lengths must match at parse time (E-SYN-102) —
    // ragged rows must not silently fold into a shape error downstream.
    let (_, diags) = parse_str("emath function f:\n    definitions:\n        m = [1, 2, 3; 4]\n");
    p.demand("1",diags.errors().any(|error| error.code == "E-SYN-102"), format!(
        "ragged `;` rows must refuse E-SYN-102, got {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("table_literal_folds_with_headers_and_rows", |p| {

    // `|x y| 1, 2 | 3, 4 |` is one Table literal: named columns, cells
    // comma-separated (A1 — space-separated cells would be juxtaposition).
    let expr =
        def_expr_of(p, "emath function f:\n    definitions:\n        t = |x y| 1, 2 | 3, 4 |\n");
    let ExprKind::Table { headers, rows } = &expr else {
        panic!("table expected, got {expr:?}")
    };
    p.eq("1", headers.clone(), vec!["x".to_string(), "y".to_string()]);
    p.eq("2", rows.len(), 2);
    p.eq("3", (int_of(&rows[0][0].kind), int_of(&rows[0][1].kind)), ("1", "2"));
    p.eq("4", (int_of(&rows[1][0].kind), int_of(&rows[1][1].kind)), ("3", "4"));

    });
    probe.case("cases_pipe_still_parses_as_cases", |p| {
    let f0 = p.failures().len();

    // Disambiguation guard (HEAD-true): `| cond => …` after `cases` stays
    // a cases arm — the table primary must not capture arm-leading `|`.
    let expr = def_expr_of(p, 
        "emath function f:\n    inputs:\n        x: Float64\n    definitions:\n        r = cases x: | x > 0 => 1 | else => 0\n",
    );
    p.demand("1",matches!(&expr, ExprKind::Cases { .. }), format!(
        "cases pipe must stay cases, got {expr:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("pipe_infix_or_is_unchanged", |p| {
    let f0 = p.failures().len();

    // Disambiguation guard (HEAD-true): infix `|` as `or` between operands
    // is untouched by the table primary (tables only start at pipe position).
    let expr = def_expr_of(p, "emath function f:\n    definitions:\n        r = true | false\n");
    p.demand("1",matches!(
            &expr,
            ExprKind::Binary {
                op: emath_core::tree::BinaryOp::Or,
                ..
            }
        ), format!(
        "infix `|` must stay or, got {expr:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("single_column_pipe_is_not_a_table", |p| {
    let f0 = p.failures().len();

    // Disambiguation negative: `|x| …` (one header before `|`) is not a
    // table — the ≥2-header rule is what keeps `|` unambiguous with
    // cases arms and infix `or`.
    let (_, diags) = parse_str("emath function f:\n    definitions:\n        t = |x| 1 |\n");
    p.demand("1",diags.has_errors(), format!(
        "single-column `|…|` must refuse, not fold to a table"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("table_ragged_rows_refuse", |p| {
    let f0 = p.failures().len();

    // Negative: a row with fewer cells than headers refuses at parse time
    // (E-SYN-102), never folds into a ragged table for admission to trip on.
    let (_, diags) =
        parse_str("emath function f:\n    definitions:\n        t = |x y| 1, 2 | 3 |\n");
    p.demand("1",diags.errors().any(|error| error.code == "E-SYN-102"), format!(
        "ragged table rows must refuse E-SYN-102, got {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("semicolon_matrix_admits", |p| {
    let f0 = p.failures().len();

    // `[1, 2; 3, 4]` admits as a rank-2 value: a 2-index lookup must be
    // legal. If a mutant collapsed `;` rows into one flat vector, the
    // rank-1 `m[0, 1]` would refuse (shape) — this pin kills that mutant.
    let checked = check_source(
        "emath function f:\n    definitions:\n        m = [1, 2; 3, 4]\n        r = m[0, 1]\n\n    outputs:\n        r: Float64\n",
    );
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("table_non_numeric_column_refuses", |p| {
    let f0 = p.failures().len();

    // Column-type check: a bare identifier cell in a numeric table is a
    // typed refusal (E-TYPE-002 unknown variable — the matrix element
    // path's numeric gate), never a silent mixed-type table.
    let checked =
        check_source("emath function f:\n    definitions:\n        t = |x y| 1, 2 | 3, zz |\n");
    p.demand("1",checked.diagnostics.has_errors(), format!(
        "non-numeric table cell must refuse"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
