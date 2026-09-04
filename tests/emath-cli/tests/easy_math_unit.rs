use emath_syntax::{expand_scratch, parse_str};

#[test]
fn one_line_solve_and_plot_desugar_to_admitted_meaning() {
    for source in ["solve x^2 = 2 over Real\n", "plot sin(x) on -3.14..3.14\n"] {
        let expansion = expand_scratch(source);
        assert!(expansion.rewritten(), "{source}");
        assert!(
            !expansion.diagnostics.has_errors(),
            "{source}: {:?}",
            expansion.diagnostics
        );
        let (direct, direct_diagnostics) = parse_str(source);
        let (expanded, expanded_diagnostics) = parse_str(&expansion.expanded);
        assert!(
            !direct_diagnostics.has_errors(),
            "{source}: {direct_diagnostics:?}"
        );
        assert!(!expanded_diagnostics.has_errors(), "{}", expansion.expanded);
        assert_eq!(
            direct, expanded,
            "scratch and explicit forms must admit identically"
        );
    }
}
