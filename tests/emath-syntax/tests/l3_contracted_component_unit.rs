//! L3 contracted-component surface.
//!
//! Failure-first pins for the canonical L3 declaration shape:
//! `emath <kind> Name:` + inputs/outputs/definitions/goals section blocks,
//! plus the optional `examples:` section surviving the parser.

use emath_core::tree::Item;
use emath_syntax::parse_str;

fn count_statements(p: &mut Probe, source: &str, section: &str) -> Option<usize> {
    let (tree, diags) = parse_str(source);
    p.demand(
        "parse",
        diags.items().is_empty(),
        format!("minimal L3 source must parse cleanly, got {diags:?}"),
    );
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) => declaration
            .sections()
            .find(|s| s.name == section)
            .map(|s| s.suite.statements.len()),
        _ => None,
    })
}

fn find_square(tree: &emath_core::tree::SyntaxTree) -> Option<&emath_core::tree::Declaration> {
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) if declaration.name == "Square" => Some(declaration),
        _ => None,
    })
}

// --- L3 section-semantics rules (R5/R6/R4 + evidence) ---

fn check_codes(source: &str) -> Vec<String> {
    let mut session =
        emath_sema::session::CompilerSession::new(emath_core::limits::Limits::default());
    session
        .check_owned("component-unit", source)
        .diagnostics
        .items()
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

use emath_test_harness::{Probe, boot};

#[test]
fn l3_contracted_component_unit() {
    boot();
    let mut probe = Probe::new("L3 contracted-component surface. Failure-first pins for the canonical L3 declaration shape: `emath <kind> Name:` + inputs/outputs/definitions/goals");
    probe.case("l3_contracted_component_parses", |p| {

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    goals:
        evaluate <area>:
            produce rust.library
";
    let _cs_1 = count_statements(p, source, "inputs");
        p.eq("1", _cs_1, Some(1));
    let _cs_2 = count_statements(p, source, "outputs");
        p.eq("2", _cs_2, Some(1));
    let _cs_3 = count_statements(p, source, "definitions");
        p.eq("3", _cs_3, Some(1));
    let _cs_4 = count_statements(p, source, "goals");
        p.eq("4", _cs_4, Some(1));

    });
    probe.case("l3_optional_sections_parse", |p| {

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    examples:
        area = 9.0
";
    let _cs_1 = count_statements(p, source, "examples");
        p.eq("1", _cs_1, Some(1));

    });
    probe.case("l2_expand_matches_handwritten_l3", |p| {
    // L2 named shorthand must expand into canonical L3 text — the
    // expanded source must parse, carry the canonical section set, and match
    // a hand-written contracted component structurally (same sections, same
    // definition names).
    let f0 = p.failures().len();

    use emath_syntax::expand_scratch;

    let l2 = "emath function Square:\n    area = side * side\n";
    let expansion = expand_scratch(l2);
    p.demand("1",expansion.rewritten(), format!(
        "L2 shorthand must rewrite, got level {:?}",
        expansion.level()
    ));
    if p.failures().len() != f0 { return; }
    let expanded = &expansion.expanded;

    let handwritten = "\
emath function Square:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side
";
    let (hand_tree, hand_diags) = parse_str(handwritten);
    p.demand("2",hand_diags.items().is_empty(), format!(
        "hand-written L3 baseline must parse cleanly, got {hand_diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    let (exp_tree, exp_diags) = parse_str(expanded);
    p.demand("3",exp_diags.items().is_empty(), format!(
        "expanded L2 output must parse cleanly, got {exp_diags:?}; expanded:\n{expanded}"
    ));
    if p.failures().len() != f0 { return; }

    let hand_decl = find_square(&hand_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in baseline"));
    let exp_decl = find_square(&exp_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in expanded output:\n{expanded}"));

    // Structural equivalence: same section set, same definition-statement
    // kinds, same count of definitions.
    let hand_sections: Vec<_> = hand_decl.sections().map(|s| s.name.clone()).collect();
    let exp_sections: Vec<_> = exp_decl.sections().map(|s| s.name.clone()).collect();
    p.eq("4", &hand_sections, &exp_sections);

    let hand_defs = hand_decl
        .sections()
        .find(|s| s.name == "definitions")
        .map(|s| s.suite.statements.len())
        .unwrap_or(0);
    let exp_defs = exp_decl
        .sections()
        .find(|s| s.name == "definitions")
        .map(|s| s.suite.statements.len())
        .unwrap_or(0);
    p.eq("5", hand_defs, 1);
    p.eq("6", &hand_defs, &exp_defs);

    });
    probe.case("l3_outputs_without_inputs_rejected", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    outputs:
        area: Float64

    goals:
        evaluate <area>:
            produce rust.library
";
    let codes = check_codes(source);
    p.demand("1",codes.iter().any(|c| c.starts_with("E-SEC-130")), format!(
        "contract mode with outputs: but no inputs: must refuse E-SEC-130, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_outputs_without_inputs_with_hole_allowed", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    helper = ?

    outputs:
        area: Float64
";
    let codes = check_codes(source);
    p.demand("1",!codes.iter().any(|c| c.starts_with("E-SEC-130")), format!(
        "a declared hole is the unknown; no E-SEC-130, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_input_output_name_clash_rejected", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        side: Float64
";
    let codes = check_codes(source);
    p.demand("1",codes.iter().any(|c| c.starts_with("E-NAME-020")), format!(
        "same name in inputs: and outputs: must refuse E-NAME-020, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_missing_goals_warns", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64
";
    let codes = check_codes(source);
    p.demand("1",codes.iter().any(|c| c.starts_with("E-SEC-133")), format!(
        "contract mode without goals: must warn E-SEC-133, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_operational_goals_need_no_evidence", |p| {
    // Phase-1 goals grammar accepts only operational verbs (evaluate,
    // differentiate, benchmark, fit, simplify) — none asserts truth, so none
    // requires `evidence:`. Regression pin: operational goals must NOT trip
    // E-EV-140. The rule activates only for assertion verbs (`prove`), which
    // the goals grammar does not accept yet.
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    goals:
        differentiate <area>:
            wrt [side]
";
    let codes = check_codes(source);
    let problems: Vec<_> = codes
        .iter()
        .filter(|c| c.starts_with("E-GOAL") || c.starts_with("E-EV-140"))
        .collect();
    p.demand("1",problems.is_empty(), format!(
        "well-formed operational goal must not error and must not demand evidence, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_definition_shadowing_input_rejected", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    definitions:
        side = 3
";
    let codes = check_codes(source);
    p.demand("1",codes.iter().any(|c| c.starts_with("E-NAME-020")), format!(
        "definitions: shadowing an inputs: name must refuse E-NAME-020, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_examples_section_passes_check", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    examples:
        area = 9.0
";
    let codes = check_codes(source);
    p.demand("1",!codes.iter().any(|c| c.starts_with("E-SEC-101")), format!(
        "optional `examples:` section must pass the Phase 1 gate, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("l3_full_contract_no_errors", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    goals:
        evaluate <area>:
            produce rust.library
";
    let codes = check_codes(source);
    p.demand("1",codes.is_empty(), format!(
        "happy-path L3 contract must admit cleanly, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
