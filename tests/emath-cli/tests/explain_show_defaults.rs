//! File/plan explanation is not a constructor command. Diagnostic codes remain.
mod common;
use common::cli;
use emath_test_harness::Probe;
fn repo(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_str()
        .expect("utf8")
        .to_string()
}
#[test]
fn probe() {
    let mut p = Probe::new(
        "explain file/plan refuses; diagnostic codes remain constructor help",
    );
    p.case("file-refuses", |p| {
        let (out, code) = cli(&[
            "explain",
            "--show-defaults",
            &repo("tests/valid/square.emath"),
        ]);
        p.eq("exit", code, 2);
        p.contains("gone", &out, "E-KIND-GONE");
        p.demand(
            "no-planner-table",
            !out.contains("planner default"),
            out.clone(),
        );
    });
    p.case("code-lookup", |p| {
        let (out, code) = cli(&["explain", "E-TYPE-002", "--json"]);
        p.eq("exit", code, 0);
        p.contains("schema", &out, "emath.diagnostic-explanation");
        // A language code, not only the CLI's own, explains what it means.
        p.contains("language-code-known", &out, "\"known\": true");
        p.contains("language-code-meaning", &out, "unknown variable");
        let (out, code) = cli(&["explain", "E-NAME-022"]);
        p.eq("text-exit", code, 0);
        p.contains("text-meaning", &out, "duplicate declaration name in one admission");
        let (out, code) = cli(&["explain", "E-NOPE-999"]);
        p.eq("unknown-exit", code, 2);
        p.contains("unknown-named", &out, "unknown diagnostic code");
    });
    p.case("list-codes", |p| {
        let (out, code) = cli(&["explain", "--list-codes", "--json"]);
        p.eq("exit", code, 0);
        p.contains("registry", &out, "emath.diagnostic-registry");
    });
    p.finish();
}
