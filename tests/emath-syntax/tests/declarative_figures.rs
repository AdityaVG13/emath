//! Declarative figures seed (bead emath-r3-figures-b1xn, 05 §7.4).
//!
//! Contracts:
//! - **Section name + payload grammar slot RESERVED**: `figures:` is
//!   out of the generic E-SEC-101 roster error ("outside the Phase 1
//!   subset (known: ...)") — the roster knows the name so kind schemas
//!   can require/allow it;
//! - **payload rows refuse naming the design forks**: budgeted
//!   sampling tied to budgets/continuation machinery from day one,
//!   sampling receipt on the artifact (visual continuity is labeled
//!   observational, never proved smoothness), renderer as a provider
//!   contract — previously the whole section died with a generic
//!   E-SEC-101 roster error naming no fork;
//! - ordinary declarations admit unchanged.
//!
//! (An EMPTY `figures:` section additionally needs an empty-section
//! parse rule — section heads demand a body, E-SYN-112 — named in the
//! seed, not landed.)
//!
//! Design prose of record: ch.3 "Figures: declarative plot specs
//! (§7.4, seed)".

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const FIGURES_PAYLOAD_ROWS: &str = "\
emath model FigPayloadProbe:
    state:
        q: Float64
        v: Float64

    equations:
        der(q) = v
        der(v) = -2.0 * q

    figures:
        phase_portrait:
            series q over t in 0..10
";

const PLAIN_MODEL: &str = "\
emath model PlainFigProbe:
    state:
        q: Float64
        v: Float64

    equations:
        der(q) = v
        der(v) = -2.0 * q
";

#[test]
fn payload_rows_refuse_naming_design_forks() {
    let errors = check(FIGURES_PAYLOAD_ROWS, "figures-fence");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("budget") && e.contains("nondeterminism")),
        "`figures:` payload rows must refuse naming the budgeted-sampling \
         fork (unbounded sampling = first nondeterminism); got: {errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("sampling receipt")),
        "the figures fence must name the sampling-receipt honesty contract; \
         got: {errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("Renderer")),
        "the figures fence must name the Renderer provider contract; got: \
         {errors:#?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("outside the Phase 1 subset (known:")),
        "the section name is RESERVED: the section head must not die with \
         the generic roster error; got: {errors:#?}"
    );
}

#[test]
fn plain_models_admit_unchanged() {
    let errors = check(PLAIN_MODEL, "figures-plain-guard");
    assert!(
        errors.is_empty(),
        "the figures seed must not affect ordinary models; got: {errors:#?}"
    );
}
