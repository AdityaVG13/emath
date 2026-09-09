//! Declarative figures seed (05 §7.4).
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

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
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

use emath_test_harness::{Probe, Source, boot};

#[test]
fn declarative_figures() {
    boot();
    let mut probe = Probe::new("Declarative figures seed (05 §7.4). Contracts: - **Section name + payload grammar slot RESERVED**: `figures:` is out of the generic E-SEC-101 roster");
    probe.case("payload_rows_refuse_naming_design_forks", |p| {
    let f0 = p.failures().len();

    let errors = check(FIGURES_PAYLOAD_ROWS, "figures-fence");
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("budget") && e.contains("nondeterminism")), format!(
        "`figures:` payload rows must refuse naming the budgeted-sampling \
         fork (unbounded sampling = first nondeterminism); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",errors.iter().any(|e| e.contains("sampling receipt")), format!(
        "the figures fence must name the sampling-receipt honesty contract; \
         got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",errors.iter().any(|e| e.contains("Renderer")), format!(
        "the figures fence must name the Renderer provider contract; got: \
         {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",!errors
            .iter()
            .any(|e| e.contains("outside the Phase 1 subset (known:")), format!(
        "the section name is RESERVED: the section head must not die with \
         the generic roster error; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("plain_models_admit_unchanged", |p| {
    let f0 = p.failures().len();

    let errors = check(PLAIN_MODEL, "figures-plain-guard");
    p.demand("1",errors.is_empty(), format!(
        "the figures seed must not affect ordinary models; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
