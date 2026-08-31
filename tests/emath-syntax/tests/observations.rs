//! `observations:` sections — read-only measured evidence
//! (bead emath-r3-observations-9ffu, spec 04 §5.2).
//!
//! Contracts:
//! - an `observations:` section admits; each `obs` row is a measured
//!   datum, not a definition;
//! - writing to an observation is a typed refusal (`E-OBS-WRITE`): the
//!   model/observation line is the point — data is never overwritten by
//!   model output;
//! - a `provenance:` payload with `sha256` is carried for tamper
//!   evidence (`--verify-data` re-hashes; drift refuses `E-OBS-HASH`).
//!
//! Failure-first: written before the `obs` parser arm existed — the
//! parse-level tests verified RED (`E-SYN-101` on the unknown head)
//! before the arm landed.

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

const OBSERVATIONS_FIXTURE: &str = "\
emath policy PkRun:
    inputs:
        dose: Float64

    state:
        conc: Float64

    observations:
        obs plasma_conc: Float64 = 2.5
        obs time_points: Vector<3> = [0.5, 1.0, 2.0]

    definitions:
        conc = dose * 2

    provenance:
        plasma_conc:
            kind \"InstrumentRun\"
            file \"pk_run_041.csv\"
            processing \"LC-MS/MS, area ratio\"
            sha256 \"1111111111111111111111111111111111111111111111111111111111111111\"

    constructors:
        public fn new() -> Result<Self, ConfigError>:
            Self:
                conc = 0.0

    goals:
        evaluate <conc>:
            produce rust.library
";

#[test]
fn observations_section_admits_with_obs_rows() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("observations-admit", OBSERVATIONS_FIXTURE);
    let errors: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        !result.diagnostics.has_errors(),
        "an observations: section with obs rows must admit; got {errors:?}"
    );
}

#[test]
fn writing_to_an_observation_is_refused() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "\
emath function Tamper:
    inputs:
        x: Float64

    observations:
        obs baseline: Float64 = 2.5

    definitions:
        baseline = x * 2
";
    let result = session.check_owned("observations-write", text);
    let rendered: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        rendered
            .iter()
            .any(|m| m.contains("E-OBS-WRITE") && m.contains("observation")),
        "a definitions: binding named like an observation must refuse E-OBS-WRITE; got {rendered:?}"
    );
}

#[test]
fn observation_rows_parse_with_typed_values() {
    // Parse-level: the `obs` head word must produce a structured row
    // (name + optional type annotation + value), not a generic command
    // or a silent drop. A `Vector<3>` value with a scalar annotation is
    // a type mismatch (E-TYPE-012), proving the value was parsed and
    // checked rather than skipped.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "\
emath function Typed:
    observations:
        obs samples: Float64 = [0.5, 1.0, 2.0]
";
    let result = session.check_owned("observations-typed", text);
    let rendered: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        rendered.iter().any(|m| m.contains("E-TYPE-012")),
        "typed obs row must be type-checked (annotation vs value), got {rendered:?}"
    );
}
