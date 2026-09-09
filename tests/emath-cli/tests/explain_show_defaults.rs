//! F8 `emath explain --show-defaults`.
mod common;
use common::cli;
use emath_test_harness::Probe;
fn repo(rel: &str) -> String { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).to_str().expect("utf8").to_string() }
#[test]
fn probe() {
    let mut p = Probe::new("show-defaults labels every default, overrides, stays deterministic and refused-aware");
    p.case("defaults", |p| { let (out, code) = cli(&["explain", "--show-defaults", &repo("tests/valid/square.emath")]); p.eq("exit", code, 0); for e in ["numeric-profile: strict-f64", "source: language default", "units-profile: permissive", "visibility: public", "outputs: all definitions", "compile: target rust, profile library, numeric strict-f64", "untyped-inputs: Float64", "source: planner default"] { p.contains(e, &out, e); } p.demand("no-override", !out.contains("units-profile: Square="), "plain file gains no override row"); });
    p.case("override", |p| { let (out, code) = cli(&["explain", "--show-defaults", &repo("tests/fixtures/language/intro/units-profile.emath")]); p.eq("exit", code, 0); p.contains("row", &out, "units-profile: Calibrated=publication (source: declaration attribute;"); });
    p.case("deterministic", |p| { let path = repo("tests/valid/square.emath"); p.eq("bytes", cli(&["explain", "--show-defaults", &path]).0, cli(&["explain", "--show-defaults", &path]).0); });
    p.case("refused", |p| { let (out, code) = cli(&["explain", "--show-defaults", &repo("tests/invalid/unit_mismatch.emath")]); p.eq("exit", code, 1); p.demand("no-table", !out.contains("effective defaults"), "refused file prints no table"); });
    p.case("json", |p| {
        let (out, code) = cli(&["explain", "--show-defaults", &repo("tests/fixtures/language/intro/units-profile.emath"), "--json"]);
        p.eq("exit", code, 0);
        let parsed = emath_artifact::parse_json_document(out.trim()).expect("json");
        p.eq("command", parsed.string_field("command").ok().as_deref(), Some("explain-show-defaults"));
        let emath_artifact::JsonValue::Arr(defs) = parsed.field("defaults").expect("defaults") else { p.fail("defaults", "must be array"); return; };
        p.eq("seven", defs.len(), 7);
        p.demand("sourced", defs.iter().all(|r| r.string_field("source").is_ok()), "every row names source");
        let emath_artifact::JsonValue::Arr(ov) = parsed.field("declaration_overrides").expect("overrides") else { p.fail("overrides", "must be array"); return; };
        p.eq("one", ov.len(), 1); if let Some(first) = ov.first() { p.eq("decl", first.string_field("declaration").ok().as_deref(), Some("Calibrated")); p.eq("value", first.string_field("value").ok().as_deref(), Some("publication")); }
    });
    p.finish();
}
