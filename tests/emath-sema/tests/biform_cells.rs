//! Bead `emath-biform-cells-jswu6` — admission-side contracts (sema
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

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// The positive language example is the on-disk fixture: a biform
/// softmax cell with independent spec and algorithm evidence.
const POSITIVE_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../language/examples/intro/01_softmax_cell.emath"
));

/// The negative seed is the on-disk fixture: one evidence object bound
/// to both sides (authority laundering).
const LAUNDER_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/invalid/biform_authority_launder.emath"
));

/// Run the front-end over a source and return the diagnostics as
/// (severity, code, message).
fn check(source: &str) -> Vec<(String, String, String)> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("biform", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
                diagnostic.message.clone(),
            )
        })
        .collect()
}

fn error_codes(out: &[(String, String, String)]) -> Vec<String> {
    out.iter()
        .filter(|(severity, _, _)| severity == "Error")
        .map(|(_, code, _)| code.clone())
        .collect()
}

/// The biform softmax example admits through the language layer: the
/// spec/algorithm sections parse into independent evidence objects and
/// the capability closure validates both sides.
#[test]
fn biform_softmax_example_admits() {
    let out = check(POSITIVE_FIXTURE);
    let codes = error_codes(&out);
    assert!(
        codes.is_empty(),
        "example must admit; got {codes:?}"
    );
}

/// Negative seed: the launder binds the SAME evidence object to the spec
/// and algorithm sides. Admission refuses typed E-CELL-011 (side-evidence
/// collision) — a green algorithm test never stamps the spec proved.
#[test]
fn launder_fixture_refuses_e_cell_011() {
    let out = check(LAUNDER_FIXTURE);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-CELL-011"),
        "launder must refuse E-CELL-011, got {codes:?}"
    );
}

/// A biform cell with only the algorithm side is a typed missing-side
/// refusal: the spec side never closes "proved by the algorithm".
#[test]
fn missing_spec_side_refuses_e_cell_009() {
    let source = "\
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
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-CELL-009"),
        "missing spec side must refuse E-CELL-009, got {codes:?}"
    );
}

/// Authority non-escalation: a provider receipt may attest the algorithm
/// side but can never raise spec authority (E-CELL-010).
#[test]
fn provider_authority_on_spec_refuses_e_cell_010() {
    let source = "\
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
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-CELL-010"),
        "provider authority on the spec side must refuse E-CELL-010, got {codes:?}"
    );
}

/// A pure cell does not carry side sections: `spec:` on a pure
/// capability is refused (E-SYN-101), never silently dropped.
#[test]
fn pure_capability_side_sections_refuse() {
    let source = "\
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
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-SYN-101"),
        "side sections on a pure cell must refuse E-SYN-101, got {codes:?}"
    );
}

/// A biform cell without a `package` has no namespaced cell name: the
/// bounded admission refuses E-CELL-005 (malformed name) — identity
/// needs a stable namespace path, never an invented one.
#[test]
fn biform_without_package_refuses_e_cell_005() {
    let source = "\
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
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-CELL-005"),
        "a package-less capability name must refuse E-CELL-005, got {codes:?}"
    );
}

/// A repeated side section is refused, never silently replaced: a second
/// `spec:` whose evidence differs from the first would otherwise have its
/// evidence object dropped BEFORE the closure check could see it — a
/// silent hole in the one-cell-two-authorities contract. Same rule for a
/// repeated `inputs` (the arity reads only the first section).
#[test]
fn duplicate_side_section_refuses_e_kind_003() {
    let source = "\
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
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-KIND-003"),
        "a duplicate spec section must refuse E-KIND-003, got {codes:?}"
    );
}

fn checked(source: &str) -> emath_sema::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("biform", source)
}

/// IR acceptance is nonzero: a clean biform declaration interns the cell
/// in the package's capability arena (class `biform`, canonical
/// `std.math.Softmax`) and records one E1/not-run evidence claim per
/// side on the declaration — the sides are visible to later slices, not
/// swallowed by admission.
#[test]
fn biform_cell_interning_and_side_claims() {
    let result = checked(POSITIVE_FIXTURE);
    assert!(
        !result.diagnostics.has_errors(),
        "example must admit: {:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    let interned = result
        .package
        .capabilities
        .iter()
        .find(|cell| cell.name.0 == "std.math.Softmax")
        .expect("biform cell interned in the capability arena");
    assert_eq!(
        interned.class,
        emath_ir::capability::CellClass::Biform,
        "interned class is biform"
    );
    let declaration = result
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.kind_label == "capability")
        .expect("capability declaration recorded");
    let claimed: Vec<&str> = declaration
        .evidence
        .iter()
        .map(|claim| claim.id.as_str())
        .collect();
    assert_eq!(
        claimed.len(),
        2,
        "one evidence claim per side, got {claimed:?}"
    );
    assert!(
        claimed.contains(&"evidence:std.math.softmax:spec:v1"),
        "spec evidence claim attached: {claimed:?}"
    );
    assert!(
        claimed.contains(&"evidence:std.math.softmax:algorithm:v2"),
        "algorithm evidence claim attached: {claimed:?}"
    );
}

fn meaning_id(source: &str) -> emath_core::MeaningId {
    checked(source)
        .package
        .meaning_id(&[])
        .expect("admitted biform package has a meaning id")
}

/// MR evidence non-identity (score 8): evidence attachments never enter
/// the meaning preimage (`emath-ir/src/meaning.rs` law: tests, evidence
/// attachments, and host bindings do not). Rebinding BOTH sides to fresh
/// independent evidence tokens must leave the MeaningID byte-identical —
/// proofs and tests attach without changing the cell's meaning.
#[test]
fn mr_evidence_rebinding_never_moves_meaning() {
    let fresh = POSITIVE_FIXTURE
        .replace(":spec:v1", ":spec:v9")
        .replace(":algorithm:v2", ":algorithm:v8");
    assert_ne!(
        POSITIVE_FIXTURE, fresh,
        "the rebinding variant actually differs at the surface"
    );
    assert_eq!(
        meaning_id(POSITIVE_FIXTURE),
        meaning_id(&fresh),
        "evidence rebinding must not move the MeaningID"
    );
}

/// MR determinism (score 4): the same biform declaration checks to the
/// same meaning twice — admission is a pure function of the source.
#[test]
fn mr_biform_meaning_deterministic() {
    assert_eq!(
        meaning_id(POSITIVE_FIXTURE),
        meaning_id(POSITIVE_FIXTURE)
    );
}

/// MR non-escalation asymmetry (score 8): a provider receipt may attest
/// the ALGORITHM side (delegated execution is the algorithm's business)
/// while the same authority on the SPEC side escalates (E-CELL-010).
/// The pair pins one-sidedness: escalation is per-side, not per-cell.
#[test]
fn mr_provider_algorithm_admits_but_spec_escalates() {
    let provider_algorithm = "\
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
    let codes = error_codes(&check(provider_algorithm));
    assert!(
        codes.is_empty(),
        "provider authority on the algorithm side admits, got {codes:?}"
    );
    // The paired spec-side case already refuses E-CELL-010 (see
    // provider_authority_on_spec_refuses_e_cell_010); restate the pair
    // here so the asymmetry is one law, not two isolated facts.
    let provider_spec = "\
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
    let codes = error_codes(&check(provider_spec));
    assert!(
        codes.iter().any(|code| code == "E-CELL-010"),
        "escalation is refused on the spec side, got {codes:?}"
    );
}
