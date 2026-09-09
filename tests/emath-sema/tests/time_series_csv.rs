//! Pure-text CSV import into executable `.emath` time series via the
//! `series_from_csv` admission primitive: BOM stripping, RFC-style quoted
//! fields with `""` escapes, the E-CSV-001..009 refusal family, declared
//! policy handoff, and semantic identity of the lowered series.

use emath_core::MeaningId;
use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{RunReport, TestVerdict, run_package};
use emath_ir::{ExprNode, meaning_id};
use emath_test_harness::{boot, error_codes, Probe, Source};

fn csv_source(csv: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", \"linear\", \"refuse\")\n"
    )
}

fn mapped_csv_source(csv: &str, time_col: &str, value_col: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, {time_col:?}, {value_col:?}, \"linear\", \"refuse\")\n"
    )
}

fn policy_csv_source(csv: &str, interpolation: &str, extrapolation: &str) -> String {
    format!(
        "emath function CsvSeries:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", {interpolation:?}, {extrapolation:?})\n"
    )
}

fn sample_source(csv: &str, interpolation: &str, extrapolation: &str, t: f64) -> String {
    format!(
        "emath function Serve:\n    definitions:\n        data = series_from_csv({csv:?}, \"time\", \"value\", {interpolation:?}, {extrapolation:?})\n        sampled = series_at(data, {t})\n    tests:\n        example:\n            expect sampled == 0.0\n"
    )
}

fn lowered(p: &mut Probe, name: &str, source: &str) -> Option<ExprNode> {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        return None;
    }
    let id = *result.package.declarations.first()?.definitions.get("data")?;
    result.package.expr(id).cloned()
}

fn demand_lowered(p: &mut Probe, name: &str, source: &str, expected: &[(f64, f64)]) {
    match lowered(p, name, source) {
        Some(ExprNode::Series { points, interpolation, extrapolation }) => {
            p.eq(format!("{name}/points"), points, expected.to_vec());
            p.eq(format!("{name}/interp"), interpolation, "linear".to_string());
            p.eq(format!("{name}/extra"), extrapolation, "refuse".to_string());
        }
        Some(other) => { p.fail(name, format!("expected ExprNode::Series, got {other:?}")); },
        None => { p.fail(name, "source must admit and lower a Series".to_string()); },
    }
}

fn error_text_of(name: &str, source: &str) -> String {
    Source::from_str(name, source)
        .check()
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn meaning_of(p: &mut Probe, name: &str, source: &str) -> Option<MeaningId> {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        return None;
    }
    match meaning_id(&result.package, &[]) {
        Ok(id) => Some(id),
        Err(error) => {
            p.fail(name, format!("meaning_id of an admitted package must succeed: {error:?}"));
            None
        }
    }
}

fn eval_sampled(p: &mut Probe, name: &str, csv: &str, interp: &str, extra: &str, t: f64) -> Option<RunReport> {
    let result = Source::from_str(name, &sample_source(csv, interp, extra, t)).must_admit(p);
    if result.diagnostics.has_errors() {
        return None;
    }
    Some(run_package(&result.package))
}

fn sampled(p: &mut Probe, report: &RunReport) -> f64 {
    match report.declarations[0].tests[0].definitions.get("sampled") {
        Some(Value::F64(v)) => *v,
        other => {
            p.fail("sampled", format!("sampled must evaluate to a scalar f64, got {other:?}"));
            f64::NAN
        }
    }
}

#[test]
fn csv_series_contract() {
    boot();
    let mut p = Probe::new("series_from_csv lowers text CSV to executable series");
    p.case("bom", |p| {
        demand_lowered(p, "bom", &csv_source("\u{FEFF}time,value\n0.0,1.0\n0.1,2.0"), &[(0.0, 1.0), (0.1, 2.0)]);
        demand_lowered(p, "bom-own-line", &csv_source("\u{FEFF}\ntime,value\n0.0,1.0\n0.1,2.0"), &[(0.0, 1.0), (0.1, 2.0)]);
        let lf = lowered(p, "lf", &csv_source("time,value\n0.0,1.0\n0.1,2.0"));
        let bom_crlf = lowered(p, "bom-crlf", &csv_source("\u{FEFF}time,value\r\n0.0,1.0\r\n0.1,2.0"));
        match (lf, bom_crlf) {
            (Some(a), Some(b)) => { p.eq("bom-crlf-identical", b, a); },
            _ => { p.fail("bom-crlf", "both variants must admit".to_string()); },
        }
    });
    p.case("quoting", |p| {
        demand_lowered(p, "quoted-comma", &csv_source("time,label,value\n0.0,\"1,5\",2.0"), &[(0.0, 2.0)]);
        demand_lowered(p, "escaped-quotes", &csv_source("time,note,value\n0.0,\"a,b\"\"c\",1.0"), &[(0.0, 1.0)]);
        demand_lowered(
            p,
            "quoted-middle",
            &csv_source("time,label,value\n0.0,\"a,b\",1.0\n0.1,c,2.0"),
            &[(0.0, 1.0), (0.1, 2.0)],
        );
        demand_lowered(p, "quoted-header", &csv_source("\"time\",value\n0.0,1.0\n0.1,2.0"), &[(0.0, 1.0), (0.1, 2.0)]);
        demand_lowered(p, "quoted-numeric", &csv_source("time,value\n0.0,\"2.0\""), &[(0.0, 2.0)]);
    });
    p.case("determinism", |p| {
        demand_lowered(p, "plain", &csv_source("time,value\n0.0,1.0\n0.1,2.0"), &[(0.0, 1.0), (0.1, 2.0)]);
        demand_lowered(
            p,
            "whitespace",
            &csv_source("  time , value  \n 0.0 , 1.0 \n 0.1 , 2.0 "),
            &[(0.0, 1.0), (0.1, 2.0)],
        );
        let lf = lowered(p, "lf", &csv_source("time,value\n0.0,1.0\n0.1,2.0"));
        let crlf = lowered(p, "crlf", &csv_source("time,value\r\n0.0,1.0\r\n0.1,2.0"));
        match (lf, crlf) {
            (Some(a), Some(b)) => { p.eq("crlf-identical", b, a); },
            _ => { p.fail("crlf", "both endings must admit".to_string()); },
        }
    });
    p.case("header-mapping", |p| {
        demand_lowered(p, "unit-bare", &csv_source("time (s),value (m/s)\n0.0,1.0\n0.1,2.0"), &[(0.0, 1.0), (0.1, 2.0)]);
        demand_lowered(
            p,
            "unit-full",
            &mapped_csv_source("time (s),value (m/s)\n0.0,1.0\n0.1,2.0", "time (s)", "value (m/s)"),
            &[(0.0, 1.0), (0.1, 2.0)],
        );
        demand_lowered(p, "reordered", &csv_source("value (m/s),time (s)\n1.0,0.0\n2.0,0.1"), &[(0.0, 1.0), (0.1, 2.0)]);
        let text = error_text_of("ambiguous", &csv_source("time (s),time (ms)\n0.0,1.0\n0.1,2.0"));
        p.contains("ambiguous-names", &text, "ambiguous");
    });
    p.case("refusal-codes", |p| {
        for (name, csv, code) in [
            ("missing-time", "tick,value\n0.0,1.0\n0.1,2.0", "E-CSV-001"),
            ("dup-time", "time,time\n0.0,1.0\n0.1,2.0", "E-CSV-002"),
            ("missing-value", "time,tick\n0.0,1.0\n0.1,2.0", "E-CSV-003"),
            ("dup-value", "time,value,value\n0.0,1.0,2.0\n0.1,3.0,4.0", "E-CSV-004"),
            ("ragged", "time,value\n0.0,1.0,2.0", "E-CSV-005"),
            ("header-only", "time,value\n", "E-CSV-007"),
            ("blank-tail", "time,value\n\n", "E-CSV-007"),
            ("equal-times", "time,value\n0.0,1.0\n0.0,2.0", "E-CSV-009"),
            ("decreasing", "time,value\n0.1,1.0\n0.0,2.0", "E-CSV-009"),
        ] {
            let result = Source::from_str(name, &csv_source(csv)).check();
            let codes = error_codes(&result.diagnostics);
            p.demand(format!("{name}/{code}"), codes.iter().any(|c| *c == code), format!("got {codes:?}"));
        }
        for value_cell in ["abc", "NaN", "inf", "-inf", ""] {
            let csv = format!("time,value\n0.0,{value_cell}\n0.1,2.0");
            let result = Source::from_str("nonfinite", &csv_source(&csv)).check();
            let codes = error_codes(&result.diagnostics);
            p.demand(
                format!("nonfinite/{value_cell:?}"),
                codes.iter().any(|c| *c == "E-CSV-008"),
                format!("got {codes:?}"),
            );
        }
        for (name, csv) in [
            ("row-quote", "time,value\n0.0,\"unclosed,1.0"),
            ("header-quote", "time,\"value\n0.0,1.0"),
        ] {
            let text = error_text_of(name, &csv_source(csv));
            p.contains(name, &text, "E-CSV-006");
        }
        let text = error_text_of("row-names-time", &csv_source("time,value\n0.1,1.0\n0.0,2.0"));
        p.contains("row-names-time", &text, "row 3 time 0");
        Source::from_str(
            "arg-shape",
            "emath function CsvSeries:\n    definitions:\n        data = series_from_csv(3.14, \"time\", \"value\", \"linear\", \"refuse\")\n",
        )
        .must_refuse(p, &["E-SERIES-CSV"]);
    });
    p.case("policies-verbatim", |p| {
        let points = [(0.0, 0.0), (0.1, 1.0), (0.2, 3.0)];
        for interpolation in ["previous", "linear", "nearest", "pwc", "monotone_cubic"] {
            for extrapolation in ["refuse", "clamp", "extend"] {
                let name = format!("{interpolation}/{extrapolation}");
                match lowered(p, &name, &policy_csv_source("time,value\n0.0,0.0\n0.1,1.0\n0.2,3.0", interpolation, extrapolation)) {
                    Some(ExprNode::Series { points: got, interpolation: gi, extrapolation: ge }) => {
                        p.eq(format!("{name}/points"), got, points.to_vec());
                        p.eq(format!("{name}/interp"), gi, interpolation.to_string());
                        p.eq(format!("{name}/extra"), ge, extrapolation.to_string());
                    }
                    Some(other) => { p.fail(name, format!("expected Series, got {other:?}")); },
                    None => { p.fail(name, "policy grid must admit".to_string()); },
                }
            }
        }
    });
    p.case("meaning-identity", |p| {
        let canonical = meaning_of(p, "canonical", &csv_source("time,value\n0.0,1.0\n0.1,2.0"));
        for (index, variant) in [
            "time,value\r\n0.0,1.0\r\n0.1,2.0",
            "\u{FEFF}time,value\n0.0,1.0\n0.1,2.0",
            "  time , value  \n 0.0 , 1.0 \n 0.1 , 2.0 ",
            "time,value\n0.0,\"1.0\"\n0.1,\"2.0\"",
            "value,note,time\n1.0,\"note,with comma\",0.0\n2.0,other,0.1",
        ]
        .iter()
        .enumerate()
        {
            let id = meaning_of(p, &format!("variant-{index}"), &csv_source(variant));
            match (canonical.clone(), id) {
                (Some(a), Some(b)) => { p.eq(format!("variant-{index}"), b, a); },
                _ => { p.fail(format!("variant-{index}"), "both must admit".to_string()); },
            }
        }
        let csv = "time,value\n0.0,0.0\n0.1,1.0\n0.2,3.0";
        let interps = ["previous", "linear", "nearest", "pwc", "monotone_cubic"];
        let ids: Vec<Option<MeaningId>> = interps
            .iter()
            .enumerate()
            .map(|(i, policy)| meaning_of(p, &format!("interp-{i}"), &policy_csv_source(csv, policy, "refuse")))
            .collect();
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate().skip(i + 1) {
                match (a.clone(), b.clone()) {
                    (Some(x), Some(y)) => { p.ne(format!("interp/{}-{}", interps[i], interps[j]), x, y); },
                    _ => { p.fail("interp-policy", "both must admit".to_string()); },
                }
            }
        }
        let extras = ["refuse", "clamp", "extend"];
        let eids: Vec<Option<MeaningId>> = extras
            .iter()
            .enumerate()
            .map(|(i, policy)| meaning_of(p, &format!("extra-{i}"), &policy_csv_source(csv, "linear", policy)))
            .collect();
        for (i, a) in eids.iter().enumerate() {
            for (j, b) in eids.iter().enumerate().skip(i + 1) {
                match (a.clone(), b.clone()) {
                    (Some(x), Some(y)) => { p.ne(format!("extra/{}-{}", extras[i], extras[j]), x, y); },
                    _ => { p.fail("extra-policy", "both must admit".to_string()); },
                }
            }
        }
        let base = meaning_of(p, "data-base", &policy_csv_source("time,value\n0.0,1.0\n0.1,2.0\n0.2,3.0", "linear", "refuse"));
        for (index, edited) in [
            "time,value\n0.0,1.0\n0.1,2.0\n0.2,3.0\n0.3,4.0",
            "time,value\n0.0,1.0\n0.1,2.0\n0.2,9.0",
            "time,value\n0.0,1.0\n0.11,2.0\n0.2,3.0",
        ]
        .iter()
        .enumerate()
        {
            let id = meaning_of(p, &format!("edit-{index}"), &policy_csv_source(edited, "linear", "refuse"));
            match (base.clone(), id) {
                (Some(a), Some(b)) => { p.ne(format!("edit-{index}"), b, a); },
                _ => { p.fail(format!("edit-{index}"), "both must admit".to_string()); },
            }
        }
        Source::from_str(
            "reordered-rows",
            &policy_csv_source("time,value\n0.1,2.0\n0.0,1.0\n0.2,3.0", "linear", "refuse"),
        )
        .must_refuse(p, &["E-CSV-009"]);
    });
    p.case("interp-eval", |p| {
        let csv = "time,value\n0.0,0.0\n1.0,2.0";
        if let Some(report) = eval_sampled(p, "linear", csv, "linear", "refuse", 0.5) {
            let v = sampled(p, &report);
            p.close("linear-mid", v, 1.0, 1e-12);
        }
        if let Some(report) = eval_sampled(p, "previous", csv, "previous", "refuse", 0.5) {
            let v = sampled(p, &report);
            p.close("previous-hold", v, 0.0, 1e-12);
        }
        if let Some(report) = eval_sampled(p, "refuse", csv, "linear", "refuse", 2.0) {
            let verdict = &report.declarations[0].tests[0].verdict;
            p.demand(
                "refuse-fault",
                matches!(verdict, TestVerdict::Fault { fault: EvalFault::SeriesOutOfSupport { .. } }),
                format!("outside support must be SeriesOutOfSupport, got {verdict:?}"),
            );
        }
        if let Some(report) = eval_sampled(p, "clamp", csv, "linear", "clamp", 2.0) {
            let v = sampled(p, &report);
            p.close("clamp-end", v, 2.0, 1e-9);
        }
        if let Some(report) = eval_sampled(p, "extend", csv, "linear", "extend", 2.0) {
            let v = sampled(p, &report);
            p.close("extend-segment", v, 4.0, 1e-9);
        }
    });
    p.case("permutation", |p| {
        let points = [(0.0, 1.0), (0.1, 2.0), (0.2, 3.0)];
        let canonical = "time,label,value\n0.0,\"n,with,c\",1.0\n0.1,\"x,y\",2.0\n0.2,z,3.0";
        let canonical_id = meaning_of(p, "perm-base", &csv_source(canonical));
        for (index, csv) in [
            "time,label,value\n0.0,\"n,with,c\",1.0\n0.1,\"x,y\",2.0\n0.2,z,3.0",
            "value,time,label\n1.0,0.0,\"n,with,c\"\n2.0,0.1,\"x,y\"\n3.0,0.2,z",
            "label,value,time\n\"n,with,c\",1.0,0.0\n\"x,y\",2.0,0.1\nz,3.0,0.2",
        ]
        .iter()
        .enumerate()
        {
            demand_lowered(p, &format!("perm-{index}"), &csv_source(csv), &points);
            let id = meaning_of(p, &format!("perm-id-{index}"), &csv_source(csv));
            match (canonical_id.clone(), id) {
                (Some(a), Some(b)) => { p.eq(format!("perm-eq-{index}"), b, a); },
                _ => { p.fail(format!("perm-{index}"), "both must admit".to_string()); },
            }
        }
        let baseline_id = meaning_of(p, "unused-base", &csv_source("time,value\n0.0,1.0\n0.1,2.0"));
        for (index, csv) in [
            "time,value,aux\n0.0,1.0,9.0\n0.1,2.0,8.0",
            "time,value,label\n0.0,1.0,\"a,b\"\n0.1,2.0,c",
            "aux,time,value\n9.0,0.0,1.0\n8.0,0.1,2.0",
            "time,value,aux1,aux2\n0.0,1.0,9.0,7.0\n0.1,2.0,8.0,6.0",
        ]
        .iter()
        .enumerate()
        {
            demand_lowered(p, &format!("unused-{index}"), &csv_source(csv), &[(0.0, 1.0), (0.1, 2.0)]);
            let id = meaning_of(p, &format!("unused-id-{index}"), &csv_source(csv));
            match (baseline_id.clone(), id) {
                (Some(a), Some(b)) => { p.eq(format!("unused-eq-{index}"), b, a); },
                _ => { p.fail(format!("unused-{index}"), "both must admit".to_string()); },
            }
        }
        demand_lowered(
            p,
            "junk-fill",
            &csv_source("time,value,junk\n0.0,1.0,nan\n0.1,2.0,\"a,b\"\n0.2,3.0,"),
            &[(0.0, 1.0), (0.1, 2.0), (0.2, 3.0)],
        );
        demand_lowered(
            p,
            "dup-unused",
            &csv_source("time,value,note,note\n0.0,1.0,\"x\",\"x\"\n0.1,2.0,y,y"),
            &[(0.0, 1.0), (0.1, 2.0)],
        );
        for csv in [
            "time,label,value\n0.0,a\n0.1,b",
            "label,value,time\na,1.0\nb,2.0",
            "time,value\n0.0,1.0,0.5",
            "value,time\n1.0,0.0,0.5",
        ] {
            Source::from_str("ragged-perm", &csv_source(csv)).must_refuse(p, &["E-CSV-005"]);
        }
        for csv in ["time,value\n0.1,1.0\n0.0,2.0", "value,time\n1.0,0.1\n2.0,0.0"] {
            Source::from_str("nonincr-perm", &csv_source(csv)).must_refuse(p, &["E-CSV-009"]);
        }
        for csv in ["time,tick\n0.0,1.0\n0.1,2.0", "tick,time\n1.0,0.0\n2.0,0.1"] {
            Source::from_str("missing-perm", &csv_source(csv)).must_refuse(p, &["E-CSV-003"]);
        }
    });
    p.finish();
}
