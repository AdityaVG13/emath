//! Custom-kind execution story
//! (CAPABILITY "emath kind | partial (schema validation) | no").
//!
//! Contracts:
//! - **Kind definitions register**: `emath kind Gauge:` with a valid
//!   `schema:`/`lower:` body checks clean (schema validated) and
//!   leaves a registered marker, so later applications get an honest
//!   story;
//! - **Function-shaped kind applications execute** through the same typed
//!   definitions, reference VM, and backend path as their declared base kind;
//! - **Undefined application names stay generic**: applying a kind that
//!   was never defined keeps the plain Phase-1-subset refusal;
//! - ordinary kinds admit unchanged.
//!
//! Docs of record: CAPABILITY.md `emath kind` row + ch.8 "Execution
//! story (today)".

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_test_harness::{Probe, Source, boot};

const KIND_DEFINED_AND_APPLIED: &str = "\
emath kind Gauge:
    extends function

    schema:
        require section inputs

    lower:
        model.inputs = section.inputs

emath Gauge HalfGauge:
    inputs:
        x: Float64

    definitions:
        y = x / 2

    tests:
        example <half>:
            given x = 8
            expect y == 4
";

const KIND_UNDEFINED_APPLICATION: &str = "\
emath Never HalfGauge:
    inputs:
        x: Float64

    definitions:
        y = x / 2
";

const KIND_DEFINITION_ALONE: &str = "\
emath kind Gauge:
    extends function

    schema:
        require section inputs

    lower:
        model.inputs = section.inputs
";

const PLAIN_FUNCTION: &str = "\
emath function PlainFn:
    inputs:
        x: Float64

    definitions:
        y = x / 2
";

#[test]
fn custom_kind_execution_story() {
    boot();
    let mut p = Probe::new("function-shaped custom kinds execute; undefined kinds keep the generic refusal");
    p.case("define-and-apply", |p| {
        // The example must Pass (8 / 2 = 4), not merely admit.
        Source::from_str("half-gauge", KIND_DEFINED_AND_APPLIED).eval_tests(&mut *p);
        let checked = Source::from_str("half-gauge", KIND_DEFINED_AND_APPLIED).check();
        match checked
            .package
            .declarations
            .iter()
            .find(|decl| decl.name.leaf() == "HalfGauge")
        {
            None => {
                p.fail("half-gauge/decl", "HalfGauge declaration missing after admit");
            }
            Some(_) => {}
        }
        let report = run_package(&checked.package);
        match report.declarations.iter().find(|run| run.name == "HalfGauge") {
            None => {
                p.fail("half-gauge/run", "custom-kind application did not run");
            }
            Some(run) => match run.tests.first() {
                None => {
                    p.fail("half-gauge/test", "HalfGauge carries no example test");
                }
                Some(test) => {
                    p.demand(
                        "half-gauge/passed",
                        test.verdict == TestVerdict::Passed,
                        format!("expected Passed, got {}", test.verdict),
                    );
                    p.eq(
                        "half-gauge/y",
                        test.definitions.get("y"),
                        Some(&Value::F64(4.0)),
                    );
                }
            },
        }
    });
    p.case("undefined-keeps-generic-refusal", |p| {
        Source::from_str("kind-undefined", KIND_UNDEFINED_APPLICATION)
            .must_refuse(&mut *p, &["E-KIND-100"]);
        let checked = Source::from_str("kind-undefined", KIND_UNDEFINED_APPLICATION).check();
        let joined = checked
            .diagnostics
            .errors()
            .map(|diag| diag.message.clone())
            .collect::<Vec<_>>()
            .join("\n");
        p.contains(
            "kind-undefined/phase1-subset",
            &joined,
            "outside the Phase 1 subset",
        );
        p.demand(
            "kind-undefined/no-run-path-story",
            joined.contains("NO RUN PATH") == false,
            "the no-run-path story must not fire for undefined kinds",
        );
    });
    p.case("kind-definition-clean", |p| {
        Source::from_str("kind-def", KIND_DEFINITION_ALONE).must_admit(&mut *p);
    });
    p.case("plain-function-guard", |p| {
        Source::from_str("kind-plain-guard", PLAIN_FUNCTION).must_admit(&mut *p);
    });
    p.finish();
}
