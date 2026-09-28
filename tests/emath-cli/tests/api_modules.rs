//! `emath api --search` finds ordinary module functions, not only
//! Language Image features: every exported name under the root's
//! `language/modules` and `language/domains`, with the import path, a
//! signature, a one-line summary, and the declaring file and line.
//! Private names never appear, even when their comments match.

mod common;

use emath_artifact::JsonValue;
use emath_test_harness::Probe;

const TOOL: &str = "# Probe tools for the api index.
# halve_probe splits n into two equal rational parts.
exports: twice_probe, halve_probe, ProbePoint

emath object ProbePoint:
    representation:
        x: Int

emath function twice_probe:
    # Double an integer without overflow checks.
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n + n

emath function halve_probe:
    inputs:
        n: Int
    outputs:
        result: Rat
    definitions:
        result = n / 2

emath function hidden_probe:
    # Private helper that also mentions overflow.
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n
";

const DOMAIN: &str = "exports: env_domain_marker

emath function env_domain_marker:
    # Domain marker for the api index.
    inputs:
        n: Int
        m: Int
    outputs:
        q: Int
        r: Int
    definitions:
        q = n
        r = m
";

fn scratch_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("emath-api-modules-{}", std::process::id()));
    let language = root.join("language");
    std::fs::create_dir_all(language.join("modules/algebra")).expect("modules dir");
    std::fs::create_dir_all(language.join("domains/probe")).expect("domains dir");
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../language")
        .canonicalize()
        .expect("shipped language");
    for entry in ["spec", "generated", "language.lock"] {
        let link = language.join(entry);
        if !link.exists() {
            std::os::unix::fs::symlink(shipped.join(entry), link).expect("link image source");
        }
    }
    std::fs::write(language.join("modules/algebra/probe.emath"), TOOL).expect("write tool");
    std::fs::write(language.join("domains/probe/marker.emath"), DOMAIN).expect("write domain");
    root
}

fn api(root: &std::path::Path, search: &str) -> Vec<JsonValue> {
    let output = std::process::Command::new(common::emath_bin())
        .args(["api", "--search", search, "--limit", "50", "--json"])
        .current_dir(root)
        .env("EMATH_ROOT", root)
        .output()
        .expect("run emath api");
    let text = String::from_utf8_lossy(&output.stdout);
    let parsed = emath_artifact::parse_json_document(&text)
        .unwrap_or_else(|error| panic!("api json: {error:?}\n{text}"));
    match parsed.field("module_exports") {
        Ok(JsonValue::Arr(rows)) => rows.clone(),
        other => panic!("module_exports must be an array, got {other:?}\n{text}"),
    }
}

fn named<'a>(rows: &'a [JsonValue], name: &str) -> Option<&'a JsonValue> {
    rows.iter()
        .find(|row| row.string_field("name").ok().as_deref() == Some(name))
}

fn field(row: Option<&JsonValue>, key: &str) -> String {
    row.and_then(|row| row.string_field(key).ok()).unwrap_or_default()
}

#[test]
fn api_indexes_module_exports() {
    let root = scratch_root();
    let mut p = Probe::new("emath api --search indexes module exports");

    p.case("name-search-reports-import-signature-summary-location", |p| {
        let rows = api(&root, "probe");
        let twice = named(&rows, "twice_probe");
        p.eq("import path", field(twice, "module"), "algebra.probe".to_string());
        p.eq("kind", field(twice, "kind"), "function".to_string());
        p.eq("signature", field(twice, "signature"), "twice_probe(n: Int) -> result: Int".to_string());
        p.eq("doc comment summary", field(twice, "summary"), "Double an integer without overflow checks.".to_string());
        p.demand(
            "file",
            field(twice, "file").ends_with("language/modules/algebra/probe.emath"),
            field(twice, "file"),
        );
        p.eq("line", twice.and_then(|row| row.int_field("line").ok()), Some(9));
        p.eq(
            "header fallback summary",
            field(named(&rows, "halve_probe"), "summary"),
            "halve_probe splits n into two equal rational parts.".to_string(),
        );
        let point = named(&rows, "ProbePoint");
        p.eq("object kind", field(point, "kind"), "object".to_string());
        p.eq("object signature", field(point, "signature"), "ProbePoint { x: Int }".to_string());
        p.demand("private absent", named(&rows, "hidden_probe").is_none(), "hidden_probe is private");
    });

    p.case("summary-search-skips-private-names", |p| {
        let rows = api(&root, "OVERFLOW");
        p.demand("exported match", named(&rows, "twice_probe").is_some(), "summary search is case-insensitive");
        p.demand("private absent", named(&rows, "hidden_probe").is_none(), "private summary must not match");
    });

    p.case("domain-modules-are-indexed", |p| {
        let rows = api(&root, "marker");
        let marker = named(&rows, "env_domain_marker");
        p.eq("import path", field(marker, "module"), "probe.marker".to_string());
        p.eq(
            "multi-output signature",
            field(marker, "signature"),
            "env_domain_marker(n: Int, m: Int) -> (q: Int, r: Int)".to_string(),
        );
    });

    let _ = std::fs::remove_dir_all(&root);
    p.finish();
}
