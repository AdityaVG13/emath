//! Failure-first harness for `emath sweep` (bead emath-6gz8m).
mod common;
use emath_artifact::{JsonValue, parse_json_document};
use emath_cli::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_test_harness::Probe;
fn dir(name: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("emath-cli-sweep-{name}-{}", std::process::id())); std::fs::create_dir_all(&d).expect("dir"); d }
fn write(dir: &std::path::PathBuf, name: &str, src: &str) -> String { let f = dir.join(name); std::fs::write(&f, src).expect("write"); f.to_string_lossy().into_owned() }
fn sweep(path: &str, args: &[&str]) -> (String, String, i32) { let o = std::process::Command::new(common::emath_lab_bin()).arg("sweep").arg(path).args(args).output().expect("sweep"); (String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned(), o.status.code().unwrap_or(-1)) }
fn doc(stdout: &str) -> JsonValue { parse_json_document(stdout).unwrap_or_else(|e| panic!("artifact: {e}\n{stdout}")) }
fn pairs(cell: &JsonValue, field: &str) -> Vec<(String, String)> { match cell.field(field).expect(field) { JsonValue::Obj(es) => es.iter().map(|(n, v)| match v { JsonValue::Str(t) => (n.clone(), t.clone()), o => panic!("{field} {n} must render, got {o:?}") }).collect(), o => panic!("{field} must be object, got {o:?}") } }
const SQ1: &str = "emath function Sq:\n    inputs:\n        p: Int\n    outputs:\n        y: Int\n    definitions:\n        y = p * p\n";
const ADD2: &str = "emath function Add2:\n    inputs:\n        p: Int\n        z: Int\n    outputs:\n        s: Int\n    definitions:\n        s = p + z\n";
const DUAL: &str = "emath function Dual:\n    inputs:\n        p: Int\n        z: Int\n    outputs:\n        s: Int\n        m: Int\n    definitions:\n        s = p + z\n        m = p * z\n";
fn diag(doc: &JsonValue) -> String { match doc.field("diagnostics").expect("diags") { JsonValue::Arr(ds) => ds[0].string_field("code").unwrap(), o => panic!("diags array, got {o:?}") } }
#[test]
fn probe() {
    let mut p = Probe::new("sweep runs cartesian grids deterministically with typed refusals and exact rows");
    p.case("cartesian", |p| {
        let d = dir("cartesian"); let path = write(&d, "add2.emath", ADD2);
        let (out, err, code) = sweep(&path, &["--function", "Add2", "--grid", "p=2,3", "z=10,20", "--json"]);
        p.eq("exit", code, EXIT_OK as i32); p.eq("clean", err, String::new());
        let a = doc(&out); p.eq("function", a.string_field("function").unwrap(), "Add2".to_string()); p.demand("meaning", a.string_field("meaning_id").unwrap().starts_with("emath:meaning:v1:"), "meaning carried");
        let JsonValue::Arr(axes) = a.field("grid").unwrap().field("axes").unwrap() else { p.fail("axes", "grid.axes array"); return; };
        p.eq("axes", (axes[0].string_field("name").unwrap(), axes[1].string_field("name").unwrap()), ("p".into(), "z".into()));
        let JsonValue::Arr(cells) = a.field("cells").unwrap() else { p.fail("cells", "cells array"); return; };
        p.eq("n", cells.len(), 4);
        for (c, want) in cells.iter().zip([[("p", "2"), ("z", "10")], [("p", "2"), ("z", "20")], [("p", "3"), ("z", "10")], [("p", "3"), ("z", "20")]]) { p.eq("bindings", pairs(c, "bindings"), want.into_iter().map(|(n, v)| (n.to_string(), v.to_string())).collect::<Vec<_>>()); p.eq("status", c.string_field("status").unwrap(), "ok".to_string()); }
        p.eq("sums", cells.iter().map(|c| pairs(c, "outputs").into_iter().find(|(n, _)| n == "s").unwrap().1).collect::<Vec<_>>(), ["12", "22", "13", "23"].into_iter().map(str::to_string).collect::<Vec<_>>());
        let s = a.field("summary").unwrap(); p.eq("total", (s.int_field("total").unwrap(), s.int_field("ok").unwrap(), s.int_field("mismatch").unwrap(), s.int_field("error").unwrap()), (4, 4, 0, 0));
        let sq = write(&d, "sq1.emath", SQ1); let (out, _, code) = sweep(&sq, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]); p.eq("exit", code, EXIT_OK as i32);
        let sqdoc = doc(&out); let JsonValue::Arr(cells) = sqdoc.field("cells").unwrap() else { p.fail("cells", "array"); return; };
        p.eq("squares", cells.iter().map(|c| pairs(c, "outputs").into_iter().find(|(n, _)| n == "y").unwrap().1).collect::<Vec<_>>(), ["4", "9", "25"].into_iter().map(str::to_string).collect::<Vec<_>>());
    });
    p.case("malformed", |p| {
        let d = dir("malformed"); let a = write(&d, "add2.emath", ADD2); let add2 = a.as_str();
        for args in [vec!["--function", "Add2", "--grid", "p"], vec!["--function", "Add2", "--grid", "p="], vec!["--function", "Add2", "--grid", "=1,2"], vec!["--function", "Add2", "--grid", "p=1,,2"], vec!["--function", "Add2", "--grid", "p=1,2", "--grid", "p=3"], vec!["--function", "Add2"], vec!["--grid", "p=1,2", "z=3"], vec!["--function", "Add2", "--grid", "p=1,2", add2, add2], vec!["--function", "Add2", "--grid", "p=1,2", "--expect", "s"], vec!["--function", "Add2", "--grid", "p=1,2", "--expect", "s=11", "--expect", "s=12"]] { let (_, err, code) = sweep(add2, &args); p.eq("usage", code, EXIT_USAGE as i32); p.contains("usage-text", &err, "usage: emath sweep"); }
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "q=1,2", "--json"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("E-EVAL-005", diag(&doc(&out)), "E-EVAL-005".to_string());
        p.eq("unbound", sweep(add2, &["--function", "Add2", "--grid", "p=1,2"]).2, EXIT_REFUSED as i32);
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "p=x", "z=3", "--json"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("E-EVAL-005", diag(&doc(&out)), "E-EVAL-005".to_string());
        let (out, _, code) = sweep(add2, &["--function", "Nope", "--grid", "p=1", "z=2", "--json"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("E-EVAL-002", diag(&doc(&out)), "E-EVAL-002".to_string());
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "p=1", "z=2", "--expect", "nope=1", "--json"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("E-EVAL-005", diag(&doc(&out)), "E-EVAL-005".to_string());
    });
    p.case("expectations", |p| {
        let d = dir("expect"); let a = write(&d, "add2.emath", ADD2); let add2 = a.as_str();
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "p=1,2", "z=10", "--expect", "s=11"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("lines", out, "Add2 p=1 z=10: 11 OK\nAdd2 p=2 z=10: 12 MISMATCH (want 11)\n".to_string());
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "p=1,2", "z=10", "--expect", "s=12", "--json"]); p.eq("exit", code, EXIT_REFUSED as i32);
        let expdoc = doc(&out); let JsonValue::Arr(cells) = expdoc.field("cells").unwrap() else { p.fail("cells", "array"); return; };
        p.eq("flip", (cells[0].string_field("status").unwrap(), cells[0].string_field("want").unwrap(), cells[0].string_field("got").unwrap(), cells[1].string_field("status").unwrap()), ("mismatch".into(), "12".into(), "11".into(), "ok".into()));
        let (out, _, code) = sweep(add2, &["--function", "Add2", "--grid", "p=1", "z=10", "--expect", "s=11"]); p.eq("exit", code, EXIT_OK as i32); p.eq("line", out, "Add2 p=1 z=10: 11 OK\n".to_string());
        let dual = write(&d, "dual.emath", DUAL);
        let (out, _, code) = sweep(&dual, &["--function", "Dual", "--grid", "p=2", "z=10", "--expect", "s=12", "--expect", "m=20"]); p.eq("exit", code, EXIT_OK as i32); p.eq("dual-ok", out, "Dual p=2 z=10: 12 20 OK\n".to_string());
        let (out, _, code) = sweep(&dual, &["--function", "Dual", "--grid", "p=2", "z=10", "--expect", "s=12", "--expect", "m=21"]); p.eq("exit", code, EXIT_REFUSED as i32); p.eq("dual-bad", out, "Dual p=2 z=10: 12 20 MISMATCH (want 21)\n".to_string());
    });
    p.case("determinism", |p| {
        let d = dir("determ"); let sq = write(&d, "sq1.emath", SQ1); let s = sq.as_str();
        let (a, _, ca) = sweep(s, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]); let (b, _, cb) = sweep(s, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]);
        p.eq("exit", (ca, cb), (EXIT_OK as i32, EXIT_OK as i32)); p.eq("bytes", a.clone(), b);
        p.demand("no-clock", ["timestamp", "elapsed", "duration", "generated_at", "now"].iter().all(|w| !a.to_ascii_lowercase().contains(w)), "no wall-clock field");
        let (oa, ob) = (d.join("a.json").to_string_lossy().into_owned(), d.join("b.json").to_string_lossy().into_owned());
        let (sa, _, _) = sweep(s, &["--function", "Sq", "--grid", "p=2,3,5", "--out", &oa]); let (sb, _, _) = sweep(s, &["--function", "Sq", "--grid", "p=2,3,5", "--out", &ob]);
        p.eq("files", std::fs::read_to_string(&d.join("a.json")).expect("a"), std::fs::read_to_string(&d.join("b.json")).expect("b")); p.eq("stdout", sa, sb);
    });
    p.case("acceptance", |p| {
        let results = std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../internal/proximity-prize/sweep-results.txt")).expect("results");
        let power = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../internal/proximity-prize/powerword-zero-sum.emath").to_string_lossy().into_owned();
        let mut n = 0;
        for line in results.lines() { let t = line.trim(); if t.is_empty() || t.starts_with('#') || t.starts_with("EXIT=") { continue; } let Some((f, r)) = t.split_once(" p=") else { p.fail("row", format!("unparsed {t}")); continue; }; let Some((pv, r)) = r.split_once(" z=") else { p.fail("row", format!("unparsed {t}")); continue; }; let Some((zv, _)) = r.split_once(": ") else { p.fail("row", format!("unparsed {t}")); continue; }; let (out, _, code) = sweep(&power, &["--function", f, "--grid", &format!("p={pv}"), &format!("z={zv}")]); p.eq(t, (out, code), (format!("{t}\n"), EXIT_OK as i32)); n += 1; }
        p.eq("rows", n, 15);
        let (out, _, code) = sweep(&power, &["--function", "n1_k3_n8", "--grid", "p=17,41", "z=9,27"]); p.eq("exit", code, EXIT_OK as i32);
        let lines: Vec<&str> = out.lines().collect(); p.eq("n", lines.len(), 4); p.eq("first", lines[0], "n1_k3_n8 p=17 z=9: 6 OK"); p.eq("last", lines[3], "n1_k3_n8 p=41 z=27: 6 OK");
    });
    p.finish();
}
