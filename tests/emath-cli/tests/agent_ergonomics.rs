//! Regression pins for the agent-ergonomics passes (11-18): safe
//! mutation dry-run gating, stdin pipelines, the diagnostics explainer,
//! uniform JSON status envelopes, the catalog matrix, doctor probes,
//! and the next-action engine.
//!
//! Each case pins a contract an agent consumes mechanically (exact exit
//! codes, exact E-* codes, exact JSON keys); a regression in any of
//! them breaks scripted workflows, not just cosmetics.
mod common;

use emath_test_harness::Probe;
use std::io::Write;
use std::process::{Command, Stdio};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("emath_agent_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Run the CLI with `stdin_text` piped to stdin; returns
/// (stdout, combined, exit code). Data is on stdout, diagnostics on
/// stderr: JSON assertions must read stdout alone, E-code assertions
/// read the combined stream.
fn cli_stdin(args: &[&str], stdin_text: &str) -> (String, String, i32) {
    let mut child = Command::new(common::emath_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn emath binary");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin_text.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for emath");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let combined = format!(
        "{}{}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    (stdout, combined, output.status.code().unwrap_or(-1))
}

/// Run the CLI with no stdin; returns (stdout, combined, exit code).
fn cli(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(common::emath_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let combined = format!(
        "{}{}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    (stdout, combined, output.status.code().unwrap_or(-1))
}

fn top_level_string(body: &str, field: &str) -> Option<String> {
    let parsed = emath_artifact::parse_json_document(body).ok()?;
    parsed.string_field(field).ok()
}

fn top_level_bool(body: &str, field: &str) -> Option<bool> {
    let parsed = emath_artifact::parse_json_document(body).ok()?;
    match parsed.field(field) {
        Ok(emath_artifact::JsonValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

const VALID_SOURCE: &str = "emath function Greeter:\n    inputs:\n        x: Float64\n\n    definitions:\n        y = x\n";

#[test]
fn probe() {
    let mut p = Probe::new("agent ergonomics: dry-run gating, stdin pipelines, diagnostics explainer, uniform envelopes, catalog matrix, doctor probes, next-action");

    // --- Pass 12: safe mutation gating ---------------------------------
    p.case("new-dry-run-writes-nothing", |p| {
        let d = scratch("new_dry");
        let out = d.join("proj");
        let (text, _, code) = cli(&["new", "demo", "--out", out.to_str().expect("utf8"), "--dry-run"]);
        p.eq("exit", code, 0);
        p.contains("plan", &text, "dry-run: emath new `demo`");
        p.eq("no-disk-writes", out.exists(), false);
        p.contains("planned-manifest", &text, "emath-package.toml");
        p.contains("planned-main", &text, "src/main.emath");
    });
    p.case("new-collision-refusal-and-force", |p| {
        let d = scratch("new_force");
        let out = d.join("proj");
        let out_path = out.to_str().expect("utf8").to_string();
        p.eq("first-creates", cli(&["new", "demo", "--out", &out_path]).2, 0);
        let (text, combined, code) = cli(&["new", "demo", "--out", &out_path]);
        p.eq("collision-exit-5-safety", code, 5);
        p.contains("collision-code", &combined, "E-TLT-011");
        let (text_dry, _, code_dry) = cli(&["new", "demo", "--out", &out_path, "--dry-run"]);
        p.eq("dry-run-still-ok", code_dry, 0);
        p.contains("dry-run-warns", &text_dry, "would collide");
        p.eq("force-overrides", cli(&["new", "demo", "--out", &out_path, "--force"]).2, 0);
        let _ = text;
    });
    p.case("new-dry-run-json", |p| {
        let d = scratch("new_json");
        let out = d.join("proj");
        let (text, _, code) = cli(&["new", "demo", "--out", out.to_str().expect("utf8"), "--dry-run", "--json"]);
        p.eq("exit", code, 0);
        p.eq("status", top_level_string(&text, "status").as_deref(), Some("ok"));
        p.eq("dry-run-flag", top_level_bool(&text, "dry_run"), Some(true));
        p.contains("planned", &text, "planned_files");
        p.eq("no-overwrite", top_level_bool(&text, "will_overwrite"), Some(false));
    });
    p.case("build-dry-run-no-artifacts", |p| {
        let d = scratch("build_dry");
        let src = d.join("m.emath");
        std::fs::write(&src, VALID_SOURCE).expect("write source");
        let artifacts = d.join("artifacts");
        let (text, _, code) = cli(&["build", src.to_str().expect("utf8"), "--out", artifacts.to_str().expect("utf8"), "--dry-run", "--json"]);
        p.eq("exit", code, 0);
        p.contains("plan-ids", &text, "plan_ids");
        p.eq("no-artifacts", artifacts.exists(), false);
        p.eq("status", top_level_string(&text, "status").as_deref(), Some("ok"));
    });
    p.case("migrate-dry-run-no-receipt", |p| {
        let d = scratch("migrate_dry");
        let src = d.join("m.emath");
        std::fs::write(&src, VALID_SOURCE).expect("write source");
        let (text, _, code) = cli(&["migrate", src.to_str().expect("utf8"), "--dry-run", "--json"]);
        p.eq("exit", code, 0);
        p.eq("status", top_level_string(&text, "status").as_deref(), Some("ok"));
        p.contains("rules", &text, "rules_applied");
        p.eq("no-receipt", !d.join("m.migrate.json").exists(), true);
    });

    // --- Pass 13: stdin pipelines --------------------------------------
    p.case("check-stdin-parity-with-file", |p| {
        let d = scratch("stdin_parity");
        let src = d.join("m.emath");
        std::fs::write(&src, VALID_SOURCE).expect("write source");
        let (file_json, _, _) = cli(&["check", src.to_str().expect("utf8"), "--json"]);
        let (stdin_json, _, code) = cli_stdin(&["check", "-", "--json"], VALID_SOURCE);
        p.eq("exit", code, 0);
        let file_pkg = top_level_string(&file_json, "package");
        let stdin_pkg = top_level_string(&stdin_json, "package");
        p.eq("package-id-parity", stdin_pkg, file_pkg);
    });
    p.case("check-stdin-refuses-verify-data", |p| {
        let (_, combined, code) = cli_stdin(&["check", "-", "--verify-data"], VALID_SOURCE);
        p.eq("exit-2-usage", code, 2);
        p.contains("code", &combined, "E-CLI-USAGE");
    });
    p.case("fmt-stdin-canonical-round-trip", |p| {
        let (text, _, code) = cli_stdin(&["fmt", "-"], VALID_SOURCE);
        p.eq("exit", code, 0);
        p.contains("canonical", &text, "canonical form (lossless round-trip)");
    });

    // --- Pass 14: diagnostics explainer --------------------------------
    p.case("explain-code-registry", |p| {
        let (text, _, code) = cli(&["explain", "E-TLT-011", "--json"]);
        p.eq("exit", code, 0);
        p.eq("known", top_level_bool(&text, "known"), Some(true));
        p.contains("remediation", &text, "--force");
        p.eq("schema", top_level_string(&text, "schema").as_deref(), Some("emath.diagnostic-explanation"));
        let (_, combined, code_unknown) = cli(&["explain", "E-NOPE-999"]);
        p.eq("unknown-exit-2", code_unknown, 2);
        p.contains("unknown-code", &combined, "unknown diagnostic code");
        let (list, _, code_list) = cli(&["explain", "--list-codes", "--json"]);
        p.eq("list-exit", code_list, 0);
        p.contains("list-schema", &list, "emath.diagnostic-registry");
        p.contains("list-rows", &list, "E-RUN-STATE");
    });

    // --- Pass 15: uniform status envelope ------------------------------
    p.case("status-key-on-ok-and-refusal", |p| {
        let d = scratch("status_env");
        let src = d.join("m.emath");
        std::fs::write(&src, VALID_SOURCE).expect("write source");
        let (ok_json, _, _) = cli(&["check", src.to_str().expect("utf8"), "--json"]);
        p.eq("check-status", top_level_string(&ok_json, "status").as_deref(), Some("ok"));
        let (refused_json, _, _) = cli(&["build", d.join("missing.emath").to_str().expect("utf8"), "--json"]);
        p.eq("refusal-status", top_level_string(&refused_json, "status").as_deref(), Some("refused"));
    });

    // --- Pass 17: doctor probes ----------------------------------------
    p.case("doctor-covers-epoch-and-language-root", |p| {
        let (text, _, code) = cli(&["doctor", "--json"]);
        p.eq("exit", code, 0);
        p.contains("epoch-row", &text, "SOURCE_DATE_EPOCH");
        p.contains("language-root-row", &text, "language-root");
        p.eq("status", top_level_string(&text, "status").as_deref(), Some("ok"));
        p.eq("command", top_level_string(&text, "command").as_deref(), Some("doctor"));
    });

    // --- Pass 18: catalog matrix ---------------------------------------
    p.case("catalog-json-machine-matrix", |p| {
        let (text, _, code) = cli(&["catalog", "--json"]);
        p.eq("exit", code, 0);
        p.eq("schema", top_level_string(&text, "schema").as_deref(), Some("emath.catalog"));
        for needle in ["\"name\": \"build\"", "\"flags\"", "--dry-run", "\"aliases\"", "\"examples\""] {
            p.contains(needle, &text, needle);
        }
    });

    // --- Pass 11: next-action engine -----------------------------------
    p.case("next-json-shape", |p| {
        let d = scratch("next_shape");
        let src = d.join("m.emath");
        std::fs::write(&src, VALID_SOURCE).expect("write source");
        let (text, _, code) = cli(&["next", src.to_str().expect("utf8"), "--json"]);
        p.eq("exit", code, 0);
        p.eq("schema", top_level_string(&text, "schema").as_deref(), Some("emath.next"));
        for key in ["\"action\"", "\"priority\"", "\"command\"", "\"claim_command\""] {
            p.contains(key, &text, key);
        }
    });

    p.finish();
}
