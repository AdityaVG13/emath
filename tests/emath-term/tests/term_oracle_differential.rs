//! Differential Term-oracle pin: generated SG parser vs `emath-term`.

use emath_lab_core::{EngineIdentity, EngineRole};
use emath_test_harness::Probe;

/// The replayed reference term of the parametric worlds crate.
const REPLAY_CANONICAL: &str = "apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))";

#[test]
fn term_oracle_differential() {
    let mut p = Probe::new("generated parser agrees with the emath-term oracle");
    p.case("identity", |p| {
        let subject = EngineIdentity { role: EngineRole::Subject, label: "generated-sg".to_string() };
        let oracle = EngineIdentity { role: EngineRole::Oracle, label: "emath-term".to_string() };
        p.demand("distinct", subject.require_distinct(&oracle, "term oracle differential").is_ok(), "subject and oracle differ");
    });
    let padded = format!("{REPLAY_CANONICAL}  \n\t ");
    for input in [REPLAY_CANONICAL, padded.as_str()] {
        p.case(input, |p| {
            let oracle = emath_term::Term::parse_canonical(input).expect("oracle parses");
            let generated = semantic_genesis_worlds::Term::parse_canonical(input).expect("generated parses");
            p.eq("agree", generated.canonical(), oracle.canonical());
            p.ne("nonempty", generated.canonical(), String::new());
        });
    }
    p.eq("canonical-stable", semantic_genesis_worlds::Term::parse_canonical(REPLAY_CANONICAL).unwrap().canonical(), REPLAY_CANONICAL.to_string());
    for bad in ["apply(", "const(a) trailing", "apply(const(ζ)", "var(\\n)"] {
        p.case(bad, |p| {
            p.demand("oracle-refuses", emath_term::Term::parse_canonical(bad).is_err(), "oracle refuses");
            p.demand("generated-refuses", semantic_genesis_worlds::Term::parse_canonical(bad).is_err(), "generated refuses");
        });
    }
    p.finish();
}
