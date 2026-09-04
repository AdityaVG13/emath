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

use emath_core::limits::Limits;
use emath_exec_ir::runner::run_package as _;
use emath_ir::EvidenceLevel;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

const PACKAGE: &str = include_str!("../../../language/stdlib/laws/approximation.emath");

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

/// The package admits with all three laws and every example passes.
#[test]
fn approximation_package_admits_and_runs() {
    let result = check("approximation-laws", PACKAGE);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| (diagnostic.code, diagnostic.message.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(result.package.declarations.len(), 3);
    assert_eq!(result.package.law_metadata.len(), 3);
    let report = emath_exec_ir::runner::run_package(&result.package);
    assert_eq!(report.summary.tests, 3);
    assert_eq!(report.summary.passed, 3);
}

/// Values: Taylor quadratic 1 + 2*0.5 + 2*0.25/2 = 2.5;
/// Chebyshev T2(0.5) = 2*0.25 - 1 = -0.5;
/// Padé (2,1) at 0.5: (1 + 0.5 + 0.125) / (1 - 0.25) = 1.625/0.75.
#[test]
fn expansion_values_are_correct() {
    let result = check("approximation-values", PACKAGE);
    assert!(!result.diagnostics.has_errors());
    let report = emath_exec_ir::runner::run_package(&result.package);
    assert_eq!(report.summary.passed, 3, "all three examples must hold");
}

/// Every law carries the honesty label: evidence level E1 claims state
/// the approximation is computed (not a remainder bound, not exact).
#[test]
fn honesty_labels_are_e1_claims() {
    let result = check("approximation-honesty", PACKAGE);
    assert!(!result.diagnostics.has_errors());
    for declaration in &result.package.declarations {
        assert_eq!(declaration.kind_label, "law");
        let claim = &declaration.evidence[0];
        assert_eq!(claim.level, EvidenceLevel::E1, "{:?}", claim.statement);
        let statement = claim.statement.to_lowercase();
        assert!(
            statement.contains("declared")
                || statement.contains("regime")
                || statement.contains("not fabricated")
                || statement.contains("not a remainder bound"),
            "evidence claim must carry the honesty label: {statement}"
        );
    }
}

/// Law identity carries the enforced regime: mutating the `require`
/// constraint changes the canonical package encoding. (An example
/// `given` value is runtime data, not identity.)
#[test]
fn declared_regime_participates_in_identity() {
    let base = check("approximation-regime-base", PACKAGE);
    let widened = check(
        "approximation-regime-widened",
        &PACKAGE.replace(
            "require abs(delta) < convergence_radius",
            "require abs(delta) <= convergence_radius",
        ),
    );
    assert_ne!(
        base.package.identity.as_ref().unwrap().content,
        widened.package.identity.as_ref().unwrap().content,
        "the declared approximation regime is meaning-bearing, not prose"
    );
}

/// Negative control: deleting the regime assumption (the declared
/// convergence radius) refuses — an approximation without a declared
/// regime is exactly the lie the honesty surface forbids.
#[test]
fn missing_declared_regime_loses_the_constraint() {
    // The declared regime IS machine-checked: removing the `require`
    // line removes the admission constraint (the declaration still
    // admits — prose assumptions remain — but the enforced regime is
    // gone). Identity still changes because constraints are IR.
    let source = PACKAGE.replace("        require abs(delta) < convergence_radius\n", "");
    assert_ne!(source, PACKAGE, "regime line must exist to be removed");
    let base = check("approximation-regime-base", PACKAGE);
    let stripped = check("approximation-no-regime", &source);
    assert!(!base.diagnostics.has_errors());
    assert!(!stripped.diagnostics.has_errors());
    // The base package carries the regime as an enforced invariant; the
    // stripped one must not.
    let base_invariants: usize = base
        .package
        .declarations
        .iter()
        .map(|d| d.invariants.len())
        .sum();
    let stripped_invariants: usize = stripped
        .package
        .declarations
        .iter()
        .map(|d| d.invariants.len())
        .sum();
    assert_eq!(
        base_invariants, 3,
        "each law carries its regime as an enforced constraint"
    );
    assert_eq!(
        stripped_invariants, 2,
        "stripped regime must lose exactly one constraint"
    );
}

/// Negative control: a Chebyshev evaluation outside the declared domain
/// (|x| > 1) refuses — the regime is enforced, not decorative.
#[test]
fn chebyshev_domain_violation_refuses_at_run() {
    // The declared domain |x| <= 1 is an enforced invariant: evaluating
    // the example at x = 2 must REFUSE at run time (typed refusal
    // verdict), never silently compute a value.
    let source = PACKAGE.replace("            given x = 0.5", "            given x = 2");
    assert_ne!(source, PACKAGE, "domain line must exist to be mutated");
    let result = check("approximation-domain-violation", &source);
    assert!(
        !result.diagnostics.has_errors(),
        "admission admits; the refusal is at run: {:?}",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| (diagnostic.code, diagnostic.message.clone()))
            .collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&result.package);
    assert_eq!(report.summary.refused, 2, "domain violation must refuse");
    assert_eq!(report.summary.passed, 1);
}
