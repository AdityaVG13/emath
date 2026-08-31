//! Intent completion is consistent across the syntax and browser APIs.

use emath_syntax::{SolveIntent, SolveWorld, apply_solve_candidate, expand_scratch};

#[test]
fn intent_completion_solve_yields_labeled_candidates_not_a_float() {
    let expansion = expand_scratch("solve x^2 = 2\n");
    assert_eq!(expansion.solve, SolveIntent::Unlabeled);
    assert_eq!(expansion.solve.menu(), &SolveWorld::ALL);
    assert!(
        expansion
            .solve
            .menu()
            .iter()
            .all(|world| !world.as_str().is_empty()
                && !world.result_type().is_empty()
                && !world.method().is_empty())
    );
    assert!(!expansion.expanded.contains("1.414"));
}

#[test]
fn wasm_solve_candidates_are_labeled_world_result_bundles() {
    let json = emath_wasm::run_op("solve_candidates", "solve x^2 = 2\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"schema\":\"emath.world-result\""), "{json}");
    for label in ["real-pm", "complex", "modular", "symbolic", "numeric"] {
        assert!(json.contains(&format!("\"world\":\"{label}\"")), "{json}");
    }
    assert!(json.contains("\"missing\":[\"modulus\"]"), "{json}");
    assert!(json.contains("\"missing\":[\"tolerance\"]"), "{json}");
    assert!(!json.contains("\"canonical\":\"1.414"), "{json}");
}

#[test]
fn wasm_candidate_application_pins_source_and_result_world() {
    let json = emath_wasm::run_op(
        "solve_candidates",
        "{\"source\":\"solve x^2 = 2\\n\",\"apply\":\"real-pm\"}",
    );
    assert!(json.contains("\"apply\": \"real-pm\""), "{json}");
    assert!(json.contains("solve x^2 = 2 over Real"), "{json}");
    assert!(json.contains("\"meaning_delta\":"), "{json}");
    assert!(json.contains("\"world\":\"real-pm\""), "{json}");
    assert!(!json.contains("\"world\":\"numeric\""), "{json}");
}

#[test]
fn unresolved_candidate_parameters_are_written_as_holes() {
    let (modular, _) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Modular).expect("modular");
    let (numeric, _) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Numeric).expect("numeric");
    assert!(modular.starts_with("modulus = ?\n"), "{modular}");
    assert!(numeric.starts_with("tolerance = ?\n"), "{numeric}");
    assert!(!modular.contains("mod 2"), "{modular}");
}
