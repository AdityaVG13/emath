//! Approximation and expansions stdlib (B31): the `approximation.laws`
//! package admits,
//! runs its examples, and carries the honesty label.
//!
//! Intent: Taylor/Chebyshev/Padé as stdlib content whose approximation
//! regime is DECLARED (radius, domain, pole avoidance) — never implied —
//! and whose evidence claims label the output approximate, not exact.
//! Used where exactness is required, the ≈-style authority degradation
//! applies (negative control here via the refusal of a regime-violating
//! evaluation claim and the E1 evidence level pin).

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_ir::EvidenceLevel;
use emath_test_harness::{Probe, Source, boot};

// Independent oracles: Taylor 1+1+0.25=2.25; Chebyshev 2·0.25-1=-0.5; Pade 1.625/0.75.
const EXPECTED: &[(&str, f64)] = &[
    ("TaylorQuadraticRegime", 2.25),
    ("ChebyshevThreeTerm", -0.5),
    ("PadeTwoOne", 2.1666666666666665),
];

#[test]
fn approximation_stdlib_honesty() {
    boot();
    let mut p = Probe::new("stdlib expansions carry declared regimes and honest E1 labels");
    let package = Source::from_workspace("language/stdlib/laws/approximation.emath");
    let admitted = package.must_admit(&mut p);
    p.case("admits-and-runs", |p| {
        p.eq("declarations", admitted.package.declarations.len(), 3);
        p.eq("law-metadata", admitted.package.law_metadata.len(), 3);
        let report = run_package(&admitted.package);
        p.eq("tests", report.summary.tests, 3);
        p.eq("passed", report.summary.passed, 3);
    });
    p.case("expansion-values", |p| {
        let report = run_package(&admitted.package);
        for (name, expected) in EXPECTED {
            match report.declarations.iter().find(|run| &run.name == name) {
                None => {
                    p.fail("expansion-values/missing", format!("no run for {name}"));
                }
                Some(run) => match run.tests.first() {
                    None => {
                        p.fail(
                            format!("{name}/example"),
                            "law carries no example test to evaluate",
                        );
                    }
                    Some(test) => {
                        p.demand(
                            format!("{name}/passed"),
                            test.verdict == TestVerdict::Passed,
                            format!("expected Passed, got {}", test.verdict),
                        );
                        match test.definitions.get("approximation") {
                            Some(Value::F64(got)) => {
                                p.close(format!("{name}/value"), *got, *expected, 1e-12);
                            }
                            other => {
                                p.fail(
                                    format!("{name}/value"),
                                    format!("expected F64 approximation, got {other:?}"),
                                );
                            }
                        }
                    }
                },
            }
        }
    });
    p.case("honesty-labels", |p| {
        for decl in &admitted.package.declarations {
            let leaf = decl.name.leaf().to_string();
            p.eq(format!("{leaf}/kind"), decl.kind_label.clone(), "law".to_string());
            match decl.evidence.first() {
                None => {
                    p.fail(format!("{leaf}/evidence"), "law carries no evidence claim");
                }
                Some(claim) => {
                    p.eq(format!("{leaf}/level"), claim.level, EvidenceLevel::E1);
                    let statement = claim.statement.to_lowercase();
                    p.demand(
                        format!("{leaf}/honesty-words"),
                        statement.contains("declared")
                            || statement.contains("regime")
                            || statement.contains("not fabricated")
                            || statement.contains("not a remainder bound"),
                        format!("evidence claim must carry the honesty label: {statement}"),
                    );
                }
            }
        }
    });
    p.case("regime-identity", |p| {
        let widened_text = package.text().replace(
            "require abs(delta) < convergence_radius",
            "require abs(delta) <= convergence_radius",
        );
        p.demand(
            "regime-line-exists",
            widened_text != package.text(),
            "the regime line must exist to be widened",
        );
        let widened =
            Source::from_str("approximation-widened", widened_text).must_admit(&mut *p);
        let base_content = admitted
            .package
            .identity
            .as_ref()
            .map(|id| id.content.0.clone())
            .unwrap_or_default();
        let wide_content = widened
            .package
            .identity
            .as_ref()
            .map(|id| id.content.0.clone())
            .unwrap_or_default();
        p.demand("base-sealed", !base_content.is_empty(), "base package must be sealed");
        p.demand("widened-sealed", !wide_content.is_empty(), "widened package must be sealed");
        p.ne("regime-moves-identity", wide_content, base_content);
    });
    p.case("regime-required", |p| {
        let stripped_text = package
            .text()
            .replace("        require abs(delta) < convergence_radius\n", "");
        p.demand(
            "regime-line-present",
            stripped_text != package.text(),
            "the regime line must exist to be removed",
        );
        let stripped =
            Source::from_str("approximation-no-regime", stripped_text).must_admit(&mut *p);
        let base_invariants: usize = admitted
            .package
            .declarations
            .iter()
            .map(|decl| decl.invariants.len())
            .sum();
        let stripped_invariants: usize = stripped
            .package
            .declarations
            .iter()
            .map(|decl| decl.invariants.len())
            .sum();
        p.eq("base-invariants", base_invariants, 3);
        p.eq("stripped-invariants", stripped_invariants, 2);
    });
    p.case("domain-violation", |p| {
        let violated_text = package
            .text()
            .replace("            given x = 0.5", "            given x = 2");
        p.demand(
            "domain-line-exists",
            violated_text != package.text(),
            "the domain line must exist to be mutated",
        );
        let violated =
            Source::from_str("approximation-domain", violated_text).must_admit(&mut *p);
        let report = run_package(&violated.package);
        p.eq("refused", report.summary.refused, 2);
        p.eq("passed", report.summary.passed, 1);
    });
    p.finish();
}
