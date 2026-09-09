//! Conformance for emath eval over admitted function specs: numeric receipts or typed E-EVAL refusals.
use std::path::PathBuf;

mod common;
use std::process::Command;

use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED};
use emath_cli_lab::run;
use emath_test_harness::Probe;

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-cli-eval-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_fixture(dir: &PathBuf, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

fn hello_square() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/language/intro/hello-square.emath")
        .to_string_lossy()
        .into_owned()
}

const SQUARE_SPEC: &str = r#"emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
"#;

const TWO_FUNCTION_SPEC: &str = r#"emath function A:
    inputs:
        x: Float64
    outputs:
        r: Float64
    definitions:
        r = x + 1.0

emath function B:
    inputs:
        x: Float64
    outputs:
        r: Float64
    definitions:
        r = x * 2.0
"#;

const TWO_INPUT_SPEC: &str = r#"emath function Add:
    inputs:
        a: Float64
        b: Float64
    outputs:
        r: Float64
    definitions:
        r = a + b
"#;

const FAILING_EXAMPLE_SPEC: &str = r#"emath function Wrong:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    tests:
        example <expects_one>:
            given x = 3
            expect y == 1
"#;

const MODEL_ONLY_SPEC: &str = r#"emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        der(x) = -k * x
"#;

const SIBLING_BINDER_SPEC: &str = r#"emath function count_to:
    inputs:
        y: Int
    outputs:
        r: Nat
    definitions:
        r = sum j in 0..y: 1

emath function count_below:
    inputs:
        y: Int
    outputs:
        r: Nat
    definitions:
        r = sum j in 0..6 if j < y: 1

emath function caller:
    inputs:
        x: Int
    outputs:
        a: Int
        b: Int
    definitions:
        a = count_to(x)
        b = count_below(x)
"#;

const MULTI_BINDER_SPEC: &str = r#"emath function grid:
    inputs:
        n: Int
        m: Int
    outputs:
        cells: Int
    definitions:
        cells = sum i in 0..n, j in 0..m: 1

emath function weighted:
    inputs:
        n: Int
    outputs:
        s: Int
    definitions:
        s = sum i in 0..n, j in 0..n: i * j

emath function filtered:
    inputs:
        n: Int
    outputs:
        evens: Int
    definitions:
        evens = sum i in 0..n, j in 0..n if int_rem(i + j, 2) == 0: 1

emath function exists_pair:
    inputs:
        n: Int
    outputs:
        found: Bool
    definitions:
        found = exists i in 0..n, j in 0..n: i * j == 6

emath function triangle:
    inputs:
        n: Int
    outputs:
        t: Int
    definitions:
        t = sum i in 0..n, j in 0..i: 1
"#;

const OUTER_REF_SPEC: &str = r#"emath function outer_ref:
    inputs:
        n: Int
    outputs:
        r: Int
    definitions:
        r = sum i in 0..n, j in 0..n if j == i: 1
"#;

const INT_VECTOR_SPEC: &str = r#"emath function witness_sum:
    inputs:
        v: Vector[Int]
    outputs:
        s: Int
    definitions:
        s = v[0] + v[1] + v[2]

emath function nat_code:
    inputs:
        c: Vector[Nat]
    outputs:
        m: Int
    definitions:
        m = c[0] * 10 + c[1]
"#;

fn eval_json(path: &str, extra: &[&str]) -> (emath_artifact::JsonValue, CliExit, String) {
    let output = Command::new(common::emath_lab_bin())
        .arg("eval")
        .arg(path)
        .args(extra)
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = match output.status.code() {
        Some(0) => EXIT_OK,
        Some(1) => EXIT_REFUSED,
        _ => CliExit::Usage,
    };
    let parsed = match emath_artifact::parse_json_document(&stdout) {
        Ok(parsed) => parsed,
        Err(error) => panic!("stdout must be valid JSON: {error} {stdout}"),
    };
    (parsed, code, stdout)
}

fn output_value(parsed: &emath_artifact::JsonValue, key: &str) -> String {
    match parsed.field("outputs").expect("outputs") {
        emath_artifact::JsonValue::Obj(fields) => fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| match v {
                emath_artifact::JsonValue::Str(s) => s.clone(),
                other => panic!("outputs entry must be string, got {other:?}"),
            })
            .expect("output key"),
        other => panic!("outputs must be object, got {other:?}"),
    }
}

#[test]
fn eval_function_specs() {
    emath_syntax::install_source_parser();
    let mut p = Probe::new("emath eval over admitted function specs returns numeric receipts or typed refusals");
    let dir = fixture_dir("probe");
    let square = write_fixture(&dir, "square.emath", SQUARE_SPEC).to_string_lossy().into_owned();
    let add = write_fixture(&dir, "add.emath", TWO_INPUT_SPEC).to_string_lossy().into_owned();
    let two = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC).to_string_lossy().into_owned();
    let wrong = write_fixture(&dir, "wrong.emath", FAILING_EXAMPLE_SPEC).to_string_lossy().into_owned();
    let decay = write_fixture(&dir, "decay.emath", MODEL_ONLY_SPEC).to_string_lossy().into_owned();
    let caller = write_fixture(&dir, "caller.emath", SIBLING_BINDER_SPEC).to_string_lossy().into_owned();
    let multi = write_fixture(&dir, "multi.emath", MULTI_BINDER_SPEC).to_string_lossy().into_owned();
    let outer = write_fixture(&dir, "outer.emath", OUTER_REF_SPEC).to_string_lossy().into_owned();
    let ivec = write_fixture(&dir, "ivec.emath", INT_VECTOR_SPEC).to_string_lossy().into_owned();
    let hello = hello_square();
    p.case("oracle", |p| {
        let (parsed, code, stdout) = eval_json(&hello, &["--json"]);
        p.eq("code", code, EXIT_OK);
        p.eq("schema", parsed.string_field("schema").expect("schema"), "emath.eval-function".to_string());
        p.eq("function", parsed.string_field("function").expect("function"), "Square".to_string());
        let inputs = match parsed.field("inputs").expect("inputs") {
            emath_artifact::JsonValue::Obj(fields) => fields,
            other => panic!("inputs must be object, got {other:?}"),
        };
        let x = inputs.iter().find(|(k, _)| k == "x").map(|(_, v)| match v {
            emath_artifact::JsonValue::Str(s) => s.clone(),
            other => panic!("input must be string, got {other:?}"),
        }).expect("x");
        p.eq("x", x, "3.0".to_string());
        p.eq("y", output_value(&parsed, "y"), "9.0".to_string());
        let _ = stdout;
    });
    p.case("failing-example", |p| {
        let (_, code, stdout) = eval_json(&wrong, &["--json"]);
        p.eq("code", code, EXIT_REFUSED);
        p.contains("e007", &stdout, "E-EVAL-007");
    });
    p.case("determinism", |p| {
        let (_, _, first) = eval_json(&add, &["--set", "a=2", "--set", "b=3", "--json"]);
        let (parsed, _, second) = eval_json(&add, &["--set", "a=2", "--set", "b=3", "--json"]);
        p.eq("repeat", first.clone(), second.clone());
        let (_, perm_code, perm_stdout) = eval_json(&add, &["--set", "b=3", "--set", "a=2", "--json"]);
        p.eq("perm-code", perm_code, EXIT_OK);
        p.eq("perm-receipt", perm_stdout, first);
        p.eq("sum", output_value(&parsed, "r"), "5.0".to_string());
    });
    p.case("set-closure", |p| {
        for (name, path, sets, ecode) in [
            ("dup", add.as_str(), vec!["--set", "a=2", "--set", "a=3", "--json"], "E-EVAL-005"),
            ("banana", add.as_str(), vec!["--set", "a=banana", "--json"], "E-EVAL-005"),
            ("vec-float", add.as_str(), vec!["--set", "a=[1,2]", "--set", "b=3", "--json"], "E-EVAL-006"),
            ("world-flag", hello.as_str(), vec!["--world", "free_symbolic", "--set", "x=3", "--json"], "E-EVAL-008"),
        ] {
            let (_, code, stdout) = eval_json(path, &sets);
            p.eq(format!("{name}-code"), code, EXIT_REFUSED);
            p.contains(format!("{name}-ecode"), &stdout, ecode);
        }
    });
    p.case("numeric", |p| {
        let (parsed, code, _) = eval_json(&hello, &["--set", "x=3", "--json"]);
        p.eq("code3", code, EXIT_OK);
        p.eq("sq3", output_value(&parsed, "y"), "9.0".to_string());
        p.demand("meaning", parsed.field("meaning_id").is_ok(), "receipt carries meaning_id");
        let (parsed4, code4, _) = eval_json(&hello, &["--set", "x=4", "--json"]);
        p.eq("code4", code4, EXIT_OK);
        p.eq("sq4", output_value(&parsed4, "y"), "16.0".to_string());
    });
    p.case("refusals", |p| {
        for (name, path, args, ecode) in [
            ("missing", add.as_str(), vec!["--set", "a=2", "--json"], "E-EVAL-004"),
            ("ambiguous", two.as_str(), vec!["--set", "x=2", "--json"], "E-EVAL-003"),
            ("unknown", two.as_str(), vec!["--function", "Nope", "--set", "x=2", "--json"], "E-EVAL-002"),
            ("model", decay.as_str(), vec!["--set", "k=1", "--json"], "E-EVAL-001"),
        ] {
            let (_, code, stdout) = eval_json(path, &args);
            p.eq(format!("{name}-code"), code, EXIT_REFUSED);
            p.contains(format!("{name}-ecode"), &stdout, ecode);
        }
        let (parsed, code, _) = eval_json(&two, &["--function", "B", "--set", "x=2", "--json"]);
        p.eq("named-code", code, EXIT_OK);
        p.eq("named-value", output_value(&parsed, "r"), "4.0".to_string());
    });
    p.case("hint", |p| {
        let (_, code, stdout) = eval_json(&two, &["--set", "x=2", "--json"]);
        p.eq("code", code, EXIT_REFUSED);
        p.contains("count", &stdout, "2 function declarations share this file");
        p.contains("flag", &stdout, "--function");
        p.contains("cand-a", &stdout, "A");
        p.contains("cand-b", &stdout, "B");
        p.demand("no-cascade", !stdout.contains("E-GEN-080") && !stdout.contains("E-SYN-2"), "no genesis cascade");
        let (_, sole_code, _) = eval_json(&square, &["--set", "x=3", "--json"]);
        p.eq("sole", sole_code, EXIT_OK);
        let genesis = write_fixture(&dir, "gen.emath", "probe = 1 + 1").to_string_lossy().into_owned();
        let (_, gen_code, _) = eval_json(&genesis, &["--json"]);
        p.eq("genesis", gen_code, EXIT_OK);
    });
    p.case("exit-codes", |p| {
        let sq: Vec<String> = vec![square.clone(), "--set".to_string(), "x=3".to_string()];
        let mut argv: Vec<String> = vec!["eval".to_string(), sq[0].clone(), sq[1].clone(), sq[2].clone()];
        p.eq("square", run(&argv), EXIT_OK);
        argv.push("--json".to_string());
        p.eq("square-json", run(&argv), EXIT_OK);
        p.eq("missing", run(&vec!["eval".to_string(), add.clone(), "--set".to_string(), "a=2".to_string()]), EXIT_REFUSED);
        p.eq("ambiguous", run(&vec!["eval".to_string(), two.clone(), "--set".to_string(), "x=2".to_string()]), EXIT_REFUSED);
        p.eq("unknown", run(&vec!["eval".to_string(), two.clone(), "--function".to_string(), "Nope".to_string(), "--set".to_string(), "x=2".to_string()]), EXIT_REFUSED);
    });
    p.case("sibling-binder", |p| {
        let (parsed, code, _) = eval_json(&caller, &["--function", "caller", "--set", "x=4", "--json"]);
        p.eq("code4", code, EXIT_OK);
        p.eq("a4", output_value(&parsed, "a"), "4".to_string());
        p.eq("b4", output_value(&parsed, "b"), "4".to_string());
        let (parsed2, code2, _) = eval_json(&caller, &["--function", "caller", "--set", "x=2", "--json"]);
        p.eq("code2", code2, EXIT_OK);
        p.eq("b2", output_value(&parsed2, "b"), "2".to_string());
    });
    p.case("multi-binder", |p| {
        for (name, func, sets, key, expected) in [
            ("grid", "grid", vec!["n=3", "m=4"], "cells", "12"),
            ("weighted", "weighted", vec!["n=4"], "s", "36"),
            ("filtered", "filtered", vec!["n=3"], "evens", "5"),
            ("exists", "exists_pair", vec!["n=4"], "found", "true"),
            ("triangle", "triangle", vec!["n=4"], "t", "6"),
        ] {
            let owned: Vec<String> = sets.iter().map(|s| s.to_string()).collect();
            let mut full: Vec<&str> = vec!["--function", func];
            for o in &owned {
                full.push("--set");
                full.push(o);
            }
            full.push("--json");
            let (parsed, code, _) = eval_json(&multi, &full);
            p.eq(format!("{name}-code"), code, EXIT_OK);
            p.eq(format!("{name}-value"), output_value(&parsed, key), expected.to_string());
        }
    });
    p.case("outer-ref", |p| {
        for (set, expected) in [("n=4", "4"), ("n=2", "2")] {
            let (parsed, code, _) = eval_json(&outer, &["--function", "outer_ref", "--set", set, "--json"]);
            p.eq(format!("code-{expected}"), code, EXIT_OK);
            p.eq(format!("value-{expected}"), output_value(&parsed, "r"), expected.to_string());
        }
    });
    p.case("int-vector", |p| {
        let (parsed, code, _) = eval_json(&ivec, &["--function", "witness_sum", "--set", "v=[3, 4, 5]", "--json"]);
        p.eq("witness-code", code, EXIT_OK);
        p.eq("witness-sum", output_value(&parsed, "s"), "12".to_string());
        let (nat, nat_code, _) = eval_json(&ivec, &["--function", "nat_code", "--set", "c=[4, 2]", "--json"]);
        p.eq("nat-code", nat_code, EXIT_OK);
        p.eq("nat-value", output_value(&nat, "m"), "42".to_string());
        for (name, func, set) in [
            ("frac", "witness_sum", "v=[1, 2.5, 3]"),
            ("neg", "nat_code", "c=[-1, 2]"),
            ("float", "witness_sum", "v=[1.5, 2, 3]"),
        ] {
            let (_, strict_code, stdout) = eval_json(&ivec, &["--function", func, "--set", set, "--json"]);
            p.eq(format!("{name}-code"), strict_code, EXIT_REFUSED);
            p.contains(format!("{name}-ecode"), &stdout, "E-EVAL-006");
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
    p.finish();
}
