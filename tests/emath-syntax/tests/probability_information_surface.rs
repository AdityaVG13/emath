//! Probability/information surface pins (criterion 2): the `~`
//! glyph's three coexisting readings must not interfere — distribution
//! tag on measurement literals (B10+04 §4.3, C7/X5), asymptotics
//! (`~~`), and logic negation (`!`, which owns negation so `~` never
//! has to). B10's random-variable input row (`x: Random<Real> ~
//! Normal(0, 1)`) is the giry-world follow-up; these pins guard the
//! readings that exist today against that landing.

use emath_core::limits::Limits;
use emath_core::tree::{Expr, ExprKind, StmtKind};
use emath_sema::CompilerSession;

fn parse_defns(source: &str) -> Result<Vec<Expr>, String> {
    let (tree, diags) = emath_syntax::parse_str(source);
    if diags.has_errors() {
        return Err(diags
            .errors()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("; "));
    }
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        return Err("no declaration".into());
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .ok_or("no definitions")?;
    let mut values = Vec::new();
    for stmt in &defs.suite.statements {
        if let StmtKind::Assign { value, .. } = &stmt.kind {
            values.push(value.clone());
        }
    }
    if values.is_empty() {
        return Err("no assignment".into());
    }
    Ok(values)
}

use emath_test_harness::{Probe, boot};

#[test]
fn probability_information_surface() {
    boot();
    let mut probe = Probe::new("Probability/information surface pins (criterion 2): the `~` glyph's three coexisting readings must not interfere — distribution tag on measurement");
    probe.case("distribution_tag_still_parses_on_measurement_literals", |p| {

    // The existing `~` reading (C7/X5 owner) is untouched.
    let values =
        parse_defns("emath function f:\n    definitions:\n        k = 0.30 ± 0.12 ~ lognormal\n")
            .unwrap();
    let ExprKind::Measured { distribution, .. } = &values[0].kind else {
        panic!("measured literal, got {:?}", values[0].kind);
    };
    p.eq("1", distribution.as_deref(), Some("lognormal"));

    });
    probe.case("double_tilde_stays_asymptotics_not_tag", |p| {
    let f0 = p.failures().len();

    // `a ~~ b` lexes as ONE TildeTilde token and parses to the Asymp
    // binary — never tag `~` + something (tag parse of the second
    // `~` is impossible by construction).
    let values = parse_defns("emath function f:\n    definitions:\n        r = a ~~ b\n").unwrap();
    p.demand("1",matches!(
            &values[0].kind,
            ExprKind::Binary {
                op: emath_core::tree::BinaryOp::Asymp,
                ..
            }
        ), format!(
        "`~~` must parse as the asymptotic relation, got {:?}",
        values[0].kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("negation_is_bang_not_tilde", |p| {
    let f0 = p.failures().len();

    // C7/X5 glyph division: logic negation is `not` today (`!p` prefix
    // is not an admitted spelling and refuses by name — it is the
    // RESERVED symbolic-negation glyph, never a tilde reading), and
    // `~` is never a negation spelling, so the two can never collide.
    let error =
        parse_defns("emath function f:\n    definitions:\n        n = !true\n").unwrap_err();
    p.demand("1",error.contains("E-SYN"), format!(
        "`!` prefix refuses by name (reserved glyph), got {error}"
    ));
    if p.failures().len() != f0 { return; }
    let values =
        parse_defns("emath function f:\n    definitions:\n        n = not true\n").unwrap();
    let ExprKind::Unary { .. } = &values[0].kind else {
        panic!("`not` negation, got {:?}", values[0].kind);
    };

    });
    probe.case("tagged_measured_still_admits_with_typed_boundary", |p| {
    let f0 = p.failures().len();

    // Composition guard: a tagged measurement literal flows through
    // parse into admission, where the string-carrier boundary applies
    // (04 §4.3: uncertainty recorded loudly; the value refuses at the
    // Phase-1 string/measurement world with its named code).
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(
        "prob-info-surface",
        "emath function f:\n    definitions:\n        k = 0.30 ± 0.12 ~ lognormal\n",
    );
    // The boundary code family is admission's; the pin is that parse
    // produced NO E-SYN-* errors (the surface is stable).
    let syn_errors: Vec<&str> = checked
        .diagnostics
        .errors()
        .filter(|error| error.code.starts_with("E-SYN"))
        .map(|error| error.code)
        .collect();
    p.demand("1",syn_errors.is_empty(), format!(
        "tagged measurement must parse clean, got {syn_errors:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
