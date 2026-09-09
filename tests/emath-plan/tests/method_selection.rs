//! Methods are optional on ordinary declarations.
//!
//! A plain `emath function` admits with no `methods:`; an unused method
//! pack must not change the function's MeaningID; a method card cannot
//! raise evidence authority (E1/not-run, proposal-only).

use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_test_harness::{Probe, Source, boot};

const PLAIN_FUNCTION: &str = "\
emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";

const WITH_UNUSED_PACK: &str = "\
emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x

use std.kinds.method

emath method UnusedPack:
    algorithm:
        kind: \"scoring\"
    falsifier:
        condition: \"held-out accuracy drops below the declared floor\"
";

const PROPOSAL: &str = "\
use std.kinds.method

emath method Careful:
    algorithm:
        kind: \"scoring\"
    falsifier:
        condition: \"held-out accuracy drops below the declared floor\"
    authority:
        claims: \"proposal\"
";

#[test]
fn methods_are_optional_and_cannot_raise_authority() {
    boot();
    let mut p = Probe::new(
        "ordinary functions admit without methods; unused packs leave MeaningID; certified authority is E-KIND-027",
    );
    p.case("plain-admits", |p| {
        let checked = Source::from_str("function-only", PLAIN_FUNCTION).must_admit(p);
        p.eq("decls", checked.package.declarations.len(), 1);
        p.eq("kind", checked.package.declarations[0].kind_label.as_str(), "function");
        p.demand(
            "no-evidence",
            checked.package.declarations[0].evidence.is_empty(),
            "plain function carries no method evidence",
        );
    });
    p.case("unused-pack-stable-meaning", |p| {
        let before = Source::from_str("before-pack", PLAIN_FUNCTION).must_admit(p);
        let after = Source::from_str("after-pack", WITH_UNUSED_PACK).must_admit(p);
        p.eq("decls", after.package.declarations.len(), 2);
        let after_fn = after
            .package
            .declarations
            .iter()
            .find(|declaration| declaration.kind_label == "function");
        match after_fn {
            None => {
                p.fail("fn", "ordinary function must still be admitted");
            }
            Some(after_fn) => {
                let before_fn = &before.package.declarations[0];
                p.eq("name", before_fn.name.clone(), after_fn.name.clone());
                p.eq("kind", before_fn.kind.clone(), after_fn.kind.clone());
                p.eq("inputs", before_fn.inputs.len(), after_fn.inputs.len());
                p.eq("outputs", before_fn.outputs.len(), after_fn.outputs.len());
                p.eq("defs", before_fn.definitions.len(), after_fn.definitions.len());
                p.demand("no-leak", after_fn.evidence.is_empty(), "method evidence must not leak");
            }
        }
        let repeat = Source::from_str("function-only-repeat", PLAIN_FUNCTION).must_admit(p);
        p.eq(
            "meaning-stable",
            before.package.meaning_id(&[]).ok(),
            repeat.package.meaning_id(&[]).ok(),
        );
    });
    p.case("authority-proposal-only", |p| {
        let proposal = Source::from_str("proposal-only", PROPOSAL).must_admit(p);
        match proposal
            .package
            .declarations
            .iter()
            .find(|declaration| declaration.kind_label == "method")
        {
            None => {
                p.fail("method", "proposal-admitting method must be admitted");
            }
            Some(method) => {
                p.eq("evidence-len", method.evidence.len(), 1);
                p.eq("verdict", method.evidence[0].verdict, ClaimVerdict::NotRun);
                p.eq("level", method.evidence[0].level, EvidenceLevel::E1);
                p.eq("checker", method.evidence[0].checker.clone(), None);
            }
        }
        Source::from_workspace("tests/invalid/method_selection.emath")
            .must_refuse(p, &["E-KIND-027"]);
    });
    p.finish();
}
