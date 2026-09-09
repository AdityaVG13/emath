//! — admission-side contracts (sema
//! tier): real `.emath` capability declarations with `class: biform`
//! reach the capability layer's closure authority
//! (`crates/emath-ir/src/capability.rs`): one cell, two authorities.
//!
//! A biform capability declares a `spec:` side (laws, types, units — what
//! the cell claims) and an `algorithm:` side (reference semantics / code
//! — how the claim is computed) with INDEPENDENT evidence objects.
//! Admission refusals are typed: E-CELL-009 (missing side), E-CELL-010
//! (authority escalation), E-CELL-011 (one evidence object claimed for
//! both sides). A green algorithm test never stamps the spec proved.
//!
//! Failure-first: every test here was written BEFORE the recognition
//! slice that parses `class:`/`spec:`/`algorithm:` and ran red on the
//! missing admission (E-SYN-101/E-KIND-027/E-KIND-003 refusals).

use emath_ir::capability::CellClass;
use emath_test_harness::{Probe, Source, boot};

const MISSING_SPEC: &str = "\
package std.math
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    algorithm:
        evidence: \"evidence:std.math.softmax:algorithm:v2\"
";

const PROVIDER_SPEC: &str = "\
package std.math
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        evidence: \"evidence:std.math.softmax:spec:v1\"
        authority: provider
    algorithm:
        evidence: \"evidence:std.math.softmax:algorithm:v2\"
";

const PROVIDER_ALGORITHM: &str = "\
package std.math
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        evidence: \"evidence:std.math.softmax:spec:v1\"
    algorithm:
        evidence: \"evidence:std.math.softmax:algorithm:v2\"
        authority: provider
";

const PURE_WITH_SIDES: &str = "\
package std.math
use std.kinds.capability

emath capability Softmax:
    class: pure
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        evidence: \"evidence:std.math.softmax:spec:v1\"
";

const NO_PACKAGE: &str = "\
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        evidence: \"evidence:std.math.softmax:spec:v1\"
    algorithm:
        evidence: \"evidence:std.math.softmax:algorithm:v2\"
";

const DUPLICATE_SPEC: &str = "\
package std.math
use std.kinds.capability

emath capability Softmax:
    class: biform
    version: \"1.0.0\"
    migration: frozen
    inputs:
        x: Vector[Float64]
    outputs:
        probability: Vector[Float64]
    spec:
        evidence: \"evidence:std.math.softmax:spec:v1\"
    spec:
        evidence: \"evidence:std.math.softmax:spec:v2\"
    algorithm:
        evidence: \"evidence:std.math.softmax:algorithm:v2\"
";

#[test]
fn biform_cells_two_authorities() {
    boot();
    let mut p = Probe::new("one biform cell carries two independent authorities");
    let example = Source::from_workspace("tests/fixtures/language/intro/01_softmax_cell.emath");
    let admitted = example.must_admit(&mut p);
    Source::from_workspace("tests/invalid/biform_authority_launder.emath")
        .must_refuse(&mut p, &["E-CELL-011"]);
    p.case("typed-refusals", |p| {
        Source::from_str("missing-spec", MISSING_SPEC).must_refuse(&mut *p, &["E-CELL-009"]);
        Source::from_str("spec-escalation", PROVIDER_SPEC).must_refuse(&mut *p, &["E-CELL-010"]);
        Source::from_str("pure-with-sides", PURE_WITH_SIDES).must_refuse(&mut *p, &["E-SYN-101"]);
        Source::from_str("no-package", NO_PACKAGE).must_refuse(&mut *p, &["E-CELL-005"]);
        Source::from_str("duplicate-spec", DUPLICATE_SPEC).must_refuse(&mut *p, &["E-KIND-003"]);
    });
    p.case("asymmetry", |p| {
        // A provider receipt may attest the ALGORITHM side (delegated
        // execution is the algorithm's business) while the same authority
        // on the SPEC side escalates (E-CELL-010): one-sided, per-side.
        Source::from_str("provider-algorithm", PROVIDER_ALGORITHM).must_admit(&mut *p);
    });
    p.case("interning", |p| {
        match admitted
            .package
            .capabilities
            .iter()
            .find(|cell| cell.name.0 == "std.math.Softmax")
        {
            None => p.fail("cell", "std.math.Softmax not interned in the capability arena"),
            Some(cell) => p.eq("class", cell.class, CellClass::Biform),
        };
        match admitted
            .package
            .declarations
            .iter()
            .find(|decl| decl.kind_label == "capability")
        {
            None => {
                p.fail("declaration", "no capability declaration recorded");
            }
            Some(decl) => {
                let claimed: Vec<&str> =
                    decl.evidence.iter().map(|claim| claim.id.as_str()).collect();
                p.eq("claims-per-side", claimed.len(), 2);
                p.demand(
                    "spec-claim",
                    claimed.contains(&"evidence:std.math.softmax:spec:v1"),
                    format!("spec evidence claim attached: {claimed:?}"),
                );
                p.demand(
                    "algorithm-claim",
                    claimed.contains(&"evidence:std.math.softmax:algorithm:v2"),
                    format!("algorithm evidence claim attached: {claimed:?}"),
                );
            }
        }
    });
    p.case("meaning", |p| {
        // Evidence attachments never enter the meaning preimage: rebinding
        // BOTH sides to fresh independent tokens leaves MeaningID identical.
        let fresh_text = example
            .text()
            .replace(":spec:v1", ":spec:v9")
            .replace(":algorithm:v2", ":algorithm:v8");
        p.demand(
            "rebind-differs",
            fresh_text.as_str() != example.text(),
            "the rebinding variant must differ at the surface",
        );
        let fresh = Source::from_str("softmax-rebound", &fresh_text).must_admit(&mut *p);
        match (
            admitted.package.meaning_id(&[]),
            fresh.package.meaning_id(&[]),
        ) {
            (Ok(base), Ok(rebound)) => p.eq("evidence-rebind-keeps-meaning", base, rebound),
            _ => p.fail(
                "evidence-rebind-keeps-meaning",
                "meaning_id failed on admitted biform packages",
            ),
        };
        // Admission is a pure function of the source: same bytes, same meaning.
        match (
            admitted.package.meaning_id(&[]),
            example.check().package.meaning_id(&[]),
        ) {
            (Ok(first), Ok(second)) => p.eq("meaning-deterministic", first, second),
            _ => p.fail(
                "meaning-deterministic",
                "meaning_id failed on admitted biform packages",
            ),
        };
    });
    p.finish();
}
