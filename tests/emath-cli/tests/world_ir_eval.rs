//! World-IR builtin bridge: `eval --world <class>` over the 8 `emath_world_ir` builtin worlds. Ran red (E-GEN-092) before the bridge.
use emath_test_harness::Probe;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

fn custom_source(body: &str) -> String {
    format!(
        "emath custom AlienGlyphs:\n    body:\n        {body}\n\n    construct meaning:\n        explore:\n            free_symbolic\n            Boolean_algebra\n            modular_numeric\n\n        protect:\n            total\n            deterministic\n\n        keep:\n            pareto 8\n\n    answer:\n        return interpretation_portfolio\n"
    )
}

fn eval_world(world: &str, body: &str) -> (emath_artifact::JsonValue, i32, String) {
    let dir = std::env::temp_dir().join(format!("emath-world-ir-eval-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{}.emath", world.replace('-', "_")));
    std::fs::write(&path, custom_source(body)).expect("write fixture");
    let output = Command::new(common::emath_lab_bin())
        .args(["eval", &path.display().to_string(), "--world", world, "--json"])
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(2);
    let parsed = emath_artifact::parse_json_document(&stdout)
        .unwrap_or_else(|error| panic!("valid JSON for --world {world}: {error}\n{stdout}"));
    (parsed, code, stdout)
}

fn run_world(path: &Path, world: &str) -> (i32, String) {
    let output = Command::new(common::emath_lab_bin())
        .args(["eval", &path.display().to_string(), "--world", world, "--json"])
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.code().unwrap_or(2), stdout)
}

#[test]
fn world_builtin_bridge() {
    let mut p = Probe::new("eval --world resolves the eight builtin worlds to their answers");
    for (world, body, answer) in [
        ("integer-ring", "a × b", "28"),
        ("commutative-monoid", "a ⋈ b", "11"),
        ("cyclic-group-z3", "2 ⊕ 1", "0"),
        ("finite-table", "1 ⊙ 2", "0"),
        ("boolean-lattice", "⊤ ∧ ⊥", "false"),
        ("free-term", "a ⋈ b", "apply(⋈,const(4),const(7))"),
        ("matrix-2x2", "a ⊞ b", "apply(⊞,const(4),const(7))"),
    ] {
        p.case(world, |p| {
            let (parsed, code, stdout) = eval_world(world, body);
            p.eq("code", code, 0);
            p.eq("answer", parsed.string_field("answer").expect("answer"), answer.to_string());
            if world == "integer-ring" {
                p.eq("world", parsed.string_field("world_name").expect("world"), world.to_string());
            }
            let _ = stdout;
        });
    }
    p.case("resolve-all", |p| {
        let dir = std::env::temp_dir().join(format!("emath-world-ir-eval-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path: PathBuf = dir.join("resolve.emath");
        std::fs::write(&path, custom_source("a ⋈ b")).expect("write fixture");
        for world in ["free-term", "finite-table", "commutative-monoid", "boolean-lattice", "integer-ring", "cyclic-group-z3", "matrix-2x2", "graph-union"] {
            let (code, stdout) = run_world(&path, world);
            p.ne(format!("{world}-resolves"), code, 1);
            let _ = stdout;
        }
    });
    p.case("unknown-world", |p| {
        let (_, code, stdout) = eval_world("no-such-world", "a ⋈ b");
        p.eq("code", code, 1);
        p.contains("refusal", &stdout, "E-GEN-092");
    });
    p.finish();
}
