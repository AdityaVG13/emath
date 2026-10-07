use emath_wasm::run_op;

const VALID: &str = "emath function Increment:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 1
    tests:
        example <increment>:
            given x = 4
            expect result == 5
";

#[test]
fn live_operations_share_semantic_admission() {
    let refused = VALID.replacen("function Increment", "function _", 1);
    for op in ["check", "run", "generate", "inputs", "format"] {
        let response = run_op(op, &refused);
        assert!(
            response.contains("\"admitted\": false"),
            "{op} must not admit a reserved declaration name: {response}"
        );
        assert!(response.contains("E-NAME"), "{op}: {response}");
    }
}

#[test]
fn generation_returns_a_crate_with_distinct_function_entries() {
    let source = format!("{}\n{}", VALID, VALID.replace("Increment", "Second").replace("x + 1", "x + 2").replace("result == 5", "result == 6"));
    let response = run_op("generate", &source);
    let json = emath_artifact::parse_json_document(&response).unwrap();
    let emath_artifact::JsonValue::Arr(files) = json.field("files").unwrap() else {
        panic!("generation must return files: {response}");
    };
    let rust = files.iter().find(|file| file.string_field("path").as_deref() == Ok("src/lib.rs")).unwrap().string_field("content").unwrap();
    assert!(rust.contains("pub fn Increment(") && rust.contains("pub fn Second("), "multiple functions cannot share an `entry` symbol");
    assert!(files.iter().any(|file| file.string_field("path").as_deref() == Ok("Cargo.toml")), "generation must return the manifest needed to compile its source");
}

#[test]
fn constructor_execution_generation_and_dispatch_policies() {
    for op in ["check", "run", "generate", "inputs", "format"] {
        let response = run_op(op, VALID);
        assert!(response.contains("\"admitted\": true"), "{op}: {response}");
    }
    let mut payload = emath_artifact::JsonWriter::object();
    payload.string("source", VALID);
    payload.object_field("given", "{\"x\":4}");
    let response = run_op("run", &payload.finish());
    assert!(response.contains("\"Increment\": \"5\""), "{response}");
    assert!(response.contains("\"passed\": true"), "{response}");
    for op in ["plan", "mig", "solve_candidates"] {
        assert!(run_op(op, VALID).contains("E-KIND-GONE"), "{op}");
    }
    for payload in [
        "{\"source\":\"\",\"given\":[]}",
        "{\"source\":\"\",\"source\":\"\"}",
    ] {
        assert!(run_op("run", payload).contains("\"ok\": false"));
    }
    assert!(run_op("unknown", VALID).contains("\"ok\": false"));
}
