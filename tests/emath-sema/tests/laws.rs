//! Executable `emath law` declarations and their metadata boundary.
use emath_exec_ir::runner::run_package;
use emath_ir::{EvidenceLevel, GoalId, MeaningError};
use emath_test_harness::{boot, Probe, Source};

const NEWTON: &str = "language/examples/physics/newton-second.emath";

#[test]
fn laws() {
    boot();
    let mut p = Probe::new("executable emath law declarations admit, run, and carry their metadata boundary");
    p.case("newton-second", |p| {
        let newton = Source::from_workspace(NEWTON);
        let result = newton.must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let declaration = &result.package.declarations[0];
        p.eq("kind", declaration.kind.0.clone(), "law".to_string());
        p.eq("kind-label", declaration.kind_label.clone(), "law".to_string());
        match declaration.evidence.first() {
            Some(evidence) => {
                p.eq("evidence-level", evidence.level, EvidenceLevel::E2);
                p.eq(
                    "evidence-assumptions",
                    evidence.assumptions.clone(),
                    vec!["The mass is constant in the chosen inertial frame.".to_string()],
                );
            }
            None => {
                p.fail("evidence", "newton-second must carry evidence");
            }
        }
        let metadata = match result.package.law_metadata.get(&declaration.id) {
            Some(metadata) => metadata,
            None => {
                p.fail("metadata", "newton-second must carry law metadata");
                return;
            }
        };
        p.eq("domain", metadata.domain.clone(), "classical mechanics".to_string());
        p.eq(
            "assumptions",
            metadata.assumptions.clone(),
            vec!["The mass is constant in the chosen inertial frame.".to_string()],
        );
        p.eq("provenance", metadata.provenance.len(), 1);
        p.eq("citations", metadata.citations.len(), 1);
        let report = run_package(&result.package);
        p.eq("tests", report.summary.tests, 1);
        p.eq("passed", report.summary.passed, 1);
        if report.declarations.is_empty() {
            p.fail("report", "run report must carry the law declaration");
        } else {
            p.eq(
                "report-metadata",
                report.declarations[0].law_metadata.as_ref(),
                Some(metadata),
            );
        }
        let revised = Source::from_str(
            "newton-second-revised-domain",
            newton.text().replace("classical mechanics", "relativistic mechanics"),
        )
        .must_admit(&mut *p);
        if revised.diagnostics.has_errors() {
            return;
        }
        match (
            result.package.identity.as_ref(),
            revised.package.identity.as_ref(),
        ) {
            (Some(before), Some(after)) => {
                p.ne(
                    "identity-tracks-metadata",
                    format!("{:?}", before.content),
                    format!("{:?}", after.content),
                );
            }
            _ => {
                p.fail("identity", "law packages must carry identity");
            }
        }
        match (
            result.package.meaning_id(&[]),
            revised.package.meaning_id(&[]),
        ) {
            (Ok(before), Ok(after)) => {
                p.eq(
                    "meaning-stable",
                    format!("{before:?}"),
                    format!("{after:?}"),
                );
            }
            (before, after) => {
                p.fail(
                    "meaning-stable",
                    format!("meaning_id must succeed, got {before:?} / {after:?}"),
                );
            }
        }
    });
    for (name, path, expected) in [
        ("physics-classical", "language/stdlib/laws/physics-classical.emath", 5u32),
        ("physics-relativity", "language/stdlib/laws/physics-relativity.emath", 1u32),
        ("computer-science", "language/stdlib/laws/computer-science.emath", 3u32),
        ("probability-statistics", "language/stdlib/laws/probability-statistics.emath", 3u32),
        ("analysis", "language/stdlib/laws/analysis.emath", 3u32),
        ("algebra-number-theory", "language/stdlib/laws/algebra-number-theory.emath", 3u32),
        ("optimization-control", "language/stdlib/laws/optimization-control.emath", 3u32),
    ] {
        p.case(name, |p| {
            let result = Source::from_workspace(path).must_admit(&mut *p);
            if result.diagnostics.has_errors() {
                return;
            }
            p.eq("declarations", result.package.declarations.len(), expected as usize);
            p.eq("metadata", result.package.law_metadata.len(), expected as usize);
            let report = run_package(&result.package);
            p.eq("tests", report.summary.tests, expected);
            p.eq("passed", report.summary.passed, expected);
            for declaration in &report.declarations {
                for test in &declaration.tests {
                    p.demand(
                        format!("{}/{}", declaration.name, test.name),
                        test.verdict.expect_passed(),
                        format!("law example must pass, got {}", test.verdict),
                    );
                }
            }
        });
    }
    p.case("classical-import", |p| {
        let result =
            Source::from_str("physics-classical-import", "use physics::classical::{NewtonSecond, Hooke}")
                .must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        p.eq("declarations", result.package.declarations.len(), 2);
        p.eq("metadata", result.package.law_metadata.len(), 2);
        let report = run_package(&result.package);
        p.eq("tests", report.summary.tests, 2);
        p.eq("passed", report.summary.passed, 2);
    });
    p.case("law-requires-assumptions", |p| {
        let text = include_str!("../../../language/examples/physics/newton-second.emath").replace(
            "    assumptions:\n        assume: \"The mass is constant in the chosen inertial frame.\"\n        require mass >= 0 kg\n\n",
            "",
        );
        Source::from_str("law-without-assumptions", text).must_refuse(&mut *p, &["E-LAW-002"]);
    });
    p.case("law-enforces-units", |p| {
        let text = include_str!("../../../language/examples/physics/newton-second.emath")
            .replace("force = mass * acceleration", "force = mass + acceleration");
        Source::from_str("law-unit-mismatch", text).must_refuse(&mut *p, &["E-UNIT-101"]);
    });
    p.case("law-unknown-evidence", |p| {
        let text = include_str!("../../../language/examples/physics/newton-second.emath")
            .replace("level E2", "level E9");
        Source::from_str("law-unknown-evidence", text).must_refuse(&mut *p, &["E-EVID-115"]);
    });
    p.case("unresolved-import", |p| {
        let text = format!(
            "use physics::NewtonSecond\n\n{}",
            include_str!("../../../language/examples/physics/newton-second.emath")
        );
        Source::from_str("law-import", text).must_refuse(&mut *p, &["E-PKG-052"]);
    });
    p.case("multiple-packages", |p| {
        Source::from_str(
            "multiple-law-packages",
            "use physics::classical::{NewtonSecond}\nuse analysis::laws::{TaylorQuadratic}\n",
        )
        .must_refuse(&mut *p, &["E-PKG-053"]);
    });
    p.case("malformed-goal", |p| {
        let mut result = Source::from_workspace(NEWTON).check();
        if result.package.declarations.is_empty() {
            p.fail("decl", "fixture must admit before goal surgery");
            return;
        }
        result.package.declarations[0].goals.push(GoalId(u32::MAX));
        p.demand(
            "missing-goal",
            matches!(
                result.package.meaning_id(&[]),
                Err(MeaningError::MissingGoal(GoalId(u32::MAX)))
            ),
            "malformed law goal must refuse meaning identity with MissingGoal",
        );
    });
    p.case("violated-assumption", |p| {
        let base = Source::from_workspace("language/stdlib/laws/probability-statistics.emath");
        let result = Source::from_str(
            "probability-invalid-normalizer",
            base.text().replace("given normalizer = 0.4", "given normalizer = 0"),
        )
        .must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        p.eq("refused", report.summary.refused, 1);
        p.eq("passed", report.summary.passed, 2);
    });
    for (name, source) in [
        ("newton-second", "use physics::classical::NewtonSecond"),
        ("mass-energy", "use physics::relativity::MassEnergyEquivalence"),
        ("amdahl", "use cs::laws::AmdahlSpeedup"),
        ("bayes", "use probability::laws::BayesPosterior"),
        ("taylor", "use analysis::laws::TaylorQuadratic"),
        ("modular-inverse", "use number_theory::laws::ModularInverse"),
        ("bellman", "use optimization_control::laws::BellmanTwoActionBackup"),
    ] {
        p.case(name, |p| {
            let result = Source::from_str("law-package-import", source).must_admit(&mut *p);
            if result.diagnostics.has_errors() {
                return;
            }
            p.eq("declarations", result.package.declarations.len(), 1);
            let report = run_package(&result.package);
            p.eq("passed", report.summary.passed, 1);
            for declaration in &report.declarations {
                for test in &declaration.tests {
                    p.demand(
                        format!("{}/{}", declaration.name, test.name),
                        test.verdict.expect_passed(),
                        format!("law example must pass, got {}", test.verdict),
                    );
                }
            }
        });
    }
    p.finish();
}
