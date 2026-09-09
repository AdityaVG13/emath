//! `emath explain E-LAW-001` renders a checker-backed Cayley witness.
use emath_cli::{EXIT_OK, explain_inspections};
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("explain renders law witness and plan-inspection JSON");
    p.case("law", |p| { p.eq("ascii", run(&["explain".into(), "E-LAW-001".into()]), EXIT_OK); p.eq("json", run(&["explain".into(), "E-LAW-001".into(), "--json".into()]), EXIT_OK); });
    p.case("file-json", |p| {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../language/examples/intro/autodiff.emath");
        p.eq("exit-json", run(&["explain".into(), path.to_string_lossy().into_owned(), "--json".into()]), EXIT_OK);
        let ins = explain_inspections(&path).expect("inspections");
        p.demand("nonempty", !ins.is_empty(), "autodiff must produce an inspection");
        let json = ins[0].to_json();
        for needle in ["\"schema\": \"emath.plan-explanation v1\"", "\"policy\"", "\"candidates\"", "\"artifact_class\""] { p.contains(needle, &json, needle); }
        p.demand("no-handrolled", !json.contains("\"symbol_note\""), "schema object only");
        p.contains("policy-line", &ins[0].explain(), "policy:");
    });
    p.case("intro-checks", |p| { for rel in ["tests/fixtures/language/intro/scratch.emath", "language/examples/intro/autodiff.emath", "language/examples/numerical/heat-rod-sim.emath"] { let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).to_string_lossy().into_owned(); p.eq(rel, run(&["check".into(), path]), EXIT_OK); } });
    p.finish();
}
