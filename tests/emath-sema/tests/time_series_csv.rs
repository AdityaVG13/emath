//! Pure-text CSV import into executable `.emath` time series via the
//! `series_from_csv` admission primitive (bead emath-xondg).
//!
//! This file grew across the 8-pass `testing-metamorphic` loop: the gaps
//! its early sections pinned in the then-naive CSV parser (bare-`,`
//! splitting, no quoting, no BOM handling) were implemented in
//! `crates/emath-sema/src/admit/lowering.rs` — the parser now strips BOM,
//! parses RFC-style quoted fields with `""` escapes, and reports the
//! E-CSV-001..009 refusal family. The failure-first history below is kept
//! as the record of what each section pinned.
//!
//! Method obligations:
//! - Failure-first is mandatory. The three gap tests (BOM, quoted comma
//!   fields, escaped quotes) MUST FAIL against the unchanged code; the two
//!   pins (CRLF determinism, plain admit+lower) constrain GLOBALLY-reachable
//!   behavior and may pass as controls.
//! - Tests assert BOTH admission success AND the exact lowered points/policy
//!   where the harness exposes them (`SemanticPackage` → `ExprNode::Series`).
//!
//! Observation seam: `CompilerSession::new(Limits::default()).check_owned(...)`
//! returns `CheckResult` whose `package: SemanticPackage` exposes
//! `declarations[i].definitions[name] -> ExprId` and `expr(id) ->
//! Option<&ExprNode>`. The lowered series is `ExprNode::Series { points,
//! interpolation, extrapolation }`.

use emath_core::limits::Limits;
use emath_core::MeaningId;
use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{RunReport, TestVerdict, run_package};
use emath_ir::meaning_id;
use emath_ir::ExprNode;
use emath_sema::admit::CheckResult;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

/// Parse a function whose only meaningful binding is the `data` series
/// produced by `series_from_csv(...)`, returning the lowered `Series` node
/// once it exists. Returns `None` if the parse refuses (no Series lowered).
fn lowered_series(source: &str) -> Option<ExprNode> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result: CheckResult = session.check_owned("time_series_csv", source);
    if result.diagnostics.has_errors() {
        return None;
    }
    let decl = &result.package.declarations[0];
    let id = decl.definitions.get("data")?;
    result.package.expr(*id).cloned()
}

/// Assert the package admits AND lowers exactly the expected points/policy.
fn assert_lowered(source: &str, expected_points: &[(f64, f64)], expected: &str) {
    let node = lowered_series(source).unwrap_or_else(|| {
        let result = check_source("assert_lowered", source);
        panic!(
            "source must admit and lower a Series; refused with:\n{}",
            error_text(&result)
        )
    });
    match node {
        ExprNode::Series {
            points,
            interpolation,
            extrapolation,
        } => {
            assert_eq!(
                points, expected_points,
                "lowered points must be bit-exact {expected:?}"
            );
            assert_eq!(interpolation, "linear", "interpolation policy");
            assert_eq!(extrapolation, "refuse", "extrapolation policy");
        }
        other => panic!("expected ExprNode::Series, got {other:?}"),
    }
}

fn csv_source(csv: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", \"linear\", \"refuse\")\n"
    )
}

/// Full check result for a source, for capturing refusal text.
fn check_source(name: &str, source: &str) -> CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

/// Joined `Display` of every error diagnostic (code + message + span).
fn error_text(result: &CheckResult) -> String {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a source that maps `time_col` / `value_col` (overridable for the
/// unit-suffixed full-name probes) instead of the plain names.
fn mapped_csv_source(csv: &str, time_col: &str, value_col: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, {time_col:?}, {value_col:?}, \"linear\", \"refuse\")\n"
    )
}

/// Assert the check yields an error carrying EXACTLY `code`. Fails (RED) if
/// no diagnostic carries that code.
fn assert_code(result: &CheckResult, code: &str) {
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.iter().any(|c| c == code),
        "expected refusal code {code}, got: {codes:?}\n{}",
        error_text(result)
    );
}

// ---- RED gap 1: BOM-prefixed header ---------------------------------------
// A CSV emitted by many Windows/Excel exports starts with the byte-order mark
// `\u{FEFF}`. Today `csv_series_column_name` trims whitespace but never
// strips the BOM, so the first header cell carries a poisoned name and the
// `time` column match refuses with `E-SERIES-CSV`. A BOM must not change the
// header.
#[test]
fn bom_prefixed_header_admits() {
    let csv = "\u{FEFF}time,value\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "bom-admits",
    );
}

// ---- FAILURE-FIRST (PASS 3): BOM prefix AND CRLF compose --------------------
// Same CSV but with a leading `\u{FEFF}` AND Windows `\r\n` endings. The
// BOM must not poison the header (RED today) and, once stripped, the CRLF
// variant must lower bit-identically to the plain LF one. Cross product pins
// BOM×CRLF compose. Expect RED before the BOM-strip fix.
#[test]
fn bom_prefixed_crlf_admits_identically() {
    let lf = "time,value\n0.0,1.0\n0.1,2.0";
    let bom_crlf = "\u{FEFF}time,value\r\n0.0,1.0\r\n0.1,2.0";
    let lf_node = lowered_series(&csv_source(lf)).expect("LF must admit");
    let bom_crlf_node =
        lowered_series(&csv_source(bom_crlf)).expect("BOM+CRLF must admit");
    assert_eq!(
        bom_crlf_node, lf_node,
        "BOM+CRLF must lower bit-identically to plain LF"
    );
}

// ---- FAILURE-FIRST (PASS 3): BOM on its own line ----------------------------
// Some exports emit `\u{FEFF}` on its own line before the header. A naive
// line filter lets it survive (U+FEFF is not whitespace) and it becomes a
// bogus one-cell header. It must normalize away and admission must yield the
// same points as canonical. Expect RED before the BOM-strip fix.
#[test]
fn bom_on_own_line_then_header_admits() {
    let csv = "\u{FEFF}\ntime,value\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "bom-own-line",
    );
}

// ---- PASS-3 pin: surrounding whitespace normalizes identically --------------
// Spaces around the header cells and around row values must trim away to the
// canonical lowered points (probed GREEN already: both header
// `csv_series_column_name` and row `trim` handle this). Determines that the
// pass-3 BOM strip is not over-eager and leaves whitespace trimming intact.
#[test]
fn surrounding_whitespace_in_header_and_cells_admits_identically() {
    let csv = "  time , value  \n 0.0 , 1.0 \n 0.1 , 2.0 ";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "surrounding-whitespace",
    );
}

// ---- RED gap 2: comma inside a quoted field -------------------------------
// RFC 4180 allows a field to be double-quoted so a comma inside it does not
// delimit cells. Today every row is split on a bare `,`, so
// `0.0,"1,5",2.0` under header `time,label,value` splits into 4 cells and
// refuses as ragged (`E-SERIES-CSV`) — the value column skips past the data.
// The quoted `label` field must not shift the `value` (2.0) column.
#[test]
fn quoted_field_with_comma_does_not_shift_columns() {
    let csv = "time,label,value\n0.0,\"1,5\",2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 2.0)],
        "quoted-comma",
    );
}

// ---- RED gap 3: escape-sequence inside a quoted field ---------------------
// RFC 4180 doubles a double-quote (`""`) inside a quoted field to include a
// literal quote. The naive split passes quote bytes through and still splits
// on the embedded comma, so a quoted field that carries both an escaped
// quote AND a comma comes apart: `0.0,"a,b""c",1.0` splits into 4 cells and
// refuses as ragged. Properly decoded the field is the single cell `a,b"c`
// and the `value` column stays 1.0.
#[test]
fn escaped_quotes_inside_quoted_field_parse() {
    let csv = "time,note,value\n0.0,\"a,b\"\"c\",1.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0)],
        "escaped-quotes",
    );
}

// ---- Determinism pin: CRLF vs LF line endings -----------------------------
// A CSV written with Windows `\r\n` line endings must admit and lower to the
// SAME points as the identical `\n` file. `lines()` already strips the `\r`,
// so this pins the determinism class (and is expected GREEN today).
#[test]
fn crlf_line_endings_admit_identically() {
    let lf = "time,value\n0.0,1.0\n0.1,2.0";
    let crlf = "time,value\r\n0.0,1.0\r\n0.1,2.0";
    let lf_node = lowered_series(&csv_source(lf)).expect("LF must admit");
    let crlf_node = lowered_series(&csv_source(crlf)).expect("CRLF must admit");
    assert_eq!(lf_node, crlf_node, "CRLF and LF must lower identically");
}

// ---- Positive control: plain CSV admits and lowers ------------------------
// Sanity that the harness compiles, the API seam works, admission succeeds,
// and the lowered points are exactly the input rows (GREEN today).
#[test]
fn plain_csv_admits_and_lowers() {
    let csv = "time,value\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "plain-control",
    );
}

// ===========================================================================
// PASS 2: header-name mapping incl. unit-suffixed headers and column reorder
// ===========================================================================

// ---- Pin: unit-suffixed header maps by the bare name -----------------------------
// `csv_series_column_name` strips a trailing `(unit)` suffix from every
// header cell, so a bare request `"time"`/`"value"` already maps to
// `time (s)` / `value (m/s)`. Expected GREEN today; pins that the unit suffix
// does not poison by-name mapping.
#[test]
fn unit_suffixed_header_maps_without_unit() {
    let csv = "time (s),value (m/s)\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "unit-bare-name",
    );
}

// ---- FAILURE-FIRST: unit-suffixed header maps by the FULL name --------------
// The same data requested by its exact raw header cells `"time (s)"` /
// `"value (m/s)"` must lower to identical points. Today `matching_column`
// compares the request only against the NORMALIZED name (unit stripped), so
// this never matches and refuses with E-SERIES-CSV. GAP: full-name mapping.
#[test]
fn unit_suffixed_header_maps_with_full_name() {
    let csv = "time (s),value (m/s)\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &mapped_csv_source(csv, "time (s)", "value (m/s)"),
        &[(0.0, 1.0), (0.1, 2.0)],
        "unit-full-name",
    );
}

// ---- Pin: column reordering is semantics-preserving --------------------------
// With the value column FIRST in both header and rows, by-name mapping must
// still yield the same lowered points as canonical order. Row order keeps
// time strictly increasing: (time=0.0,value=1.0) then (time=0.1,value=2.0).
// Expected GREEN today; pins that reordering does not shift selected columns.
#[test]
fn column_reordering_is_semantics_preserving() {
    let csv = "value (m/s),time (s)\n1.0,0.0\n2.0,0.1";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "reordered-columns",
    );
}

// ---- Pin: unit-suffixed duplicates are ambiguous ------------------------------
// Two header cells that normalize to the same bare name (`time (s)` and
// `time (ms)` both → `time`) leave a bare `"time"` request ambiguous. Must
// REFUSE (never guess), and the refusal must say "ambiguous". GREEN today
// (both normalized cells match → 2 candidates). Pins no silent tie-break.
#[test]
fn unit_suffixed_duplicates_are_ambiguous() {
    let csv = "time (s),time (ms)\n0.0,1.0\n0.1,2.0";
    let result = check_source("time_series_csv", &csv_source(csv));
    assert!(
        result.diagnostics.has_errors(),
        "ambiguous time columns must refuse admission"
    );
    let text = error_text(&result);
    assert!(
        text.contains("ambiguous"),
        "refusal must name ambiguity, got:\n{text}"
    );
}

// ===========================================================================
// PASS 4: quoted fields / escaped quotes
// ===========================================================================

// ---- FAILURE-FIRST (PASS 4): quoted header name maps -----------------------
// A header cell may itself be double-quoted: `"time",value`. Today the raw
// header cell keeps its quotes (`"time"`), so neither the normalized name
// nor the raw-cell form equals the bare request `"time"` and admission
// refuses with E-SERIES-CSV. The quoted header must map to the `time`
// column. Expect RED before the field-splitter fix.
#[test]
fn quoted_header_name_maps() {
    let csv = "\"time\",value\n0.0,1.0\n0.1,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "quoted-header",
    );
}

// ---- FAILURE-FIRST (PASS 4): quoted numeric cell parses --------------------
// A numeric cell may be double-quoted: `0.0,"2.0"`. Today the value cell
// keeps its quotes, `"2.0"` fails the `f64` parse, and admission refuses.
// After unquoting it must lower to the same points as the unquoted `2.0`.
#[test]
fn quoted_numeric_cell_parses() {
    let csv = "time,value\n0.0,\"2.0\"";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 2.0)],
        "quoted-numeric",
    );
}

// ---- FAILURE-FIRST (PASS 4): quoted comma field in any column position -----
// A quoted comma field must not shift columns whether it sits in any position,
// and rows may freely mix quoted and unquoted fields. Today row 1
// `0.0,"a,b",1.0` splits into 4 cells and refuses as ragged. Expected
// lowered points `[(0.0,1.0),(0.1,2.0)]`.
#[test]
fn quoted_comma_field_in_middle_and_last_column() {
    let csv = "time,label,value\n0.0,\"a,b\",1.0\n0.1,c,2.0";
    assert_lowered(
        &csv_source(csv),
        &[(0.0, 1.0), (0.1, 2.0)],
        "quoted-comma-middle",
    );
}

// ---- FAILURE-FIRST (PASS 5): typed refusals for CSV data classes -----------
// All DATA-class refusals today collapse to the flat `E-SERIES-CSV`. PASS 5
// splits them into the numbered family `E-CSV-001..009` (project convention
// `E-<TOPIC>-<NUM>`, cf. E-EVENT-001 / E-TRANS-001). Each test below is RED
// pre-change (it still receives `E-SERIES-CSV`) and GREEN after. A negative
// control pins that the ARG-SHAPE family stays disjoint on `E-SERIES-CSV`.

#[test]
fn missing_time_column_refuses_ecsv001() {
    let csv = "tick,value\n0.0,1.0\n0.1,2.0";
    assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-001");
}

#[test]
fn duplicate_time_column_refuses_ecsv002() {
    let csv = "time,time\n0.0,1.0\n0.1,2.0";
    assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-002");
}

#[test]
fn missing_value_column_refuses_ecsv003() {
    let csv = "time,tick\n0.0,1.0\n0.1,2.0";
    assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-003");
}

#[test]
fn duplicate_value_column_refuses_ecsv004() {
    let csv = "time,value,value\n0.0,1.0,2.0\n0.1,3.0,4.0";
    assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-004");
}

#[test]
fn ragged_row_refuses_ecsv005() {
    let csv = "time,value\n0.0,1.0,2.0";
    assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-005");
}

#[test]
fn malformed_quotes_refuse_ecsv006() {
    // Dangling quote in a data row: the splitter flags the row malformed.
    let row_csv = "time,value\n0.0,\"unclosed,1.0";
    let row_text = error_text(&check_source("time_series_csv", &csv_source(row_csv)));
    assert!(
        row_text.contains("E-CSV-006"),
        "malformed data row must refuse E-CSV-006, got:\n{row_text}"
    );
    // Dangling quote in the header row.
    let header_csv = "time,\"value\n0.0,1.0";
    let header_text = error_text(&check_source("time_series_csv", &csv_source(header_csv)));
    assert!(
        header_text.contains("E-CSV-006"),
        "malformed header must refuse E-CSV-006, got:\n{header_text}"
    );
}

#[test]
fn empty_data_refuses_ecsv007() {
    // Header only, no data rows.
    let header_only = "time,value\n";
    assert_code(
        &check_source("time_series_csv", &csv_source(header_only)),
        "E-CSV-007",
    );
    // Header plus one trailing blank line (blank lines are filtered).
    let header_blank = "time,value\n\n";
    assert_code(
        &check_source("time_series_csv", &csv_source(header_blank)),
        "E-CSV-007",
    );
}

#[test]
fn nonfinite_cells_refuse_ecsv008() {
    for value_cell in ["abc", "NaN", "inf", "-inf", ""] {
        let csv = format!("time,value\n0.0,{value_cell}\n0.1,2.0");
        let result = check_source("time_series_csv", &csv_source(&csv));
        assert_code(&result, "E-CSV-008");
    }
}

#[test]
fn nonincreasing_time_refuses_ecsv009() {
    // Equal times: row 2 time is not strictly after row 1.
    let equal = "time,value\n0.0,1.0\n0.0,2.0";
    assert_code(&check_source("time_series_csv", &csv_source(equal)), "E-CSV-009");
    // Decreasing times: row 2 time is not after row 1.
    let decreasing = "time,value\n0.1,1.0\n0.0,2.0";
    assert_code(
        &check_source("time_series_csv", &csv_source(decreasing)),
        "E-CSV-009",
    );
    // The refusal must name the offending row and its time value (`0.0f64`
    // `Display`s as `0`).
    let text = error_text(&check_source("time_series_csv", &csv_source(decreasing)));
    assert!(
        text.contains("row 3 time 0"),
        "nonincreasing refusal must name the row/time pair, got:\n{text}"
    );
}

// ---- Negative control: arg-shape refusal stays on E-SERIES-CSV ------------
// The compat pin (BronzeCoyote's `tests/emath-syntax/tests/time_series.rs`)
// requires ARG-SHAPE refusals to keep `E-SERIES-CSV`. A non-string-literal
// first argument is an arg-shape defect, disjoint from the data-class family.
// GREEN before and after PASS 5.
#[test]
fn nonliteral_csv_argument_still_refuses_eseriescsv() {
    let source =
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv(3.14, \"time\", \"value\", \"linear\", \"refuse\")\n";
    assert_code(&check_source("time_series_csv", source), "E-SERIES-CSV");
}

// ===========================================================================
// PASS 6: declared interpolation/extrapolation handoff + semantic identity
// ===========================================================================
//
// Metamorphic identity laws, oracle-free: we cannot hand-compute arbitrary
// series meaning, but we CAN assert what must be INVARIANT under the
// all-caps canonical `meaning_id` (the public seam `emath_ir::meaning::meaning_id`
// over an admitted `CheckResult.package`). Every assertion is a
// discrimination test, not a tautology: it pins that (a) textual CSV
// variants that lower to the same points collapse to one meaning, (b)
// policies are identity-bearing, (c) data edits change meaning, and (d) the
// declared policy strings reach the lowered `ExprNode::Series` verbatim.
// The declared policies must also drive the reference interpreter (public
// `emath_exec_ir::runner::run_package` + `series_at` → `EmirOp::SeriesSample`).

/// Meaning id of the `data` series admission across the WHOLE package.
fn meaning_id_of(source: &str) -> MeaningId {
    let result = check_source("time_series_csv_id", source);
    assert!(
        !result.diagnostics.has_errors(),
        "identity source must admit:\n{}",
        error_text(&result)
    );
    meaning_id(&result.package, &[])
        .unwrap_or_else(|e| panic!("meaning_id of an admitted package must succeed: {e:?}"))
}

/// A `series_from_csv` source with the two policies made variable.
fn policy_csv_source(csv: &str, interpolation: &str, extrapolation: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", {interpolation:?}, {extrapolation:?})\n"
    )
}

/// A function that admits `data` from CSV then samples it with `series_at`.
fn sample_report_source(csv: &str, interpolation: &str, extrapolation: &str, t: f64) -> String {
    format!(
        "emath function Serve:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", {interpolation:?}, {extrapolation:?})\n        sampled = series_at(data, {t})\n    tests:\n        example:\n            expect sampled == 0.0\n"
    )
}

/// Admit + run a CSV-built sampled series in the reference interpreter.
fn eval_report(csv: &str, interpolation: &str, extrapolation: &str, t: f64) -> RunReport {
    let source = sample_report_source(csv, interpolation, extrapolation, t);
    let result = check_source("time_series_csv_eval", &source);
    assert!(
        !result.diagnostics.has_errors(),
        "eval source must admit:\n{}",
        error_text(&result)
    );
    run_package(&result.package)
}

/// The value the reference interpreter computed for the `sampled` definition.
fn sampled_value(report: &RunReport) -> f64 {
    match report.declarations[0].tests[0].definitions.get("sampled") {
        Some(Value::F64(v)) => *v,
        other => panic!("sampled must evaluate to a scalar f64, got {other:?}"),
    }
}

// ---- MR-ID-1: textual variants are meaning-identical -----------------------
// The SAME logical series written six different ways (LF, CRLF, BOM prefix,
// surrounding whitespace, quoted value cells, reordered columns with a
// quoted-embedded-comma label) must all admit AND collapse to the canonical
// meaning. Lowering already normalizes each form to the same `points`
// (passes 1,3,4); this pins that the semantic identity sees only the
// normalized series, not the raw bytes.
#[test]
fn csv_textual_variants_are_meaning_identical() {
    let canonical = "time,value\n0.0,1.0\n0.1,2.0";
    let canonical_id = meaning_id_of(&csv_source(canonical));
    let variants = [
        "time,value\r\n0.0,1.0\r\n0.1,2.0", // (b) CRLF
        "\u{FEFF}time,value\n0.0,1.0\n0.1,2.0", // (c) BOM prefix
        "  time , value  \n 0.0 , 1.0 \n 0.1 , 2.0 ", // (d) surrounding whitespace
        "time,value\n0.0,\"1.0\"\n0.1,\"2.0\"", // (e) quoted value cells
        // (f) label column reordered + comma inside a quoted note cell.
        "value,note,time\n1.0,\"note,with comma\",0.0\n2.0,other,0.1",
    ];
    for (index, variant) in variants.iter().enumerate() {
        let variant_id = meaning_id_of(&csv_source(variant));
        assert_eq!(
            canonical_id, variant_id,
            "CSV variant {index} must be meaning-identical to canonical:\n{variant}"
        );
    }
}

// ---- MR-ID-2: interpolation policy hashes in -------------------------------
// Identical points, extrapolation fixed; each of the five interpolation
// policies must yield a pairwise-distinct meaning. Policy is identity-bearing
// for CSV-built series exactly as it is for the literal parent bead.
#[test]
fn interpolation_policy_hash_distinctness() {
    let interps = ["previous", "linear", "nearest", "pwc", "monotone_cubic"];
    let csv = "time,value\n0.0,0.0\n0.1,1.0\n0.2,3.0";
    let ids: Vec<_> = interps
        .iter()
        .map(|policy| meaning_id_of(&policy_csv_source(csv, policy, "refuse")))
        .collect();
    for (i, left) in ids.iter().enumerate() {
        for (j, right) in ids.iter().enumerate() {
            if i != j {
                assert_ne!(
                    left, right,
                    "interpolation `{}` and `{}` must hash to different meanings",
                    interps[i],
                    interps[j]
                );
            }
        }
    }
}

// ---- MR-ID-3: extrapolation policy hashes in -------------------------------
// Identical points + interpolation; each of the three extrapolation policies
// must yield a pairwise-distinct meaning.
#[test]
fn extrapolation_policy_hash_distinctness() {
    let extras = ["refuse", "clamp", "extend"];
    let csv = "time,value\n0.0,0.0\n0.1,1.0\n0.2,3.0";
    let ids: Vec<_> = extras
        .iter()
        .map(|policy| meaning_id_of(&policy_csv_source(csv, "linear", policy)))
        .collect();
    for (i, left) in ids.iter().enumerate() {
        for (j, right) in ids.iter().enumerate() {
            if i != j {
                assert_ne!(
                    left, right,
                    "extrapolation `{}` and `{}` must hash to different meanings",
                    extras[i],
                    extras[j]
                );
            }
        }
    }
}

// ---- MR-ID-4: data edits change meaning; row reorder is refused ------------
// Same policy; extra point, changed value, and shifted time each change data
// and must change meaning. Row ORDER is not a free dimension: permuting rows
// makes the time axis non-increasing and is REFUSED (E-CSV-009), so row order
// cannot be a distinct series.
#[test]
fn data_hash_distinctness_and_row_order_refusal() {
    let base = "time,value\n0.0,1.0\n0.1,2.0\n0.2,3.0";
    let base_id = meaning_id_of(&policy_csv_source(base, "linear", "refuse"));
    let extra_point =
        meaning_id_of(&policy_csv_source("time,value\n0.0,1.0\n0.1,2.0\n0.2,3.0\n0.3,4.0", "linear", "refuse"));
    let changed_value =
        meaning_id_of(&policy_csv_source("time,value\n0.0,1.0\n0.1,2.0\n0.2,9.0", "linear", "refuse"));
    let shifted_time =
        meaning_id_of(&policy_csv_source("time,value\n0.0,1.0\n0.11,2.0\n0.2,3.0", "linear", "refuse"));
    assert_ne!(base_id, extra_point, "adding a point must change meaning");
    assert_ne!(base_id, changed_value, "changing a value must change meaning");
    assert_ne!(base_id, shifted_time, "shifting a time must change meaning");
    // Permuting row order → non-increasing time axis → refused E-CSV-009.
    let reordered = "time,value\n0.1,2.0\n0.0,1.0\n0.2,3.0";
    let result = check_source("time_series_csv", &policy_csv_source(reordered, "linear", "refuse"));
    assert_code(&result, "E-CSV-009");
}

// ---- MR-HANDOFF-1: declared policies reach lowered series verbatim ---------
// For every (interpolation × extrapolation) combination the lowered
// `ExprNode::Series` carries EXACTLY the declared policy strings with the
// points fixed across the whole grid. Pins that the handoff text is never
// normalized, renamed, defaulted, or silently dropped.
#[test]
fn declared_policies_reach_lowered_series_verbatim() {
    let csv = "time,value\n0.0,0.0\n0.1,1.0\n0.2,3.0";
    let points = [(0.0, 0.0), (0.1, 1.0), (0.2, 3.0)];
    let interps = ["previous", "linear", "nearest", "pwc", "monotone_cubic"];
    let extras = ["refuse", "clamp", "extend"];
    for interpolation in interps {
        for extrapolation in extras {
            let node = lowered_series(&policy_csv_source(csv, interpolation, extrapolation))
                .unwrap_or_else(|| {
                    panic!("policy grid ({interpolation},{extrapolation}) must admit a Series")
                });
            match node {
                ExprNode::Series {
                    points: got_points,
                    interpolation: got_interp,
                    extrapolation: got_extra,
                } => {
                    assert_eq!(got_points, points, "points fixed across grid");
                    assert_eq!(
                        got_interp, interpolation,
                        "interpolation must reach the Series verbatim"
                    );
                    assert_eq!(
                        got_extra, extrapolation,
                        "extrapolation must reach the Series verbatim"
                    );
                }
                other => panic!("expected ExprNode::Series, got {other:?}"),
            }
        }
    }
}

// ---- MR-EVAL-1: the reference interpreter honors the declared interpolation ---
// Points (0,0),(1,2): at t=0.5 `linear` must compute 1.0 while `previous`
// must compute 0.0. The declared policy actually changes the computed number.
#[test]
fn interp_world_honors_declared_interpolation() {
    let csv = "time,value\n0.0,0.0\n1.0,2.0";
    let linear = sampled_value(&eval_report(csv, "linear", "refuse", 0.5));
    // Bit-exact by construction: both values are exactly representable and
    // the interpolation arithmetic on them is exact.
    assert_eq!(linear, 1.0, "`linear` at t=0.5 must interpolate to 1.0");
    let previous = sampled_value(&eval_report(csv, "previous", "refuse", 0.5));
    assert_eq!(previous, 0.0, "`previous` at t=0.5 must hold 0.0");
}

// ---- MR-EVAL-2: extrapolation `refuse` is a typed fault --------------------
// Sampling outside support with `refuse` yields the typed
// `EvalFault::SeriesOutOfSupport`; `clamp` returns the endpoint value and
// `extend` continues the outer interval (emath-uooxi behavior). All finite.
#[test]
fn extrapolation_refuse_is_typed() {
    let csv = "time,value\n0.0,0.0\n1.0,2.0";
    let refuse = &eval_report(csv, "linear", "refuse", 2.0).declarations[0]
        .tests[0].verdict;
    assert!(
        matches!(
            refuse,
            TestVerdict::Fault {
                fault: EvalFault::SeriesOutOfSupport { .. }
            }
        ),
        "`refuse` outside [0,1] must be the typed SeriesOutOfSupport fault, got {refuse:?}"
    );
    let clamp = sampled_value(&eval_report(csv, "linear", "clamp", 2.0));
    assert!(
        (clamp - 2.0).abs() < 1e-9,
        "`clamp` at t=2.0 must return the endpoint value 2.0, got {clamp}"
    );
    let extend = sampled_value(&eval_report(csv, "linear", "extend", 2.0));
    assert!(
        (extend - 4.0).abs() < 1e-9,
        "`extend` at t=2.0 must continue the outer segment to 4.0, got {extend}"
    );
}

// ===========================================================================
// PASS 7: column-permutation / unused-column metamorphic laws
// ===========================================================================
//
// The CSV surface has two physical freedoms that must be semantic NO-OPs:
// (1) the physical ORDER of the columns in the header and every row body, and
// (2) the presence, position, content, and duplicates of columns that are NOT
// selected by the `series_from_csv` time/value requests. Metamorphic law: any
// reorder of the physical columns, and any addition/removal/duplication of
// UNSELECTED columns, must admit and lower bit-identically (same points +
// policy) AND collapse to one meaning_id.
//
// Composition rule: MR-1 (permutation) ∘ MR-2 (unused columns) must still
// hold, which is exactly mr_permutation_composed_with_unused_columns.
//
// Negative poles: (a) a logical defect (ragged cells, nonincreasing time,
// missing value column) must KEEP refusing with the SAME code under every
// physical permutation; (b) an unselected column must never be
// finite-validated (its nonfinite/quoted-commа/empty fill must not poison the
// selected projection) and must never be subject to the SELECTED-column
// ambiguity rule.

// ---- MR-PERM-1: column permutation is meaning-invariant ------------------
// The same logical series `{(0.0,1.0),(0.1,2.0),(0.2,3.0)}` with a quoting,
// comma-bearing `label` column written in k=3 physical column orders. Only the
// SELECTED time/value columns determine the series; the label cell must not
// shift anything. Must admit, lower bit-identically, and yield one id.
#[test]
fn mr_column_permutation_is_meaning_invariant() {
    let canonical = "time,label,value\n0.0,\"n,with,c\",1.0\n0.1,\"x,y\",2.0\n0.2,z,3.0";
    let points = [(0.0, 1.0), (0.1, 2.0), (0.2, 3.0)];
    let canonical_id = meaning_id_of(&csv_source(canonical));
    let permutations = [
        "time,label,value\n0.0,\"n,with,c\",1.0\n0.1,\"x,y\",2.0\n0.2,z,3.0",
        "value,time,label\n1.0,0.0,\"n,with,c\"\n2.0,0.1,\"x,y\"\n3.0,0.2,z",
        "label,value,time\n\"n,with,c\",1.0,0.0\n\"x,y\",2.0,0.1\nz,3.0,0.2",
    ];
    for (index, csv) in permutations.iter().enumerate() {
        assert_lowered(&csv_source(csv), &points, &format!("perm-{index}"));
        assert_eq!(
            canonical_id,
            meaning_id_of(&csv_source(csv)),
            "permutation {index} must be meaning-identical:\n{csv}"
        );
    }
}

// ---- MR-UNUSED-1: unused columns (numeric/text/front/two) are insensitive --.
// Four extra-column variants of the same `time,value` data -- an extra
// numeric column, a text column holding a quoted comma, the unused column
// placed FIRST, and TWO unused columns -- all admit, lower bit-identically to
// the two-column baseline, and share its meaning_id.
#[test]
fn mr_unused_column_insensitivity() {
    let baseline = "time,value\n0.0,1.0\n0.1,2.0";
    let points = [(0.0, 1.0), (0.1, 2.0)];
    let baseline_id = meaning_id_of(&csv_source(baseline));
    let variants = [
        "time,value,aux\n0.0,1.0,9.0\n0.1,2.0,8.0", // (a) extra numeric
        "time,value,label\n0.0,1.0,\"a,b\"\n0.1,2.0,c", // (b) text w/ quoted comma
        "aux,time,value\n9.0,0.0,1.0\n8.0,0.1,2.0", // (c) unused up front
        "time,value,aux1,aux2\n0.0,1.0,9.0,7.0\n0.1,2.0,8.0,6.0", // (d) two unused
    ];
    for (index, csv) in variants.iter().enumerate() {
        assert_lowered(&csv_source(csv), &points, &format!("unused-{index}"));
        assert_eq!(
            baseline_id,
            meaning_id_of(&csv_source(csv)),
            "unused-column variant {index} must be meaning-identical:\n{csv}"
        );
    }
}

// ---- MR-COMPOUND-1: permutation composed with unused columns -----------------
// MR-1 ∘ MR-2: permuting ALL columns -- the selected ones AND the extra
// unused `note` column -- across the row bodies must still lower bit-identically
// and share one meaning. One law, two freedoms, chained.
#[test]
fn mr_permutation_composed_with_unused_columns() {
    let baseline = "time,value,note\n0.0,1.0,\"a,b\"\n0.1,2.0,c";
    let points = [(0.0, 1.0), (0.1, 2.0)];
    let baseline_id = meaning_id_of(&csv_source(baseline));
    let compound = [
        "note,value,time\n\"a,b\",1.0,0.0\nc,2.0,0.1",
        "value,note,time\n1.0,\"a,b\",0.0\n2.0,c,0.1",
        "time,note,value\n0.0,\"a,b\",1.0\n0.1,c,2.0",
    ];
    for (index, csv) in compound.iter().enumerate() {
        assert_lowered(&csv_source(csv), &points, &format!("compound-{index}"));
        assert_eq!(
            baseline_id,
            meaning_id_of(&csv_source(csv)),
            "permutation∘unused variant {index} must be meaning-identical:\n{csv}"
        );
    }
}

// ---- MR-UNUSED-2: unselected-column fill is irrelevant ----------------------
// An unused `junk` column carrying nonfinite text (`nan`), a quoted comma
// field (`"a,b"`), and an empty trailing cell must NOT poison the projection:
// only the SELECTED time/value cells are finite-validated. Baseline unchanged.
#[test]
fn mr_unused_column_fill_irrelevance() {
    let baseline = "time,value\n0.0,1.0\n0.1,2.0\n0.2,3.0";
    let points = [(0.0, 1.0), (0.1, 2.0), (0.2, 3.0)];
    let baseline_id = meaning_id_of(&csv_source(baseline));
    let junk = "time,value,junk\n0.0,1.0,nan\n0.1,2.0,\"a,b\"\n0.2,3.0,";
    assert_lowered(&csv_source(junk), &points, "junk-fill");
    assert_eq!(
        baseline_id,
        meaning_id_of(&csv_source(junk)),
        "unselected-column fill must not poison meaning"
    );
}

// ---- MR-REFUSAL-1: refusals are preserved under column permutation ----------
// Refusal determinism: a fixed LOGICAL defect keeps refusing with the SAME
// code no matter how the physical columns are reordered. (a) ragged rows
// (both the missing-cell and extra-cell directions) → E-CSV-005; (b)
// nonincreasing time → E-CSV-009; (c) missing value column → E-CSV-003.
#[test]
fn mr_permutation_preserves_refusals() {
    let ragged = [
        // missing one cell (header is 3-wide, each row is 2 cells)
        "time,label,value\n0.0,a\n0.1,b",
        "label,value,time\na,1.0\nb,2.0",
        "value,time,label\n1.0,0.0\n2.0,0.1",
        // extra one cell (header is 2-wide, each row is 3 cells)
        "time,value\n0.0,1.0,0.5",
        "value,time\n1.0,0.0,0.5",
    ];
    for csv in ragged {
        assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-005");
    }
    let nonincreasing = ["time,value\n0.1,1.0\n0.0,2.0", "value,time\n1.0,0.1\n2.0,0.0"];
    for csv in nonincreasing {
        assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-009");
    }
    let missing_value = ["time,tick\n0.0,1.0\n0.1,2.0", "tick,time\n1.0,0.0\n2.0,0.1"];
    for csv in missing_value {
        assert_code(&check_source("time_series_csv", &csv_source(csv)), "E-CSV-003");
    }
}

// ---- MR-UNUSED-3: duplicate UNUSED columns are irrelevant --------------------
// Two identical UNUSED columns (`note,note`) admit identically to a single
// `note` baseline. The ambiguity rule applies only to a REQUESTED column's
// matches -- never as a global duplicate policy over the whole header.
#[test]
fn mr_duplicate_unused_columns_are_irrelevant() {
    let baseline = "time,value,note\n0.0,1.0,\"x\"\n0.1,2.0,y";
    let points = [(0.0, 1.0), (0.1, 2.0)];
    let baseline_id = meaning_id_of(&csv_source(baseline));
    let duplicates = "time,value,note,note\n0.0,1.0,\"x\",\"x\"\n0.1,2.0,y,y";
    assert_lowered(&csv_source(duplicates), &points, "dup-unused");
    assert_eq!(
        baseline_id,
        meaning_id_of(&csv_source(duplicates)),
        "duplicate unselected columns must not affect meaning"
    );
}
