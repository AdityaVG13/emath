//! L3 contracted-component surface (bead emath-l3-contracted-component-ceus7).
//!
//! Failure-first pins for the canonical L3 declaration shape:
//! `emath <kind> Name:` + inputs/outputs/definitions/goals section blocks,
//! plus the optional `examples:` section surviving the parser.

use emath_core::tree::Item;
use emath_syntax::parse_str;

fn count_statements(source: &str, section: &str) -> Option<usize> {
    let (tree, diags) = parse_str(source);
    assert!(
        diags.items().is_empty(),
        "minimal L3 source must parse cleanly, got {diags:?}"
    );
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) => declaration
            .sections()
            .find(|s| s.name == section)
            .map(|s| s.suite.statements.len()),
        _ => None,
    })
}

#[test]
fn l3_contracted_component_parses() {
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
    assert_eq!(count_statements(source, "inputs"), Some(1));
    assert_eq!(count_statements(source, "outputs"), Some(1));
    assert_eq!(count_statements(source, "definitions"), Some(1));
    assert_eq!(count_statements(source, "goals"), Some(1));
}

fn find_square(
    tree: &emath_core::tree::SyntaxTree,
) -> Option<&emath_core::tree::Declaration> {
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) if declaration.name == "Square" => {
            Some(declaration)
        }
        _ => None,
    })
}

#[test]
fn l3_optional_sections_parse() {
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
    assert_eq!(
        count_statements(source, "examples"),
        Some(1),
        "optional `examples:` section must survive parsing"
    );
}

/// Pass 4: L2 named shorthand must expand into canonical L3 text — the
/// expanded source must parse, carry the canonical section set, and match
/// a hand-written contracted component structurally (same sections, same
/// definition names).
#[test]
fn l2_expand_matches_handwritten_l3() {
    use emath_syntax::expand_scratch;

    let l2 = "emath function Square:\n    area = side * side\n";
    let expansion = expand_scratch(l2);
    assert!(
        expansion.rewritten(),
        "L2 shorthand must rewrite, got level {:?}",
        expansion.level()
    );
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
    assert!(
        hand_diags.items().is_empty(),
        "hand-written L3 baseline must parse cleanly, got {hand_diags:?}"
    );

    let (exp_tree, exp_diags) = parse_str(expanded);
    assert!(
        exp_diags.items().is_empty(),
        "expanded L2 output must parse cleanly, got {exp_diags:?}; expanded:\n{expanded}"
    );

    let hand_decl = find_square(&hand_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in baseline"));
    let exp_decl = find_square(&exp_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in expanded output:\n{expanded}"));

    // Structural equivalence: same section set, same definition-statement
    // kinds, same count of definitions.
    let hand_sections: Vec<_> = hand_decl.sections().map(|s| s.name.clone()).collect();
    let exp_sections: Vec<_> = exp_decl.sections().map(|s| s.name.clone()).collect();
    assert_eq!(hand_sections, exp_sections, "canonical section set");

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
    assert_eq!(hand_defs, 1, "hand-written baseline: one definition");
    assert_eq!(hand_defs, exp_defs, "definition counts must match");
}

// --- Pass 5: L3 section-semantics rules (R5/R6/R4 + evidence) ---

fn check_codes(source: &str) -> Vec<String> {
    emath_syntax::install_source_parser();
    let mut session = emath_sema::session::CompilerSession::new(emath_core::limits::Limits::default());
    session
        .check_owned("pass5", source)
        .diagnostics
        .items()
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

#[test]
fn l3_outputs_without_inputs_rejected() {
    let source = "\
emath function Area:
    outputs:
        area: Float64

    goals:
        evaluate <area>:
            produce rust.library
";
    let codes = check_codes(source);
    assert!(
        codes.iter().any(|c| c.starts_with("E-SEC-130")),
        "contract mode with outputs: but no inputs: must refuse E-SEC-130, got {codes:?}"
    );
}

#[test]
fn l3_outputs_without_inputs_with_hole_allowed() {
    let source = "\
emath function Area:
    helper = ?

    outputs:
        area: Float64
";
    let codes = check_codes(source);
    assert!(
        !codes.iter().any(|c| c.starts_with("E-SEC-130")),
        "a declared hole is the unknown; no E-SEC-130, got {codes:?}"
    );
}

#[test]
fn l3_input_output_name_clash_rejected() {
    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        side: Float64
";
    let codes = check_codes(source);
    assert!(
        codes.iter().any(|c| c.starts_with("E-NAME-020")),
        "same name in inputs: and outputs: must refuse E-NAME-020, got {codes:?}"
    );
}

#[test]
fn l3_missing_goals_warns() {
    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64
";
    let codes = check_codes(source);
    assert!(
        codes.iter().any(|c| c.starts_with("E-SEC-133")),
        "contract mode without goals: must warn E-SEC-133, got {codes:?}"
    );
}

/// Phase-1 goals grammar accepts only operational verbs (evaluate,
/// differentiate, benchmark, fit, simplify) — none asserts truth, so none
/// requires `evidence:`. Regression pin: operational goals must NOT trip
/// E-EV-140. The rule activates only for assertion verbs (`prove`), which
/// the goals grammar does not accept yet.
#[test]
fn l3_operational_goals_need_no_evidence() {
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
    assert!(
        problems.is_empty(),
        "well-formed operational goal must not error and must not demand evidence, got {codes:?}"
    );
}

#[test]
fn l3_definition_shadowing_input_rejected() {
    let source = "\
emath function Area:
    inputs:
        side: Float64

    definitions:
        side = 3
";
    let codes = check_codes(source);
    assert!(
        codes.iter().any(|c| c.starts_with("E-NAME-020")),
        "definitions: shadowing an inputs: name must refuse E-NAME-020, got {codes:?}"
    );
}

#[test]
fn l3_examples_section_passes_check() {
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
    assert!(
        !codes.iter().any(|c| c.starts_with("E-SEC-101")),
        "optional `examples:` section must pass the Phase 1 gate, got {codes:?}"
    );
}

#[test]
fn l3_full_contract_no_errors() {
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
    assert!(
        codes.is_empty(),
        "happy-path L3 contract must admit cleanly, got {codes:?}"
    );
}

