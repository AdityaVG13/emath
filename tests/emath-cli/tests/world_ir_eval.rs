//! World-IR builtin bridge: `eval --world <class>` over the 8
//! `emath_world_ir` builtin worlds. Ran red (E-GEN-092) before the bridge.

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

/// Fixture with the known-good custom-world section shape.
fn custom_source(body: &str) -> String {
    format!(
        "emath custom AlienGlyphs:\n    body:\n        {body}\n\n    \
         construct meaning:\n        explore:\n            free_symbolic\n            \
         Boolean_algebra\n            modular_numeric\n\n        protect:\n            \
         total\n            deterministic\n\n        keep:\n            pareto 8\n\n    \
         answer:\n        return interpretation_portfolio\n"
    )
}

fn write_fixture(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-world-ir-eval-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{name}.emath"));
    std::fs::write(&path, custom_source(body)).expect("write fixture");
    path
}

/// Spawns the built binary in `--json` mode: (parsed stdout, exit code).
fn eval_json(world: &str, body: &str) -> (emath_artifact::JsonValue, i32, String) {
    let name = world.replace('-', "_");
    let path = write_fixture(&name, body);
    run_json(&path, world)
}

fn run_json(path: &Path, world: &str) -> (emath_artifact::JsonValue, i32, String) {
    let output = Command::new(common::emath_bin())
        .args([
            "eval",
            &path.display().to_string(),
            "--world",
            world,
            "--json",
        ])
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(2);
    let parsed = emath_artifact::parse_json_document(&stdout)
        .unwrap_or_else(|error| panic!("valid JSON for --world {world}: {error}\n{stdout}"));
    (parsed, code, stdout)
}

#[test]
fn integer_ring_multiplies_declared_expression() {
    let (parsed, code, stdout) = eval_json("integer-ring", "a × b");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(parsed.string_field("answer").expect("answer"), "28");
    assert_eq!(
        parsed.string_field("world_name").expect("world"),
        "integer-ring"
    );
}

#[test]
fn commutative_monoid_adds_declared_expression() {
    let (parsed, code, stdout) = eval_json("commutative-monoid", "a ⋈ b");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(parsed.string_field("answer").expect("answer"), "11");
}

#[test]
fn cyclic_group_table_looks_up_rows() {
    let (parsed, code, stdout) = eval_json("cyclic-group-z3", "2 ⊕ 1");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(parsed.string_field("answer").expect("answer"), "0");
}

#[test]
fn finite_table_looks_up_rows() {
    let (parsed, code, stdout) = eval_json("finite-table", "1 ⊙ 2");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(parsed.string_field("answer").expect("answer"), "0");
}

#[test]
fn boolean_lattice_table_conjoins() {
    let (parsed, code, stdout) = eval_json("boolean-lattice", "⊤ ∧ ⊥");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(parsed.string_field("answer").expect("answer"), "false");
}

#[test]
fn free_term_keeps_the_term_structural() {
    let (parsed, code, stdout) = eval_json("free-term", "a ⋈ b");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(
        parsed.string_field("answer").expect("answer"),
        "apply(⋈,const(4),const(7))"
    );
}

#[test]
fn prose_semantics_stay_structural() {
    let (parsed, code, stdout) = eval_json("matrix-2x2", "a ⊞ b");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(
        parsed.string_field("answer").expect("answer"),
        "apply(⊞,const(4),const(7))"
    );
}

#[test]
fn all_eight_builtins_resolve() {
    for world in [
        "free-term",
        "finite-table",
        "commutative-monoid",
        "boolean-lattice",
        "integer-ring",
        "cyclic-group-z3",
        "matrix-2x2",
        "graph-union",
    ] {
        let path = write_fixture("resolve", "a ⋈ b");
        let (_, code, stdout) = run_json(&path, world);
        assert_ne!(code, 1, "world `{world}` must resolve: {stdout}");
    }
}

#[test]
fn unknown_world_outside_both_rosters_refused() {
    let path = write_fixture("unknown", "a ⋈ b");
    let (_, code, stdout) = run_json(&path, "no-such-world");
    assert_eq!(code, 1);
    assert!(stdout.contains("E-GEN-092"), "{stdout}");
}
