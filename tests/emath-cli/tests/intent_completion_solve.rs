//! `emath solve --check` lists labeled completions, never a naked float.
use emath_cli::{EXIT_OK, EXIT_REFUSED};
use emath_cli_lab::{run, solve_check_json_document};
use emath_syntax::expand_scratch;
use emath_test_harness::{Probe, boot};
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("solve check lists five labeled worlds, unknown labels refuse");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../language/examples/intro/solve_x2_eq_2.emath").display().to_string();
    let exp = expand_scratch("solve x^2 = 2\n");
    p.case("menu", |p| { p.eq("len", exp.solve.menu().len(), 5); p.eq("all", exp.solve.menu(), &emath_syntax::SolveWorld::ALL); });
    p.case("check", |p| { p.eq("text", run(&["solve".into(), "--check".into(), path.clone()]), EXIT_OK); p.eq("json", run(&["solve".into(), "--check".into(), "--json".into(), path.clone()]), EXIT_OK); });
    p.case("candidates", |p| {
        let parsed = emath_artifact::parse_json_document(&solve_check_json_document(&exp)).expect("json");
        p.eq("command", parsed.string_field("command").expect("command"), "solve".to_string());
        p.eq("ok", parsed.field("ok").expect("ok"), &emath_artifact::JsonValue::Bool(true));
        let emath_artifact::JsonValue::Arr(cands) = parsed.field("solve_candidates").expect("cands") else { p.fail("cands", "must be array"); return; };
        p.eq("len", cands.len(), 5);
        let labels: Vec<String> = cands.iter().map(|c| c.string_field("label").expect("label")).collect();
        p.eq("labels", labels, vec!["real-pm", "complex", "modular", "symbolic", "numeric"].into_iter().map(str::to_string).collect::<Vec<_>>());
        for (w, c) in emath_syntax::SolveWorld::ALL.iter().zip(cands) { p.eq("label", c.string_field("label").expect("label"), w.as_str().to_string()); p.eq("roundtrip", emath_syntax::SolveWorld::parse_label(&c.string_field("label").expect("label")), Some(*w)); p.eq("result", c.string_field("result_type").expect("t"), w.result_type().to_string()); for k in ["label", "result_type", "domain", "exactness", "method", "evidence_class"] { p.demand(k, c.string_field(k).is_ok(), "candidate key"); } }
    });
    p.case("apply", |p| { p.eq("known", run(&["solve".into(), "--apply".into(), "real-pm".into(), path.clone()]), EXIT_OK); });
    p.case("refusals", |p| {
        let bad = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/invalid/solve_x2_eq_2_unlabeled.emath").display().to_string();
        p.eq("unlabeled", run(&["solve".into(), "--check".into(), bad]), EXIT_REFUSED);
        let tmp = std::env::temp_dir().join("emath-solve-unknown.emath"); std::fs::write(&tmp, "solve x^2 = 2\n").expect("write");
        p.eq("unknown", run(&["solve".into(), "--apply".into(), "quaternion".into(), tmp.display().to_string()]), EXIT_REFUSED);
        let _ = std::fs::remove_file(&tmp);
    });
    p.finish();
}
