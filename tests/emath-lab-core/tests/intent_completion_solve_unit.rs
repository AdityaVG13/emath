//! Intent completion stays labeled across the syntax and browser APIs.

use emath_syntax::{SolveIntent, SolveWorld, apply_solve_candidate, expand_scratch};
use emath_test_harness::Probe;

#[test]
fn intent_completion_solve() {
    let mut p = Probe::new("solve completions are labeled world bundles, never bare floats");
    p.case("scratch-labeled", |p| {
        let expansion = expand_scratch("solve x^2 = 2\n");
        p.eq("intent", expansion.solve, SolveIntent::Unlabeled);
        p.eq("menu", expansion.solve.menu(), &SolveWorld::ALL);
        p.demand("labeled", expansion.solve.menu().iter().all(|w| !w.as_str().is_empty() && !w.result_type().is_empty() && !w.method().is_empty()), "every world carries label, type, method");
        p.demand("no-float", !expansion.expanded.contains("1.414"), "no premature numeric answer");
    });
    p.case("wasm-bundles", |p| {
        let json = emath_wasm::run_op("solve_candidates", "solve x^2 = 2\n");
        for needle in ["\"ok\": true", "\"schema\":\"emath.world-result\"", "\"world\":\"real-pm\"", "\"world\":\"complex\"", "\"world\":\"modular\"", "\"world\":\"symbolic\"", "\"world\":\"numeric\"", "\"missing\":[\"modulus\"]", "\"missing\":[\"tolerance\"]"] {
            p.contains(needle, &json, needle);
        }
        p.demand("no-float", !json.contains("\"canonical\":\"1.414"), "no bare float bundle: {json}");
    });
    p.case("wasm-apply-pins", |p| {
        let json = emath_wasm::run_op("solve_candidates", "{\"source\":\"solve x^2 = 2\\n\",\"apply\":\"real-pm\"}");
        for needle in ["\"apply\": \"real-pm\"", "solve x^2 = 2 over Real", "\"meaning_delta\":", "\"world\":\"real-pm\""] {
            p.contains(needle, &json, needle);
        }
        p.demand("not-numeric", !json.contains("\"world\":\"numeric\""), "apply pins real-pm: {json}");
    });
    p.case("holes", |p| {
        let (modular, _) = apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Modular).expect("modular");
        let (numeric, _) = apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Numeric).expect("numeric");
        p.demand("modulus-hole", modular.starts_with("modulus = ?\n"), "modular writes modulus hole: {modular}");
        p.demand("tolerance-hole", numeric.starts_with("tolerance = ?\n"), "numeric writes tolerance hole: {numeric}");
        p.demand("no-mod", !modular.contains("mod 2"), "no invented modulus: {modular}");
    });
    p.finish();
}
